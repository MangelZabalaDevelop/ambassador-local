# config.sh: settings shared by run.sh, stop.sh and memwatch.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max with GB10 (128 GB unified memory), DGX OS, Docker with the NVIDIA runtime.
# Tencent's own GB10 setup for Hy4-flash (AngelSlim/Hy4-flash-GGUF, README_SPARK-4Bit.md): their SPARK-4Bit GGUF on
# llama.cpp with their patch, which the Dockerfile builds into an image on the first start.

RECIPE=hy4-flash-spark4bit-gb10
# NVIDIA's CUDA 13.0.3 devel image for Ubuntu 24.04 (tag 13.0.3-devel-ubuntu24.04), the CUDA of DGX OS
IMAGE="${IMAGE:-nvidia/cuda@sha256:b7ae301dea2c162444795462ce17a05f6a516e5a75944b57af5b88540a1a2266}"   # must match "image" in recipes/catalog.json
FIXED_IMAGE="${FIXED_IMAGE:-ambassador-local/llama-hy4-flash:2cbca38-65acb8a}"   # built from IMAGE + Dockerfile on the first start

# Weights: Tencent AngelSlim's SPARK-4Bit-Fast GGUF (NVFP4 experts, Engram tables read from disk) and the image
# encoder, pinned by commit
REPO=AngelSlim/Hy4-flash-GGUF
REVISION=65acb8ae7f59847fe03a17291f1b72b89b141a9e
GGUF=Hy4-flash-SPARK-4Bit-Fast.gguf
MMPROJ=mmproj-Hy4-flash-bf16.gguf

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
LOGS="${LOGS:-$WORK/logs}"
NAME=al_hy4_flash_gguf                  # container name

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-hy4-flash}"

# One request at a time (llama.cpp's fastest for one user), the model's MTP head drafting DRAFT tokens. Measured
# on one GB10, average of the standard prompts with thinking on: 1 → 29.5, 2 → 30.6, 3 → 28.4, 4 → 26.8 tok/s
# (3 and 4 only help code and JSON, by under 1 tok/s). Tencent's README uses 196608 tokens of context.
CONTEXT="${CONTEXT:-262144}"
DRAFT="${DRAFT:-2}"
MIN_FREE_GB="${MIN_FREE_GB:-110}"       # memory that must be free before starting
WATCH_FLOOR_GIB="${WATCH_FLOOR_GIB:-2}" # memwatch.sh kills the engine if MemAvailable stays under this...
WATCH_HOLD_S="${WATCH_HOLD_S:-4}"       # ...for this many seconds,
WATCH_HARD_GIB="${WATCH_HARD_GIB:-1}"   # or as soon as it drops under this hard floor

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
