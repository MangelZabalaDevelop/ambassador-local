#!/bin/bash
# stop.sh: stops the model, keeps its last 10 logs (compressed) and frees the GPU.
cd "$(dirname "$0")"; source ./config.sh
mkdir -p "$LOGS"
if docker ps -aq -f "name=^${NAME}\$" | grep -q .; then
  docker stop -t 30 "$NAME" >/dev/null 2>&1
  docker logs "$NAME" 2>&1 | gzip > "$LOGS/vllm-$(date +%Y%m%d-%H%M%S).log.gz"
  docker rm -f "$NAME" >/dev/null 2>&1
fi
ls -t "$LOGS"/vllm-*.log.gz 2>/dev/null | tail -n +11 | xargs -r rm -f
log "stopped (logs in $LOGS)"
exit 0
