# config.sh: settings shared by run.sh and stop.sh. Sourced, never run.
# Every value can be overridden from the environment, e.g. PORT=9000 ./run.sh
#
# One Dell Pro Max with GB10 (128 GB unified memory), DGX OS, Docker with the NVIDIA runtime.
# Thinking Machines' own checkpoint of Inkling 2 Mini (NVFP4 experts, MXFP8 elsewhere) on SGLang, with the DSpark
# draft head that ships with it. Based on SGLang's verified DGX Spark recipe for this model (docs.sglang.io cookbook).

RECIPE=inkling2-mini-nvfp4-gb10
# SGLang's Inkling 2 Mini build (tag inkling-2-mini, 2026-11-20), one digest for arm64 and amd64
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

# One request at a time with the model's full 1M context. Measured on a GB10 (thinking on, temperature 1.0):
# DSpark drafting 8 tokens per step 37.1 tok/s, without it 34.1; vLLM 38.0 with DSpark but reads long prompts at
# half the speed. DSpark only takes 8 (the checkpoint's block size). SGLang gets 65% of the memory: 20 GB of weights,
# the rest is KV cache for 1.1M tokens once the sliding-window layers keep only 2% of it: 22K, room for their 1K
# window plus a whole chunk of a long prompt (a 600K-token prompt was read and recalled correctly).
CONTEXT="${CONTEXT:-1048576}"
PARALLEL="${PARALLEL:-1}"
MEM_FRACTION="${MEM_FRACTION:-0.65}"
SWA_RATIO="${SWA_RATIO:-0.02}"
DRAFT_TOKENS="${DRAFT_TOKENS:-8}"
MIN_FREE_GB="${MIN_FREE_GB:-85}"        # memory that must be free before starting

log() { printf '[%s] %s\n' "$(date +%T)" "$*"; }
die() { log "ERROR: $*" >&2; exit 1; }

SNAPSHOT="$HUB/models--${REPO//\//--}/snapshots/$REVISION"
