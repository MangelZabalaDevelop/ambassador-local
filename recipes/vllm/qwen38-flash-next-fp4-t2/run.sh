#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")"; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image: docker pull $IMAGE"
[[ -f "$SNAPSHOT/config.json" ]] || die "missing the weights: hf download $REPO --revision $REVISION"

docker rm -f "$NAME" >/dev/null 2>&1 || true
apps=$(nvidia-smi --query-compute-apps=name --format=csv,noheader 2>/dev/null | sed 's/.*\///' | sort -u | paste -sd, -)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."

mkdir -p "$LOGS" "$CACHE"/{triton,b12x,flashinfer,vllm}
# The Hugging Face cache that really holds the model (the model folder may be a link into another cache).
hf=$(dirname "$(dirname "$(realpath "$HUB/models--${REPO//\//--}")")")
MOUNTS=(-v "$hf:/root/.cache/huggingface:ro"
  -v "$CACHE/triton:/root/.triton" -v "$CACHE/b12x:/root/.cache/b12x" -v "$CACHE/flashinfer:/root/.cache/flashinfer" -v "$CACHE/vllm:/root/.cache/vllm")

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  KEY_ENV=(--env-file <(printf 'VLLM_API_KEY=%s\n' "$(tr -d '\r\n' < "$API_KEY_FILE")"))
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

ARGS=(serve "$REPO" --revision "$REVISION" --served-model-name "$SERVED_NAME" --host "$HOST" --port "$PORT"
  --max-model-len "$CONTEXT" --max-num-seqs "$PARALLEL" --gpu-memory-utilization "$GPU_MEM" --kv-offloading-size "$KV_OFFLOAD_GB"
  --quantization modelopt_mixed --dtype bfloat16 --kv-cache-dtype auto --load-format instanttensor
  --moe-backend b12x --linear-backend b12x --gdn-decode-kernel b12x --no-enable-flashinfer-autotune
  --tensor-parallel-size 1 --pipeline-parallel-size 1 --decode-context-parallel-size 1
  --async-scheduling --block-size 64 --enable-chunked-prefill --enable-prefix-caching --enable-prompt-tokens-details
  --max-num-batched-tokens 6019 --long-prefill-token-threshold 2048 --max-cudagraph-capture-size 64
  --compilation-config '{"cudagraph_mode":"FULL_AND_PIECEWISE","custom_ops":["all"]}'
  --mamba-cache-mode align --mamba-ssm-cache-dtype auto --recurrent-checkpoint-policy aligned
  --no-language-model-only --mm-encoder-tp-mode data --mm-processor-cache-gb 0.0
  --speculative-config "{\"method\":\"mtp\",\"draft_sample_method\":\"probabilistic\",\"rejection_sample_method\":\"standard\",\"moe_backend\":\"b12x\",\"num_speculative_tokens\":$SPEC_TOKENS}"
  --reasoning-parser qwen3 --enable-auto-tool-choice --tool-call-parser qwen3_xml
  --default-chat-template-kwargs "{\"enable_thinking\": true, \"preserve_thinking\": true, \"reasoning_effort\": \"$EFFORT\"}")

log "Starting $REPO @ ${REVISION:0:7}: $CONTEXT context, up to $PARALLEL requests at once, MTP $SPEC_TOKENS"
docker run -d --name "$NAME" --init --gpus all --ipc=host --network host --ulimit memlock=-1 --ulimit stack=67108864 \
  --env-file engine.env "${KEY_ENV[@]}" "${MOUNTS[@]}" --entrypoint /opt/venv/bin/vllm "$IMAGE" "${ARGS[@]}" "$@" >/dev/null
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "vLLM exited with code $code"
./stop.sh
exit 1
