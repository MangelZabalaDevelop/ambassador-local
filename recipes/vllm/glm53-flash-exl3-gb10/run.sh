#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image: docker pull $IMAGE"
[[ -f "$SNAPSHOT/config.json" ]] || die "missing the weights: hf download $REPO --revision $REVISION"
[[ -f "$DRAFT_SNAPSHOT/config.json" ]] || die "missing the draft model (CC BY-NC-ND 4.0, read its license first):" \
  "hf download $DRAFT_REPO --revision $DRAFT_REVISION"

if ! docker image inspect "$FIXED_IMAGE" >/dev/null 2>&1; then
  log "Building $FIXED_IMAGE (adds the weight-loader fix, about a minute)"
  docker build -q --build-arg BASE_IMAGE="$IMAGE" -t "$FIXED_IMAGE" - < Dockerfile >/dev/null || die "could not build $FIXED_IMAGE"
fi

docker rm -f "$NAME" >/dev/null 2>&1 || true
# Small GPU users don't count (the desktop, remote desktop's ~300 MiB): only 2 GiB or more, or an unknown size.
apps=$(nvidia-smi --query-compute-apps=name,used_memory --format=csv,noheader,nounits 2>/dev/null | awk -F', ' '$NF !~ /^[0-9]+$/ || $NF >= 2048 {sub(/, [^,]*$/, ""); sub(/.*\//, ""); print}' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."
free=$(awk '/MemAvailable/{printf "%d", $2/1048576}' /proc/meminfo)
(( free >= MIN_FREE_GB )) || die "only ${free} GB of memory free, need ${MIN_FREE_GB}. Stop other models first."

mkdir -p "$LOGS" "$CACHE"/{vllm,triton,tilelang,torchinductor}
# Each model folder may be a link into another Hugging Face cache: mount the cache that really holds it, at the
# same path inside the container, so the snapshot's links into blobs/ still resolve.
real() { realpath "$HUB/models--${1//\//--}"; }
MODEL_DIR=$(real "$REPO")/snapshots/$REVISION
DRAFT_DIR=$(real "$DRAFT_REPO")/snapshots/$DRAFT_REVISION
MOUNTS=()
for hub in $(printf '%s\n' "$(dirname "$(real "$REPO")")" "$(dirname "$(real "$DRAFT_REPO")")" | sort -u); do
  MOUNTS+=(-v "$hub:$hub:ro")
done
MOUNTS+=(-v "$CACHE/vllm:/root/.cache/vllm" -v "$CACHE/triton:/root/.triton/cache"
  -v "$CACHE/tilelang:/root/.tilelang/cache" -v "$CACHE/torchinductor:/tmp/torchinductor_root")

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  # docker reads the value from its own environment: the key never shows in the process list.
  VLLM_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export VLLM_API_KEY
  KEY_ENV=(-e VLLM_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

SPEC="{\"method\":\"dflash\",\"model\":\"$DRAFT_DIR\",\"num_speculative_tokens\":$SPEC_TOKENS,\"kv_cache_dtype\":\"auto\",\"draft_sample_method\":\"probabilistic\",\"rejection_sample_method\":\"standard\",\"draft_tensor_parallel_size\":1}"
ARGS=(serve "$MODEL_DIR" --served-model-name "$SERVED_NAME" --host "$HOST" --port "$PORT"
  --tensor-parallel-size 1 --quantization exl3
  --max-model-len "$CONTEXT" --max-num-seqs "$PARALLEL" --gpu-memory-utilization "$GPU_MEM"
  --max-num-batched-tokens 7168 --kv-cache-dtype fp8
  --enable-prefix-caching --no-enable-flashinfer-autotune
  --speculative-config "$SPEC"
  --tool-call-parser glm47 --enable-auto-tool-choice --reasoning-parser glm45
  --chat-template /opt/glm53/chat_template.jinja
  --limit-mm-per-prompt '{"image":2,"video":0}' --skip-mm-profiling
  --cudagraph-capture-sizes 1 2 4 8 16 24 32)

log "Starting $REPO @ ${REVISION:0:7}: $CONTEXT context, up to $PARALLEL requests at once, DFlash2 $SPEC_TOKENS"
# The image's start step: the video-placeholder patch installs an import hook, so it must run in the container.
docker run -d --name "$NAME" --init --gpus all --ipc=host --network host --shm-size 32g --cap-add IPC_LOCK \
  --ulimit memlock=-1 --ulimit stack=67108864 --add-host "$(hostname):127.0.1.1" \
  --env-file engine.env "${KEY_ENV[@]}" "${MOUNTS[@]}" --entrypoint bash "$FIXED_IMAGE" \
  -c 'python3 /opt/glm53/patch_glm_video_placeholders.py && exec vllm "$@"' vllm "${ARGS[@]}" "$@" >/dev/null
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"
log "The first start compiles kernels and can take 10-15 minutes; later starts reuse them."

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "vLLM exited with code $code"
./stop.sh
exit 1
