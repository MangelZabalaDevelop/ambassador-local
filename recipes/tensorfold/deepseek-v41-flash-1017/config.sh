# config.sh: settings shared by build.sh, prepare.sh, run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. CONTEXT=524288 PARALLEL=4 ./run.sh
#
# Two GB10 linked by the ConnectX cable, tensor-parallel 2: rank 0 (and the API) on the GB10 where you run the
# scripts, rank 1 on the other one. The scripts reach the other GB10 with the passwordless SSH that the cluster
# setup in NVIDIA Sync leaves ready. Same paths on both GB10.

RECIPE=deepseek-v41-flash-1017
IMAGE="${IMAGE:-ambassador-local/tensorfold-dsv41:6366d2c}"   # must match "image" in recipes/catalog.json
ENGINE_COMMIT=6366d2c585b4f05fe33bfe5cbdb33ea15264841a        # must match "engine_commit" in recipes/catalog.json

# Weights: EXL3 2.9 bpw quant of deepseek-ai/DeepSeek-V4.1-Flash-1017 (MIT)
REPO=LibertAIDAI/DeepSeek-V4.1-Flash-1017-EXL3-2.9bpw
REVISION=0be63c3337efbb2a0061d5b8bb4006fa186d433f
# Engram tables: shards 47 and 48 of DeepSeek's original checkpoint (~203 GB), read at run time
ENGRAM_REPO=deepseek-ai/DeepSeek-V4.1-Flash-1017
ENGRAM_REVISION=1bae0e1088993b9b82de08f59de5be5a34e42170
ENGRAM_FILES="config.json model.safetensors.index.json LICENSE model-00047-of-00048.safetensors model-00048-of-00048.safetensors"
# Kernel kit for the dsv41 family (Apache-2.0)
KIT_REPO=https://github.com/bertholomus/deepseek-v4.1-tensorfold-tp2-2xgb10.git
KIT_REF=abe07c7d509a0199cc7fec15263be2aa52ac652f

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/tensorfold/$RECIPE}"
CKPT=$WORK/ckpt                         # flat checkpoint: hard links to the Hugging Face cache, our chat template
ENGRAM=$HUB/models--${ENGRAM_REPO//\//--}/snapshots/$ENGRAM_REVISION
KIT=$WORK/kit-repo/kit
TOKEN_MAP=$WORK/data/dsv41_token_map.json
RANK_CACHE=$WORK/rank-cache             # rank0of2-*.bin on this GB10, rank1of2-*.bin on the other
LOGS=$WORK/logs
NAME=al_dsv41                           # container name, the same on both GB10

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8090}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-$RECIPE}"
MASTER_PORT="${MASTER_PORT:-29620}"
GID="${GID:-3}"                         # RoCE v2 IPv4 GID index (the same on both GB10 and both rails)

# Tuned on two GB10 (2026-10-20): the streams share ONE pool of CONTEXT tokens and each request reserves
# prompt + max_tokens when it enters. 2 streams over the model's whole 1M window with the 262144 output tokens of
# the model card: a lone request gets the full 1M; two at once fit while they add up to ~1M, otherwise the second
# waits for the first. What the requests leave free keeps prompt prefixes cached.
CONTEXT="${CONTEXT:-1048576}"           # per-request window = the model's max_position_embeddings
PARALLEL="${PARALLEL:-2}"               # simultaneous streams (rank 1 uses the same)
MAX_TOKENS="${MAX_TOKENS:-262144}"      # for requests that do not send max_tokens
TEMPERATURE=1.0; TOP_P=0.95; TOP_K=20   # the engine accepts top_k up to 56
THINKING="${THINKING:-1}"               # default reasoning effort is "max" (our chat template); requests can ask for less
FILL_CHUNKS="${FILL_CHUNKS:-}"          # TF_FILL_CHUNKS: long prefills in chunks between other streams' rounds (empty = off)
FLAVOR="${FLAVOR:-safe}"                # safe = the engine author's build; fast = ReleaseFast, native CPU (A/B only)

