#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image: docker pull $IMAGE"
[[ -f "$SNAPSHOT/config.json" ]] || die "missing the weights: hf download $REPO --revision $REVISION"

docker rm -f "$NAME" >/dev/null 2>&1 || true
apps=$(nvidia-smi --query-compute-apps=name --format=csv,noheader 2>/dev/null | sed 's/.*\///' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."
free=$(awk '/MemAvailable/{printf "%d", $2/1048576}' /proc/meminfo)
(( free >= MIN_FREE_GB )) || die "only ${free} GB of memory free, need ${MIN_FREE_GB}. Stop other models first."

mkdir -p "$LOGS" "$CACHE"/{triton,b12x,flashinfer,vllm}
# The Hugging Face cache that really holds the model (the model folder may be a link into another cache).
hf=$(dirname "$(dirname "$(realpath "$HUB/models--${REPO//\//--}")")")
MOUNTS=(-v "$hf:/root/.cache/huggingface:ro"
  -v "$CACHE/triton:/root/.triton" -v "$CACHE/b12x:/root/.cache/b12x" -v "$CACHE/flashinfer:/root/.cache/flashinfer" -v "$CACHE/vllm:/root/.cache/vllm")

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  # docker reads the value from its own environment: the key never shows in the process list.
  VLLM_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export VLLM_API_KEY
  KEY_ENV=(-e VLLM_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

ARGS=(serve "$REPO" --revision "$REVISION" --served-model-name "$SERVED_NAME" --host "$HOST" --port "$PORT"
  --tensor-parallel-size 1 --pipeline-parallel-size 1
  --max-model-len "$CONTEXT" --max-num-seqs "$PARALLEL" --gpu-memory-utilization "$GPU_MEM"
  --quantization modelopt_mixed --dtype bfloat16 --kv-cache-dtype fp8 --load-format b12x
  --moe-backend b12x --linear-backend b12x --gdn-decode-kernel b12x --no-enable-flashinfer-autotune
  --block-size 16 --enable-chunked-prefill --enable-prefix-caching --max-num-batched-tokens 4096
  --mamba-cache-mode align --mm-encoder-tp-mode data
  --compilation-config '{"pass_config":{"fuse_act_quant":true}}'
  --speculative-config "{\"method\":\"mtp\",\"num_speculative_tokens\":$SPEC_TOKENS}"
  --reasoning-parser qwen3 --enable-auto-tool-choice --tool-call-parser qwen3_xml
  --default-chat-template-kwargs "{\"enable_thinking\": true, \"preserve_thinking\": true, \"reasoning_effort\": \"$EFFORT\"}")

log "Starting $REPO @ ${REVISION:0:7}: $CONTEXT context, up to $PARALLEL requests at once, MTP $SPEC_TOKENS"
docker run -d --name "$NAME" --init --gpus all --ipc=host --network host --ulimit memlock=-1 --ulimit stack=67108864 \
  --security-opt seccomp=unconfined \
  --env-file engine.env "${KEY_ENV[@]}" "${MOUNTS[@]}" --entrypoint vllm "$IMAGE" "${ARGS[@]}" "$@" >/dev/null
# seccomp=unconfined: Docker's default profile blocks io_uring, which streams the PLE tables from disk.
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"
log "The first start compiles kernels and can take 10-15 minutes; later starts reuse them."

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "vLLM exited with code $code"
./stop.sh
exit 1
