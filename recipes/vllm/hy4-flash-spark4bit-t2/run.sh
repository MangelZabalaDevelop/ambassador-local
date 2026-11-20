#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
for f in "$GGUF" "$MMPROJ"; do
  [[ -f "$SNAPSHOT/$f" ]] || die "missing the weights: hf download $REPO $GGUF $MMPROJ --revision $REVISION"
done
bash ./build.sh

docker rm -f "$NAME" >/dev/null 2>&1 || true
apps=$(nvidia-smi -i "$GPU" --query-compute-apps=name,used_memory --format=csv,noheader,nounits 2>/dev/null | awk -F', ' '$NF !~ /^[0-9]+$/ || $NF >= 2048 {sub(/, [^,]*$/, ""); sub(/.*\//, ""); print}' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."

if [[ "$CONTEXT" == auto ]]; then
  read -r total used < <(nvidia-smi -i "$GPU" --query-gpu=memory.total,memory.used --format=csv,noheader,nounits | tr -d ,)
  free=$(( total - used )); CONTEXT=
  for fit in "${CONTEXT_FIT[@]}"; do
    (( free >= ${fit#*:} )) && { CONTEXT=${fit%%:*}; break; }
  done
  [[ -n "$CONTEXT" ]] || die "only $free MiB of GPU memory free, need ${CONTEXT_FIT[-1]#*:}. Close the apps that use the GPU."
  log "$free MiB of GPU memory free: $CONTEXT tokens of context"
fi

mkdir -p "$LOGS"
MODEL_HUB=$(dirname "$(realpath "$HUB/models--${REPO//\//--}")")

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  LLAMA_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export LLAMA_API_KEY
  KEY_ENV=(-e LLAMA_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

ARGS=(--model "$SNAPSHOT/$GGUF" --mmproj "$SNAPSHOT/$MMPROJ" --mmproj-offload
  --alias "$SERVED_NAME" --host "$HOST" --port "$PORT"
  --ctx-size "$CONTEXT" --parallel 1 --n-gpu-layers 999 --split-mode none
  --override-tensor '^per_layer_token_embd\.weight$=CPU,^token_embd\.weight$=CUDA0'
  --load-mode mmap --lazy-mode on --no-host --fit off --flash-attn on
  --batch-size 2048 --ubatch-size 1024 --threads 4 --threads-batch 4
  --cache-type-k f16 --cache-type-v f16
  --spec-type draft-mtp --spec-draft-n-max "$DRAFT"
  --metrics --jinja)

log "Starting $REPO $GGUF @ ${REVISION:0:7} on GPU $GPU: $CONTEXT context, one request at a time, MTP $DRAFT"
docker run -d --name "$NAME" --init --gpus "device=$GPU" --network host --ulimit memlock=-1 \
  --env-file engine.env "${KEY_ENV[@]}" -v "$MODEL_HUB:$MODEL_HUB:ro" \
  "$FIXED_IMAGE" "${ARGS[@]}" "$@" >/dev/null
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "llama-server exited with code $code"
./stop.sh
exit 1
