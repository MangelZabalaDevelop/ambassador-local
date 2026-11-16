# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max with GB10 (128 GB unified memory), DGX OS, Docker with the NVIDIA runtime.
# Based on NVIDIA's official single-Spark recipe for this checkpoint (eugr/spark-vllm-docker,
# recipes/qwen3.8-flash-next-nvfp4-solo.yaml).

RECIPE=qwen38-flash-next-fp4-gb10
IMAGE="${IMAGE:-eugr/spark-vllm-b12x@sha256:19ab778dd57f019c44caf5df91a671cf688923d1ca9fa8ce92be0c9acb46d215}"   # must match "image" in recipes/catalog.json

# Weights: NVIDIA's FP4-Local checkpoint of Qwen3.8 Flash Next, pinned by commit (same as the T2 recipe)
REPO=nvidia/Qwen3.8-Flash-Next-FP4-Local
REVISION=b9ce18a6887e18c089fdea8509a33e63429a3300

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
CACHE="${CACHE:-$WORK/cache}"           # kernels compiled on the first start (triton, b12x, flashinfer, vllm)
LOGS="${LOGS:-$WORK/logs}"
NAME=al_qwen38_fp4                      # container name

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-qwen3.8-flash-next}"

# GB10 shares its 128 GB between CPU and GPU: vLLM gets 80% and the per-layer embedding (PLE) tables stay on
# disk, so the system keeps room to breathe. 256K per request, MTP drafting 4 tokens, FP8 KV cache.
CONTEXT="${CONTEXT:-262144}"
PARALLEL="${PARALLEL:-4}"
GPU_MEM="${GPU_MEM:-0.8}"
SPEC_TOKENS="${SPEC_TOKENS:-4}"
EFFORT="${EFFORT:-xhigh}"
MIN_FREE_GB="${MIN_FREE_GB:-100}"       # memory that must be free before starting

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
