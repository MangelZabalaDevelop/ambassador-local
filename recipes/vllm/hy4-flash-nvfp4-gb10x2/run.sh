#!/bin/bash
# run.sh: starts rank 1 on the other GB10 and rank 0 (the API) here, and stays in the foreground with rank 0's log.
# It stops both ranks as soon as either one dies, or on Ctrl+C. OpenAI-compatible API on http://$HOST:$PORT/v1,
# ready when http://127.0.0.1:$PORT/health answers. Extra arguments go to vLLM on rank 0.
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh
need_link
docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
w "docker info >/dev/null 2>&1" || die "Docker is not reachable without sudo on the other GB10 ($PEER)"
get="hf download $REPO --revision $REVISION"
[[ -f "$SNAPSHOT/config.json" ]] || die "missing the weights on this GB10: $get"
w "test -f $SNAPSHOT/config.json" || die "missing the weights on the other GB10 ($PEER): $get"

# The engine: vLLM's image plus six files (Dockerfile), built on each GB10 the first time (about a minute).
if ! docker image inspect "$FIXED_IMAGE" >/dev/null 2>&1; then
  docker image inspect "$IMAGE" >/dev/null 2>&1 || die "missing the engine image on this GB10: docker pull $IMAGE"
  log "Building $FIXED_IMAGE on this GB10"
  docker build -q --build-arg BASE_IMAGE="$IMAGE" -t "$FIXED_IMAGE" - < Dockerfile >/dev/null || die "could not build $FIXED_IMAGE"
fi
if ! w "docker image inspect $FIXED_IMAGE >/dev/null 2>&1"; then
  w "docker image inspect $IMAGE >/dev/null 2>&1" || die "missing the engine image on the other GB10 ($PEER): docker pull $IMAGE"
  log "Building $FIXED_IMAGE on the other GB10"
  wi "docker build -q --build-arg BASE_IMAGE=$IMAGE -t $FIXED_IMAGE - >/dev/null" < Dockerfile || die "could not build $FIXED_IMAGE on $PEER"
fi

# No other model may keep the GPU or the memory
docker rm -f "$NAME" >/dev/null 2>&1 || true
w "docker rm -f $NAME >/dev/null 2>&1; true"
pkill -f "[m]emwatch.sh $NAME" 2>/dev/null || true
w "pkill -f '[m]emwatch.sh $NAME'; true"
# Small GPU users don't count (the desktop, remote desktop's ~300 MiB): only 2 GiB or more, or an unknown size.
gpu_apps='nvidia-smi --query-compute-apps=name,used_memory --format=csv,noheader,nounits 2>/dev/null | awk -F", " "\$NF !~ /^[0-9]+\$/ || \$NF >= 2048 {sub(/, [^,]*\$/, \"\"); sub(/.*\//, \"\"); print}" | sort -u | paste -sd, -'
apps=$(bash -c "$gpu_apps"); [[ -z "$apps" ]] || die "the GPU of this GB10 is in use by $apps. Stop it first."
apps=$(w "$gpu_apps"); [[ -z "$apps" ]] || die "the GPU of the other GB10 ($PEER) is in use by $apps. Stop it first."
# On a GB10 a rank that runs out of memory freezes the machine instead of failing cleanly. Wait for the page cache.
for i in $(seq 1 60); do
  h=$(avail_gib); k=$(w "awk '/MemAvailable/ {printf \"%.1f\", \$2 / 1048576}' /proc/meminfo")
  awk -v a="$h" -v b="$k" -v m="$MIN_AVAIL_GIB" 'BEGIN {exit !(a >= m && b >= m)}' && break
  (( i == 60 )) && die "available memory: this GB10 $h GiB, other GB10 $k GiB; each needs $MIN_AVAIL_GIB. Stop other models first."
  sleep 2
done
log "Available memory: this GB10 $h GiB, other GB10 $k GiB"

mkdir -p "$LOGS" "$CACHE"
w "mkdir -p $LOGS $CACHE $WORK/recipe"
for f in config.sh memwatch.sh; do wi "cat > $WORK/recipe/$f" < "$f"; done   # the other GB10's memory brake

KEY_ENV=()
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  # docker reads the value from its own environment: the key never shows in the process list.
  VLLM_API_KEY=$(tr -d '\r\n' < "$API_KEY_FILE"); export VLLM_API_KEY
  KEY_ENV=(-e VLLM_API_KEY)
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

