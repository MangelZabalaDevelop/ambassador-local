# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max with GB10 (128 GB unified memory), DGX OS, Docker with the NVIDIA runtime.
# Based on gitcommit90's single-Spark recipe (github.com/gitcommit90/glm-5.3-one-spark @ ddc28b1, MIT),
# run from its published image plus the recipe's later weight-loader fix (Dockerfile).

RECIPE=glm53-flash-exl3-gb10
IMAGE="${IMAGE:-ghcr.io/gitcommit90/glm-5.3-one-spark@sha256:ba6db0547b24162854948fb16cda68dd4408360e03aeba480edb8d3d876eaeef}"   # must match "image" in recipes/catalog.json
FIXED_IMAGE="${FIXED_IMAGE:-ambassador-local/glm53-one-spark:loader-fix}"   # built from IMAGE + Dockerfile on the first start

# Weights: Turboderp's 2.05-bit EXL3 quant of GLM-5.3-Flash (branch 2.05bpw), pinned by commit
REPO=turboderp/GLM-5.3-Flash-exl3
REVISION=51058cd551c7e570d87bd32a4adee720edce2349
# Draft model for speculative decoding: Inco AI's DFlash2 (CC BY-NC-ND 4.0: noncommercial research/evaluation)
DRAFT_REPO=incoai/GLM-5.3-Flash-DFlash2
DRAFT_REVISION=bf582e4eacc1810f76656d1811693ff6c6737d2a

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
CACHE="${CACHE:-$WORK/cache}"           # kernels compiled on the first start (vllm, triton, tilelang, inductor)
LOGS="${LOGS:-$WORK/logs}"
NAME=al_glm53_exl3                      # container name

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-glm-5.3-flash}"

# GB10 shares its 128 GB between CPU and GPU: vLLM gets 88% (the recipe's 90% left ~1 GB free after reading an
# image). 256K per request, up to 4 requests at once, DFlash2 drafting 5 tokens (the recipe's measured best for
# prose and code; 8 is faster only for lists and JSON), FP8 KV cache.
CONTEXT="${CONTEXT:-262144}"
PARALLEL="${PARALLEL:-4}"
GPU_MEM="${GPU_MEM:-0.88}"
SPEC_TOKENS="${SPEC_TOKENS:-5}"
MIN_FREE_GB="${MIN_FREE_GB:-110}"       # memory that must be free before starting

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
DRAFT_SNAPSHOT="$HUB/models--${DRAFT_REPO//\//--}/snapshots/$DRAFT_REVISION"
