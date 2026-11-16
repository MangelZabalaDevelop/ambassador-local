#!/bin/bash
# stop.sh: stops the model, the bad_words proxy and ramwatch, keeps the last 10 logs (compressed) and frees the GPU.
cd "$(dirname "$0")"; source ./config.sh
mkdir -p "$LOGS"
if docker ps -aq -f "name=^${NAME}\$" | grep -q .; then
  docker stop -t 30 "$NAME" >/dev/null 2>&1
  docker logs "$NAME" 2>&1 | gzip > "$LOGS/vllm-$(date +%Y%m%d-%H%M%S).log.gz"
  docker rm -f "$NAME" >/dev/null 2>&1
fi
for f in "$WORK/proxy.pid" "$WORK/ramwatch.pid"; do
  [[ -f "$f" ]] && kill "$(cat "$f")" 2>/dev/null
  rm -f "$f"
done
ls -t "$LOGS"/vllm-*.log.gz 2>/dev/null | tail -n +11 | xargs -r rm -f
log "stopped (logs in $LOGS)"
exit 0