MIN_AVAIL_GIB="${MIN_AVAIL_GIB:-112}"   # weights (~99 GiB) + the engine's reserve (12.2 GiB), on each GB10
WATCH_FLOOR_GIB="${WATCH_FLOOR_GIB:-2}" # memwatch.sh kills the rank if MemAvailable stays under this...
WATCH_HOLD_S="${WATCH_HOLD_S:-4}"       # ...for this many seconds,
WATCH_HARD_GIB="${WATCH_HARD_GIB:-1}"   # or as soon as it drops under this hard floor

if [[ "$FLAVOR" == fast ]]; then P=/opt/tensorfold-fast; else P=/opt/tensorfold; fi
BIN0=$P/native/bin/tensorfold-native
BIN1=$P/bin/tf-dsv41-lanes

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }
avail_gib() { awk '/MemAvailable/ {printf "%.1f", $2 / 1048576}' /proc/meminfo; }

# The direct cable: the first ConnectX (mlx5) interface with carrier and an IPv4 address, and the GB10 behind it.
link_iface() {
  local n d
  for n in /sys/class/net/*; do
    d=${n##*/}
    [[ "$(readlink "$n/device/driver" 2>/dev/null)" == *mlx5_core && "$(cat "$n/carrier" 2>/dev/null)" == 1 ]] || continue
    ip -4 -o addr show dev "$d" | grep -q ' inet ' && { echo "$d"; return; }
  done
}
IFACE="${IFACE:-$(link_iface)}"
MASTER_ADDR="${MASTER_ADDR:-$( [[ -n "$IFACE" ]] && ip -4 -o addr show dev "$IFACE" | awk '{split($4, a, "/"); print a[1]; exit}')}"
PEER="${PEER:-$( [[ -n "$IFACE" ]] && ip -4 neigh show dev "$IFACE" | awk '$NF != "FAILED" && $NF != "INCOMPLETE" {print $1; exit}')}"
# Both RoCE rails of the cabled QSFP port: the RDMA devices whose port is ACTIVE (the same count on both GB10).
HCAS="${HCAS:-$(for d in /sys/class/infiniband/*; do grep -q ACTIVE "$d/ports/1/state" 2>/dev/null && printf '%s,' "${d##*/}"; done | sed 's/,$//')}"

need_link() {
  [[ -n "$IFACE" && -n "$MASTER_ADDR" ]] || die "no ConnectX link with an IPv4 address. Connect the two GB10 and run the cluster setup in NVIDIA Sync."
  [[ -n "$PEER" ]] || die "the other GB10 was not found on $IFACE. Ping it once, or set PEER=<its address on the direct cable>."
  [[ -n "$HCAS" ]] || die "no active RDMA device on the ConnectX port"
}
w() { ssh -n -o BatchMode=yes -o ConnectTimeout=10 "$PEER" "$@"; }   # no stdin (does not eat the script)
wi() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$PEER" "$@"; }     # with stdin: for pipes to the other GB10

RUN_ARGS="--gpus all --ipc=host --network host --ulimit memlock=-1 --ulimit stack=67108864 --cap-add IPC_LOCK --device /dev/infiniband \
-v $WORK:$WORK:ro -v $HUB:$HUB:ro"
NCCL_ENV="-e NCCL_SOCKET_IFNAME=$IFACE -e NCCL_IB_HCA=$HCAS -e NCCL_IB_GID_INDEX=$GID -e TF_RDMA_GID_INDEX=$GID"
COMMON_ENV="-e TF_DS_REPLAY_FLOOR=1 -e TF_DS_STREAMS=$PARALLEL -e HF_HUB_OFFLINE=1 -e TENSORFOLD_NO_UPDATE_CHECK=1"
[[ -z "$FILL_CHUNKS" ]] || COMMON_ENV+=" -e TF_FILL_CHUNKS=$FILL_CHUNKS"
