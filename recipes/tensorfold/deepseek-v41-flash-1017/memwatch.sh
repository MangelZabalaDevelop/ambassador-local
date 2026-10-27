#!/bin/bash
# memwatch.sh <container>: memory brake for this GB10's rank. On a GB10 the GPU shares the RAM: a rank that eats all
# of it freezes the machine instead of failing. Kills the rank's container (docker kill -s KILL) if MemAvailable stays
# under WATCH_FLOOR_GIB for WATCH_HOLD_S seconds in a row, or as soon as it drops under WATCH_HARD_GIB.
# A dip that recovers by itself is only logged. Samples every 0.25 s; ends when the container is gone.
cd "$(dirname "$0")" || exit 1; source ./config.sh
c=$1
floor=$(awk -v g="$WATCH_FLOOR_GIB" 'BEGIN {printf "%d", g * 1048576}')
hard=$(awk -v g="$WATCH_HARD_GIB" 'BEGIN {printf "%d", g * 1048576}')
hold=$(( WATCH_HOLD_S * 4 ))
for _ in $(seq 1 40); do pid=$(docker inspect -f '{{.State.Pid}}' "$c" 2>/dev/null); [[ "${pid:-0}" != 0 ]] && break; sleep 0.5; done
[[ "${pid:-0}" != 0 ]] || exit 0
log "watching $c (pid $pid): floor ${WATCH_FLOOR_GIB} GiB x ${WATCH_HOLD_S}s, hard ${WATCH_HARD_GIB} GiB"
under=0; low=0
while [[ -d /proc/$pid ]]; do
  a=$(awk '/MemAvailable/ {print $2}' /proc/meminfo)
  if (( a < hard )); then
    docker kill -s KILL "$c" >/dev/null 2>&1; log "KILLED $c: MemAvailable $((a / 1024)) MiB < hard floor"; break
  elif (( a < floor )); then
    (( under == 0 || a < low )) && low=$a
    under=$((under + 1))
    if (( under >= hold )); then docker kill -s KILL "$c" >/dev/null 2>&1; log "KILLED $c: MemAvailable < ${WATCH_FLOOR_GIB} GiB for ${WATCH_HOLD_S}s (min $((low / 1024)) MiB)"; break; fi
  elif (( under > 0 )); then
    log "dip of $((under * 250)) ms (min $((low / 1024)) MiB), recovered"; under=0
  fi
  sleep 0.25
done
