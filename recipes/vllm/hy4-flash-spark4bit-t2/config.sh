# config.sh: settings shared by build.sh, run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max Tower T2 with an RTX PRO 6000 Blackwell (96 GB), Linux, Docker with the NVIDIA runtime.
# Tencent's SPARK-4Bit GGUF of Hy4-flash (AngelSlim/Hy4-flash-GGUF) on llama.cpp with their patch, which the
# Dockerfile builds into an image for the RTX PRO 6000 on the first start. Same weights as the GB10 recipe.

RECIPE=hy4-flash-spark4bit-t2
# NVIDIA's CUDA 13.0.3 devel image for Ubuntu 24.04 (tag 13.0.3-devel-ubuntu24.04)
IMAGE="${IMAGE:-nvidia/cuda@sha256:b7ae301dea2c162444795462ce17a05f6a516e5a75944b57af5b88540a1a2266}"   # must match "image" in recipes/catalog.json
FIXED_IMAGE="${FIXED_IMAGE:-ambassador-local/llama-hy4-flash:2cbca38-65acb8a-sm120a}"   # built from IMAGE + Dockerfile on the first start

# Weights: Tencent AngelSlim's SPARK-4Bit-Fast GGUF (NVFP4 experts; the 59 GiB Engram table stays in system memory,
# read on demand) and the image encoder, pinned by commit
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
GPU="${GPU:-0}"                         # the card, as nvidia-smi numbers it

# One request at a time (llama.cpp's fastest for one user), the model's MTP head drafting DRAFT tokens. Measured on
# an RTX PRO 6000, average of the standard prompts with thinking on: 1 → 123.4, 2 → 128.5, 3 → 118.6 tok/s.
DRAFT="${DRAFT:-2}"
# The weights take about 90 GB of the card's 96 GB; the context gets what is left, so run.sh picks the largest that
# fits next to whatever already uses the card (a desktop, a browser). Each entry is "context:MiB that must be free",
# largest first: the card's use measured after loading, plus an image request and some use, plus 200 MiB.
# 256K tokens does not fit with the image encoder on the card, and an 8-bit KV cache breaks the MTP drafts.
CONTEXT="${CONTEXT:-auto}"              # or a number of tokens
CONTEXT_FIT=(196608:96950 131072:94950 65536:92850)

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
