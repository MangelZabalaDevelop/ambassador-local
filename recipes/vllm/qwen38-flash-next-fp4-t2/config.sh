# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max Tower T2 with an RTX PRO 6000 Blackwell (96 GB), Ubuntu 24.04, Docker with the NVIDIA runtime.

RECIPE=qwen38-flash-next-fp4-t2
IMAGE="${IMAGE:-ghcr.io/local-inference-lab/vllm@sha256:faf0b1f08d688b1aaa7d5161dc3b724c701d17d68278e7d32ebdd3a6b5127ce2}"   # must match "image" in recipes/catalog.json

# Weights: NVIDIA's FP4-Local checkpoint of Qwen3.8 Flash Next, pinned by commit
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

# Tuned on the T2 (2026-09): 256K per request, up to 4 at once, MTP drafting 3 tokens, b12x kernels everywhere,
# 24 GB of the KV cache offloaded to system memory. Thinking on, reasoning effort xhigh unless a request asks for less.
CONTEXT="${CONTEXT:-262144}"
PARALLEL="${PARALLEL:-4}"
GPU_MEM="${GPU_MEM:-0.96}"
KV_OFFLOAD_GB="${KV_OFFLOAD_GB:-24}"
SPEC_TOKENS="${SPEC_TOKENS:-3}"
EFFORT="${EFFORT:-xhigh}"

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
