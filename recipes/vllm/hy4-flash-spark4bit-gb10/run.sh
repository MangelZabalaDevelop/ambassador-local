#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine's base image: docker pull $IMAGE"
for f in "$GGUF" "$MMPROJ"; do
  [[ -f "$SNAPSHOT/$f" ]] || die "missing the weights: hf download $REPO $GGUF $MMPROJ --revision $REVISION"
done

if ! docker image inspect "$FIXED_IMAGE" >/dev/null 2>&1; then
  log "Building $FIXED_IMAGE (Tencent's llama.cpp for Hy4-flash, about 4 minutes the first time)"
  docker build -q --build-arg BASE_IMAGE="$IMAGE" -t "$FIXED_IMAGE" - < Dockerfile >/dev/null || die "could not build $FIXED_IMAGE"
fi

docker rm -f "$NAME" >/dev/null 2>&1 || true
pkill -f "[m]emwatch.sh $NAME" 2>/dev/null || true
apps=$(nvidia-smi --query-compute-apps=name --format=csv,noheader 2>/dev/null | sed 's/.*\///' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."
free=$(awk '/MemAvailable/{printf "%d", $2/1048576}' /proc/meminfo)
(( free >= MIN_FREE_GB )) || die "only ${free} GB of memory free, need ${MIN_FREE_GB}. Stop other models first."

mkdir -p "$LOGS"
# The model folder may be a link into another Hugging Face cache: mount the cache that really holds it, at the same
# path inside the container, so the snapshot's links into blobs/ still resolve.
MODEL_HUB=$(dirname "$(realpath "$HUB/models--${REPO//\//--}")")

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  # docker reads the value from its own environment: the key never shows in the process list.
  LLAMA_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export LLAMA_API_KEY
  KEY_ENV=(-e LLAMA_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

# Tencent's server command for the GB10. The Engram tables (per_layer_token_embd, 63.5B parameters) stay on disk and
# are read on demand (--lazy-mode on); the rest of the model goes to the GPU.
ARGS=(--model "$SNAPSHOT/$GGUF" --mmproj "$SNAPSHOT/$MMPROJ" --mmproj-offload
  --alias "$SERVED_NAME" --host "$HOST" --port "$PORT"
  --ctx-size "$CONTEXT" --parallel 1 --n-gpu-layers 999 --split-mode none
  --override-tensor '^per_layer_token_embd\.weight$=CPU,^token_embd\.weight$=CUDA0'
  --load-mode mmap --lazy-mode on --no-host --fit off --flash-attn on
  --batch-size 2048 --ubatch-size 1024 --threads 4 --threads-batch 4
  --cache-type-k f16 --cache-type-v f16
  --spec-type draft-mtp --spec-draft-n-max "$DRAFT"
  --metrics --jinja)

log "Starting $REPO $GGUF @ ${REVISION:0:7}: $CONTEXT context, one request at a time, MTP $DRAFT"
docker run -d --name "$NAME" --init --gpus all --network host --ulimit memlock=-1 \
  --env-file engine.env -e CUDA_VISIBLE_DEVICES=0 "${KEY_ENV[@]}" -v "$MODEL_HUB:$MODEL_HUB:ro" \
  "$FIXED_IMAGE" "${ARGS[@]}" "$@" >/dev/null
setsid nohup bash "$PWD/memwatch.sh" "$NAME" >> "$LOGS/memwatch.log" 2>&1 < /dev/null &
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "llama-server exited with code $code"
./stop.sh
exit 1
