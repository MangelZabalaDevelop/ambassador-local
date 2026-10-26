#!/bin/bash
# run.sh: starts rank 1 on the other GB10 and rank 0 (the API) here, and stays in the foreground.
# It stops both ranks as soon as either one dies, or on Ctrl+C. OpenAI-compatible API on http://$HOST:$PORT/v1
set -euo pipefail
cd "$(dirname "$0")"; source ./config.sh
need_link
mkdir -p "$LOGS"
ready="$ENGINE_COMMIT $REVISION"
[[ "$(cat "$WORK/.ready" 2>/dev/null)" == "$ready" ]] || die "this GB10 is not prepared: run ./prepare.sh"
w "test \"\$(cat $WORK/.ready 2>/dev/null)\" = \"$ready\"" || die "the other GB10 ($PEER) is not prepared: run ./prepare.sh"

# No other model may keep the memory
docker rm -f "$NAME" >/dev/null 2>&1 || true
w "docker rm -f $NAME >/dev/null 2>&1; true"
pkill -f "[m]emwatch.sh $NAME" 2>/dev/null || true
w "pkill -f '[m]emwatch.sh $NAME'; true"

# Memory: on a GB10 a rank that runs out of it freezes the machine instead of failing cleanly. Wait for the page cache.
for i in $(seq 1 60); do
  h=$(avail_gib); k=$(w "awk '/MemAvailable/ {printf \"%.1f\", \$2 / 1048576}' /proc/meminfo")
  awk -v a="$h" -v b="$k" -v m="$MIN_AVAIL_GIB" 'BEGIN {exit !(a >= m && b >= m)}' && break
  (( i == 60 )) && die "available memory: this GB10 $h GiB, other GB10 $k GiB; each needs $MIN_AVAIL_GIB. Stop other models first."
  sleep 2
done
log "Available memory: this GB10 $h GiB, other GB10 $k GiB"

KEY_ARGS=""; KEY_MOUNT=""
if [[ -n "$API_KEY_FILE" ]]; then
  [[ -s "$API_KEY_FILE" ]] || die "API_KEY_FILE $API_KEY_FILE is empty or missing"
  KEY_MOUNT="-v $(realpath "$API_KEY_FILE"):/run/api-key:ro"; KEY_ARGS="--api-key-file /run/api-key"
fi
[[ "$HOST" == 127.0.0.1 || -n "$API_KEY_FILE" ]] || log "WARNING: serving on $HOST without an API key"

THINK=--thinking; [[ "$THINKING" == 1 ]] || THINK=--no-thinking
CMD1="$BIN1 $CKPT $RANK_CACHE 1 2 $MASTER_ADDR $MASTER_PORT $KIT --engram $ENGRAM --token-map $TOKEN_MAP \
--pool $CONTEXT --streams $PARALLEL --drafts 1 --graphs 1 --side 1 --prefetch 1 --aio 1 --rdma $HCAS"
log "Rank 1 ($PEER, $FLAVOR): $CMD1"
w "docker run -d --name $NAME $RUN_ARGS $COMMON_ENV $NCCL_ENV --entrypoint '' $IMAGE $CMD1" >/dev/null
w "setsid nohup bash $WORK/recipe/memwatch.sh $NAME >> $LOGS/memwatch.log 2>&1 < /dev/null &"

ENV0="-e TF_TP_WORLD=2 -e TF_TP_MASTER=$MASTER_ADDR -e TF_TP_PORT=$MASTER_PORT -e TF_DS_RANK_CACHE=$RANK_CACHE -e TF_DS_KIT=$KIT \
-e TENSORFOLD_CUDA_KERNELS=$KIT/aot -e TF_DS_ENGRAM=$ENGRAM -e TF_DS_TOKEN_MAP=$TOKEN_MAP -e TF_DS_GRAPHS=1 -e TF_DS_HC_SIDE=1 \
-e TF_DS_L2_PREFETCH=1 -e TF_DS_ENGRAM_AIO=1 -e TF_RDMA_DEVICES=$HCAS"
CMD0="$BIN0 serve $CKPT --backend cuda --host $HOST --port $PORT --name $SERVED_NAME --context $CONTEXT --parallel $PARALLEL \
--max-tokens $MAX_TOKENS --temperature $TEMPERATURE --top-p $TOP_P --top-k $TOP_K $THINK --no-update-check $KEY_ARGS"
log "Rank 0 (this GB10, $FLAVOR): $CMD0 $*"
# shellcheck disable=SC2086
docker run -d --name "$NAME" $RUN_ARGS $KEY_MOUNT $COMMON_ENV $NCCL_ENV $ENV0 --entrypoint '' "$IMAGE" $CMD0 "$@" >/dev/null
setsid nohup bash "$PWD/memwatch.sh" "$NAME" >> "$LOGS/memwatch.log" 2>&1 < /dev/null &
log "API: http://$HOST:$PORT/v1 (model $SERVED_NAME); ready when http://127.0.0.1:$PORT/health answers"

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
