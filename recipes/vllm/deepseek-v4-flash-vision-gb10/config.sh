# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max with GB10 (128 GB unified memory), DGX OS, Docker with the NVIDIA runtime.
# Serves GaelicThunder's "Kalibrated Vision Exp" pack of DeepSeek-V4-Flash-Vision-Exp with the recipe published in
# github.com/GaelicThunder/DeepSeek-V4-Flash-Vision-One-DGX-Spark (Apache-2.0), pinned by commit.

RECIPE=deepseek-v4-flash-vision-gb10
# Engine: 0xSero's sparkinfer vLLM image, pinned by digest (must match "image" in recipes/catalog.json).
IMAGE="${IMAGE:-ghcr.io/0xsero/deepseek-v4-flash-0731-spark-sparkinfer@sha256:2e077489a83a0360952828051fe7f7a32c1801e5ce8436d85f7267583d614ff4}"
# The same image with xgrammar fixed once at build time (Dockerfile), instead of a pip upgrade at every boot.
FIXED_IMAGE="${FIXED_IMAGE:-ambassador-local/ds4-vision:xgrammar-0.2.6}"

# The serving recipe (vision port overlay, MiaAI-Lab launcher, bad_words proxy, ramwatch), pinned by commit.
SRC_REPO=GaelicThunder/DeepSeek-V4-Flash-Vision-One-DGX-Spark
SRC_COMMIT=aaff897c88d840574d3bc1a9e74766672f1d995f

# Weights: the Kalibrated pack (256 experts, 2-bit with 28 of 43 expert layers at calibrated 3-bit), pinned by commit.
REPO=GaelicThunder/DeepSeek-V4-Flash-Vision-Exp-ablit-EXL3-Kalibrated
REVISION=178652ce2ef562c9135488def954a2418db6f5e5

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
SRC="$WORK/src-${SRC_COMMIT:0:7}"     # the recipe source, downloaded on the first start
MODEL="$WORK/model"                   # the pack as the engine expects it: hard links into the Hugging Face cache
LOGS="${LOGS:-$WORK/logs}"
NAME=al_ds4_vision                    # container name

HOST="${HOST:-127.0.0.1}"             # where the API listens (the bad_words proxy); 0.0.0.0 = your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
ENGINE_PORT="${ENGINE_PORT:-30031}"   # the engine itself, only on 127.0.0.1, behind the proxy
API_KEY_FILE="${API_KEY_FILE:-}"      # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-deepseek-v4-flash-vision}"

# Measured by the recipe's author and here (2026-11-16): 240K context, one request at a time, DSpark drafting 5 tokens.
# vLLM takes 92.5% of the memory (100 GB of weights + ~250K tokens of KV cache): never go above 0.93 on a GB10.
CONTEXT="${CONTEXT:-245760}"
UTIL="${UTIL:-0.925}"
SPEC_TOKENS="${SPEC_TOKENS:-5}"
EFFORT="${EFFORT:-max}"
MIN_FREE_GB="${MIN_FREE_GB:-110}"     # memory that must be free before starting
RAMWATCH_FLOOR_MB="${RAMWATCH_FLOOR_MB:-700}"   # stop the engine below this, instead of letting the GB10 freeze

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
