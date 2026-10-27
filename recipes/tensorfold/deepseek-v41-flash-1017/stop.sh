#!/bin/bash
# stop.sh: stops both ranks, keeps their last 10 logs (compressed) and frees the memory.
cd "$(dirname "$0")" || exit 1; source ./config.sh
mkdir -p "$LOGS"
ts=$(date +%Y%m%d-%H%M%S)
if docker ps -aq -f "name=^${NAME}\$" | grep -q .; then
  docker stop -t 30 "$NAME" >/dev/null 2>&1
  docker logs "$NAME" 2>&1 | gzip > "$LOGS/rank0-$ts.log.gz"
  docker rm -f "$NAME" >/dev/null 2>&1
fi
ls -t "$LOGS"/rank0-*.log.gz 2>/dev/null | tail -n +11 | xargs -r rm -f
pkill -f "[m]emwatch.sh $NAME" 2>/dev/null
if [[ -n "$PEER" ]]; then
  w "docker ps -aq -f name=^${NAME}\\\$ | grep -q . && { docker stop -t 30 $NAME >/dev/null 2>&1; mkdir -p $LOGS; docker logs $NAME 2>&1 | gzip > $LOGS/rank1-$ts.log.gz; docker rm -f $NAME >/dev/null 2>&1; }; ls -t $LOGS/rank1-*.log.gz 2>/dev/null | tail -n +11 | xargs -r rm -f; pkill -f '[m]emwatch.sh $NAME'; true"
fi
log "stopped (logs in $LOGS)"
exit 0
