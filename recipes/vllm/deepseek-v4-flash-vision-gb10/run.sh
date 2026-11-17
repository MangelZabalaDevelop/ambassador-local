#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1 (text and images), ready when http://127.0.0.1:$PORT/health answers.
# The first start builds the fixed engine image (~1 min), downloads the recipe source (~3 MB) and compiles kernels.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh
HERE=$PWD

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
[[ -f "$SNAPSHOT/tp1/rank-sliced-tp1-manifest.json" ]] || die "missing the weights: hf download $REPO --revision $REVISION"
command -v python3 >/dev/null || die "python3 is required (the bad_words proxy)"

"$HERE/stop.sh" >/dev/null 2>&1 || true
apps=$(nvidia-smi --query-compute-apps=name --format=csv,noheader 2>/dev/null | sed 's/.*\///' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."
free=$(awk '/MemAvailable/{printf "%d", $2/1048576}' /proc/meminfo)
(( free >= MIN_FREE_GB )) || die "only ${free} GB of memory free, need ${MIN_FREE_GB}. Stop other models first."
mkdir -p "$LOGS" "$WORK"

# 1. Engine image: the pinned base plus the xgrammar fix, built once.
if ! docker image inspect "$FIXED_IMAGE" >/dev/null 2>&1; then
  docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image: docker pull $IMAGE"
  log "Building $FIXED_IMAGE from the pinned image (once)"
  docker build -q --build-arg BASE_IMAGE="$IMAGE" -t "$FIXED_IMAGE" -f "$HERE/Dockerfile" "$HERE" >/dev/null
fi

# 2. Recipe source at its pinned commit: the vision overlay, the launcher, the proxy and ramwatch.
if [[ ! -f "$SRC/scripts/serve.sh" ]]; then
  log "Downloading $SRC_REPO @ ${SRC_COMMIT:0:7}"
  rm -rf "$SRC.tmp"; mkdir -p "$SRC.tmp"
  curl -fsSL "https://codeload.github.com/$SRC_REPO/tar.gz/$SRC_COMMIT" | tar -xz -C "$SRC.tmp" --strip-components=1
  mv "$SRC.tmp" "$SRC"
fi

# 3. The pack as the engine wants it (tp1/, dspark-draft-k64/): hard links into the Hugging Face cache, no copy.
if [[ ! -f "$MODEL/tp1/rank-sliced-tp1-manifest.json" || "$(cat "$MODEL/.revision" 2>/dev/null)" != "$REVISION" ]]; then
  log "Linking the weights into $MODEL"
  rm -rf "$MODEL.tmp"; cp -rlL "$SNAPSHOT" "$MODEL.tmp"
  echo "$REVISION" > "$MODEL.tmp/.revision"
  rm -rf "$MODEL"; mv "$MODEL.tmp" "$MODEL"
fi
mkdir -p "$WORK/cache" "$WORK/hf-empty"

# 4. The engine, as the recipe's scripts/serve.sh runs it, on 127.0.0.1 only.
BASE="$SRC/third_party/MiaAI-Lab-DeepSeek-v4-Flash-One-DGX-Spark/image-patch"
OVERLAY="$SRC/overlay/dsvision"
MOUNTS=(-v "$MODEL:/models" -v "$WORK/cache:/cache" -v "$WORK/hf-empty:/hf-cache"
  -v "$BASE/entrypoint-256k.sh:/opt/recipe/scripts/entrypoint.sh:ro"
  -v "$BASE/serve-ds4-flash.sh:/opt/vllm/serve-ds4-flash.sh:ro"
  -v "$BASE/coalesce_rank_sliced_exl3.py:/opt/recipe/scripts/coalesce_rank_sliced_exl3.py:ro"
  -v "$BASE/vllm/models/deepseek_v4/nvidia/dspark.py:/opt/vllm/vllm/models/deepseek_v4/nvidia/dspark.py:ro"
  -v "$BASE/sparkinfer/moe/_shared/kernels/tiny_decode.py:/opt/sparkinfer/sparkinfer/moe/_shared/kernels/tiny_decode.py:ro"
  -v "$BASE/sparkinfer/attention/_shared/mla/prefill.py:/opt/sparkinfer/sparkinfer/attention/_shared/mla/prefill.py:ro"
  -v "$BASE/sparkinfer/attention/_shared/mla/prefill_mg.py:/opt/sparkinfer/sparkinfer/attention/_shared/mla/prefill_mg.py:ro")
while IFS= read -r f; do MOUNTS+=(-v "$OVERLAY/$f:/opt/sparkinfer/$f:ro"); done < <(cd "$OVERLAY" && find sparkinfer -name '*.py' | sort)
while IFS= read -r f; do MOUNTS+=(-v "$OVERLAY/$f:/opt/vllm/$f:ro"); done < <(cd "$OVERLAY" && find vllm -name '*.py' | sort)

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  # docker reads the value from its own environment: the key never shows in the process list.
  VLLM_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export VLLM_API_KEY
  KEY_ENV=(-e VLLM_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

log "Starting $REPO @ ${REVISION:0:7}: $CONTEXT context, one request at a time, DSpark $SPEC_TOKENS"
docker run -d --name "$NAME" --init --gpus all --ipc host --network host --shm-size 16g \
  --env-file engine.env "${KEY_ENV[@]}" -e HOST=127.0.0.1 -e PORT="$ENGINE_PORT" -e SERVED_MODEL_NAME="$SERVED_NAME" \
  -e MAX_MODEL_LEN="$CONTEXT" -e GPU_MEMORY_UTILIZATION="$UTIL" -e DSPARK_TOKENS="$SPEC_TOKENS" \
  -e DEFAULT_CHAT_TEMPLATE_KWARGS_EFFORT="$EFFORT" \
  "${MOUNTS[@]}" --entrypoint /bin/bash "$FIXED_IMAGE" -lc 'exec /opt/recipe/scripts/entrypoint.sh' >/dev/null

# 5. ramwatch stops the engine before unified memory runs out (a GB10 can freeze instead of failing an allocation);
#    the proxy adds bad_words to every request against a token the decode path invents (")Skip", see the README).
setsid nohup "$SRC/scripts/ramwatch.sh" "$NAME" "$RAMWATCH_FLOOR_MB" >> "$LOGS/ramwatch.log" 2>&1 < /dev/null &
echo $! > "$WORK/ramwatch.pid"
setsid nohup python3 "$SRC/scripts/badwords_proxy.py" --host "$HOST" --listen "$PORT" \
  --upstream "http://127.0.0.1:$ENGINE_PORT" >> "$LOGS/proxy.log" 2>&1 < /dev/null &
echo $! > "$WORK/proxy.pid"
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"
log "The first start compiles kernels and can take 25-30 minutes; later starts take about 2."

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "vLLM exited with code $code"
./stop.sh
exit 1