# The model folder may be a link into another Hugging Face cache: mount the cache that really holds it, at the same
# path inside the container, so the snapshot's links into blobs/ still resolve.
model=$(realpath "$HUB/models--${REPO//\//--}")
hub=$(dirname "$model")
# Ranks talk over the cable: NCCL over both RoCE rails, its bootstrap and Gloo on the cable's interface.
FLAGS=(--init --gpus all --ipc=host --network host --shm-size 16g --memory "$MEM_LIMIT"
  --ulimit memlock=-1 --ulimit stack=67108864 --ulimit nofile=1048576:1048576 --cap-add IPC_LOCK --device /dev/infiniband
  -v "$hub:$hub:ro" -v "$CACHE:/cache"
  -e HOME=/cache -e FLASHINFER_WORKSPACE_BASE=/cache -e MAX_JOBS=4 -e HF_HUB_OFFLINE=1 -e VLLM_ENGINE_READY_TIMEOUT_S=3600
  -e NCCL_IB_DISABLE=0 -e NCCL_IGNORE_CPU_AFFINITY=1 -e NCCL_IB_HCA="$HCAS"
  -e NCCL_SOCKET_IFNAME="$IFACE" -e GLOO_SOCKET_IFNAME="$IFACE" -e TP_SOCKET_IFNAME="$IFACE")
ARGS=(serve "$model/snapshots/$REVISION" --served-model-name "$SERVED_NAME"
  --tensor-parallel-size 2 --enable-expert-parallel --dtype bfloat16
  --max-model-len "$CONTEXT" --max-num-seqs "$PARALLEL" --max-num-batched-tokens 8192
  --kv-cache-memory-bytes "$KV_CACHE" --gpu-memory-utilization "$GPU_MEM"
  --speculative-config "{\"num_speculative_tokens\":$SPEC_TOKENS,\"method\":\"dfly\"}"
  --limit-mm-per-prompt '{"image":1,"video":0}'
  --reasoning-parser hy_v4 --tool-call-parser hy_v4 --enable-auto-tool-choice
  --nnodes 2 --master-addr "$MASTER_ADDR" --master-port "$MASTER_PORT")

log "Starting $REPO @ ${REVISION:0:7} on two GB10 (this one and $PEER): $CONTEXT context, up to $PARALLEL requests at once, DFly $SPEC_TOKENS"
log "Rank 1 (headless) on the other GB10 ($PEER)"
w "$(printf '%q ' docker run -d --name "$NAME" "${FLAGS[@]}" --entrypoint vllm "$FIXED_IMAGE" "${ARGS[@]}" --node-rank 1 --headless)" >/dev/null
w "setsid nohup bash $WORK/recipe/memwatch.sh $NAME >> $LOGS/memwatch.log 2>&1 < /dev/null &"
log "Rank 0 and the API on this GB10 ($MASTER_ADDR)"
docker run -d --name "$NAME" "${FLAGS[@]}" "${KEY_ENV[@]}" --entrypoint vllm "$FIXED_IMAGE" "${ARGS[@]}" \
  --node-rank 0 --host "$HOST" --port "$PORT" "$@" >/dev/null
setsid nohup bash "$PWD/memwatch.sh" "$NAME" >> "$LOGS/memwatch.log" 2>&1 < /dev/null &
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"
log "The first start compiles and tunes kernels and takes about 10 minutes; later starts reuse them."

trap 'log "stop requested"; ./stop.sh; exit 0' INT TERM
# Watch rank 1 while rank 0 writes its log in the foreground: a lone rank waits for its partner forever.
( while sleep 20; do
    docker ps -q -f "name=^${NAME}\$" | grep -q . || exit 0
    w true 2>/dev/null || continue
    w "docker ps -q -f name=^${NAME}\\\$" | grep -q . && continue
    log "rank 1 died on the other GB10: stopping rank 0"; w "docker logs --tail 30 $NAME" 2>&1 | sed 's/^/  [rank 1] /'
    docker stop -t 30 "$NAME" >/dev/null 2>&1; exit 1
  done ) &
watch=$!
docker logs -f "$NAME" 2>&1 || true
kill "$watch" 2>/dev/null || true
code=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo 1)
log "rank 0 exited with code $code"
./stop.sh
exit 1
