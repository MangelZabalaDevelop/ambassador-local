# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max Tower T2 with an RTX PRO 6000 Blackwell (96 GB), Linux, Docker with the NVIDIA runtime.
# Thinking Machines' own checkpoint of Inkling 2 Mini (NVFP4 experts, MXFP8 elsewhere) on SGLang, with the DSpark
# draft head that ships with it. Based on SGLang's verified RTX PRO 6000 recipe for this model (docs.sglang.io cookbook).

RECIPE=inkling2-mini-nvfp4-t2
# SGLang's Inkling 2 Mini build (tag inkling-2-mini, 2026-11-20), one digest for amd64 and arm64
IMAGE="${IMAGE:-lmsysorg/sglang@sha256:efac853b46811ccb760545b7b29e7119bdf907537ccf6e863fac3011fe8b98de}"   # must match "image" in recipes/catalog.json

# Weights: thinkingmachines/Inkling-2-Mini, pinned by commit (20 GB, the DSpark head included)
REPO=thinkingmachines/Inkling-2-Mini
REVISION=ed40e7da625f594fc9c4990f5f66be24873c22d7

HUB="${HF_HUB_CACHE:-${HF_HOME:-$HOME/.cache/huggingface}/hub}"
WORK="${WORK:-$HOME/.cache/ambassador-local/vllm/$RECIPE}"
CACHE="${CACHE:-$WORK/cache}"           # kernels compiled on the first start (triton, sglang)
LOGS="${LOGS:-$WORK/logs}"
NAME=al_inkling2_mini                   # container name

HOST="${HOST:-127.0.0.1}"               # 0.0.0.0 = reachable from your network (set API_KEY_FILE then)
PORT="${PORT:-8000}"
API_KEY_FILE="${API_KEY_FILE:-}"        # file with the key clients must send; empty = no key
SERVED_NAME="${SERVED_NAME:-inkling-2-mini}"

# One request at a time with the model's full 1M context. Measured on an RTX PRO 6000 Blackwell (thinking on,
# temperature 1.0):
# DSpark drafting 8 tokens per step is 21% faster than without it; vLLM has no DSpark kernels for this card yet,
# and llama.cpp is less than half as fast. DSpark only takes 8 (the checkpoint's block size). SGLang gets 80% of
# the card: 20 GB of weights and draft head, about 50 GB of KV cache, and the rest for CUDA graphs and whatever
# else uses the card. The sliding-window layers get 5% of the full layers' tokens: about 60K next to 1.2M, room
# for their 1K window plus a whole chunk of a long prompt. Too small a share stalls long prompts.
CONTEXT="${CONTEXT:-1048576}"
PARALLEL="${PARALLEL:-1}"
MEM_FRACTION="${MEM_FRACTION:-0.8}"
SWA_RATIO="${SWA_RATIO:-0.05}"
DRAFT_TOKENS="${DRAFT_TOKENS:-8}"

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
