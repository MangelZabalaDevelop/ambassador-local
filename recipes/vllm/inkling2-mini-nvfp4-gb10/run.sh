#!/bin/bash
# run.sh: starts the model and stays in the foreground with its log; Ctrl+C stops it.
# OpenAI-compatible API on http://$HOST:$PORT/v1, ready when http://127.0.0.1:$PORT/health answers.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image: docker pull $IMAGE"
[[ -f "$SNAPSHOT/config.json" ]] || die "missing the weights: hf download $REPO --revision $REVISION"

docker rm -f "$NAME" >/dev/null 2>&1 || true
# Small GPU users don't count (the desktop, remote desktop's ~300 MiB): only 2 GiB or more, or an unknown size.
apps=$(nvidia-smi --query-compute-apps=name,used_memory --format=csv,noheader,nounits 2>/dev/null | awk -F', ' '$NF !~ /^[0-9]+$/ || $NF >= 2048 {sub(/, [^,]*$/, ""); sub(/.*\//, ""); print}' | sort -u | paste -sd, - || true)
[[ -z "$apps" ]] || die "the GPU is in use by $apps. Stop it first."
free=$(awk '/MemAvailable/{printf "%d", $2/1048576}' /proc/meminfo)
(( free >= MIN_FREE_GB )) || die "only ${free} GB of memory free, need ${MIN_FREE_GB}. Stop other models first."

mkdir -p "$LOGS" "$CACHE"/{triton,sglang} "$WORK/run"
# The Hugging Face cache that really holds the model (the model folder may be a link into another cache).
hf=$(dirname "$(dirname "$(realpath "$HUB/models--${REPO//\//--}")")")
MOUNTS=(-v "$hf:/root/.cache/huggingface:ro" -v "$CACHE/triton:/root/.triton" -v "$CACHE/sglang:/root/.cache/sglang")

# SGLang takes its key only as an argument, so it goes in a private config file instead: never in the process list.
CONFIG=()
rm -f "$WORK/run/key.yaml"
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  key=$(tr -d '\r\n' < "$API_KEY_FILE")
  [[ "$key" =~ ^[A-Za-z0-9._~+/=-]+$ ]] || die "the key in $API_KEY_FILE has characters a config file can't hold"
  (umask 077; printf 'api-key: "%s"\n' "$key" > "$WORK/run/key.yaml")
  MOUNTS+=(-v "$WORK/run/key.yaml:/run/al/key.yaml:ro")
  CONFIG=(--config /run/al/key.yaml)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

ARGS=(serve "${CONFIG[@]}" --model-path "$REPO" --revision "$REVISION" --served-model-name "$SERVED_NAME"
  --host "$HOST" --port "$PORT"
  --tp 1 --ep 1 --attention-backend fa4 --dtype bfloat16 --page-size 128
  --context-length "$CONTEXT" --max-running-requests "$PARALLEL"
  --mem-fraction-static "$MEM_FRACTION" --swa-full-tokens-ratio "$SWA_RATIO"
  --speculative-algorithm DSPARK --speculative-num-draft-tokens "$DRAFT_TOKENS"
  --grammar-backend xgrammar --enable-multimodal
  --reasoning-parser inkling --tool-call-parser inkling2)

log "Starting $REPO @ ${REVISION:0:7}: $CONTEXT context, $PARALLEL request at a time, DSpark $DRAFT_TOKENS"
docker run -d --name "$NAME" --init --gpus all --ipc=host --shm-size 16g --network host --ulimit memlock=-1 \
  --env-file engine.env "${MOUNTS[@]}" "$IMAGE" sglang "${ARGS[@]}" "$@" >/dev/null
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"
log "The first start compiles kernels and takes about 5 minutes; later starts reuse them."

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
docker logs -f "$NAME" 2>&1 || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "SGLang exited with code $code"
./stop.sh
exit 1
