# config.sh: settings shared by run.sh, stop.sh and memwatch.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. CONTEXT=131072 PARALLEL=2 ./run.sh
#
# Two GB10 linked by the ConnectX cable, vLLM tensor-parallel 2 with expert parallel: rank 0 (and the API) on the GB10
# where you run the scripts, rank 1 on the other one. The scripts reach the other GB10 with the passwordless SSH that
# the cluster setup in NVIDIA Sync leaves ready. Same paths on both GB10.
# Based on atakann/dgx-spark-recipes (MIT), recipe llm/hy4-flash-nvfp4-vllm-tp2 @ e764efa.

RECIPE=hy4-flash-nvfp4-gb10x2
# vLLM's official Hy4-flash image for arm64 (vLLM 77766db with the Hy4-flash pull request, FlashInfer 0.7.4)
IMAGE="${IMAGE:-vllm/vllm-openai@sha256:5e2c1badbca60595d6591c60feb4b8547f59da96e7a91710d40239250e721aba}"   # must match "image" in recipes/catalog.json
FIXED_IMAGE="${FIXED_IMAGE:-ambassador-local/vllm-hy4-flash:77766db-e764efa}"   # IMAGE + Dockerfile, built on each GB10 on the first start

# Weights: atakans' NVFP4 build of tencent/Hy4-flash (experts NVFP4 with NVIDIA ModelOpt, Engram tables FP8), pinned
REPO=atakans/Hy4-flash-NVFP4
REVISION=922cbf11f2abf547951d22eabc76b876f2a9d03b

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
CACHE="${CACHE:-$WORK/cache}"           # kernels compiled and tuned on the first start (FlashInfer, Triton, vLLM)
LOGS="${LOGS:-$WORK/logs}"
NAME=al_hy4_flash                       # container name, the same on both GB10

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-hy4-flash}"
MASTER_PORT="${MASTER_PORT:-29501}"

# Measured on two GB10 by the recipe's author: the weights take ~52 GiB of each GB10 plus half of the Engram tables,
# and a 6 GB KV cache already holds ~578K tokens (sparse + sliding-window attention keeps the cache small), so 256K
# per request with up to 4 requests at once. DFly (the model's own drafter) proposes 5 tokens a step.
CONTEXT="${CONTEXT:-262144}"
PARALLEL="${PARALLEL:-4}"
KV_CACHE="${KV_CACHE:-6G}"
GPU_MEM="${GPU_MEM:-0.85}"
SPEC_TOKENS="${SPEC_TOKENS:-5}"
MEM_LIMIT="${MEM_LIMIT:-112g}"          # docker --memory of each rank

MIN_AVAIL_GIB="${MIN_AVAIL_GIB:-108}"   # free memory each GB10 needs before starting
WATCH_FLOOR_GIB="${WATCH_FLOOR_GIB:-2}" # memwatch.sh kills the rank if MemAvailable stays under this...
WATCH_HOLD_S="${WATCH_HOLD_S:-4}"       # ...for this many seconds,
WATCH_HARD_GIB="${WATCH_HARD_GIB:-1}"   # or as soon as it drops under this hard floor

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }
avail_gib() { awk '/MemAvailable/ {printf "%.1f", $2 / 1048576}' /proc/meminfo; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"

# The direct cable: the first ConnectX (mlx5) interface with carrier and an IPv4 address, and the GB10 behind it.
link_iface() {
  local n d
  for n in /sys/class/net/*; do
    d=${n##*/}
    [[ "$(readlink "$n/device/driver" 2>/dev/null)" == *mlx5_core && "$(cat "$n/carrier" 2>/dev/null)" == 1 ]] || continue
    ip -4 -o addr show dev "$d" | grep -q ' inet ' && { echo "$d"; return; }
  done
}
# (Every probe ends in success even when it finds nothing: the scripts run with set -e, and need_link explains.)
IFACE="${IFACE:-$(link_iface || true)}"
MASTER_ADDR="${MASTER_ADDR:-$(if [[ -n "$IFACE" ]]; then ip -4 -o addr show dev "$IFACE" | awk '{split($4, a, "/"); print a[1]; exit}'; fi)}"
PEER="${PEER:-$(if [[ -n "$IFACE" ]]; then ip -4 neigh show dev "$IFACE" | awk '$NF != "FAILED" && $NF != "INCOMPLETE" {print $1; exit}'; fi)}"
# Both RoCE rails of the cabled QSFP port: the RDMA devices whose port is ACTIVE (the same count on both GB10).
HCAS="${HCAS:-$(for d in /sys/class/infiniband/*; do if grep -q ACTIVE "$d/ports/1/state" 2>/dev/null; then printf '%s,' "${d##*/}"; fi; done | sed 's/,$//')}"

need_link() {
  [[ -n "$IFACE" && -n "$MASTER_ADDR" ]] || die "no ConnectX link with an IPv4 address. Connect the two GB10 and run the cluster setup in NVIDIA Sync."
  [[ -n "$PEER" ]] || die "the other GB10 was not found on $IFACE. Ping it once, or set PEER=<its address on the direct cable>."
  [[ -n "$HCAS" ]] || die "no active RDMA device on the ConnectX port"
}
w() { ssh -n -o BatchMode=yes -o ConnectTimeout=10 "$PEER" "$@"; }   # no stdin (does not eat the script)
wi() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$PEER" "$@"; }     # with stdin: for pipes to the other GB10
