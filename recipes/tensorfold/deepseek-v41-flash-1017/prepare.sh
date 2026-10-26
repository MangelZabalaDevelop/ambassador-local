#!/bin/bash
# prepare.sh: gets both GB10 ready to serve. Run it once on the GB10 that will serve the API; it can be run again
# and skips what is already done. Heavy steps run with bounded memory so they never freeze a model that is serving.
#   1. engine image on both GB10 (built here with build.sh, copied to the other GB10)
#   2. weights and Engram tables in each GB10's Hugging Face cache, pinned by commit and checked by sha256
#   3. flat checkpoint (hard links to the cache) with the chat template this engine can compile
#   4. kernel kit pinned by commit, RoPE and vision-bias tables generated from the checkpoint, Engram token map
#   5. each GB10's weight file for its rank (rank 0 here, rank 1 on the other GB10), CPU only
# Both repos are public. If one ever needs a token, export HF_TOKEN on each GB10: it is passed to the download
# container from the environment, never on a command line or to disk.
#
# --node R is the per-GB10 part (steps 2-5); the first GB10 runs it for itself and, over SSH, on the other one.
set -euo pipefail
cd "$(dirname "$0")"; source ./config.sh
HERE=$PWD
mkdir -p "$LOGS" "$WORK/data" "$RANK_CACHE"

CAP=0; systemd-run --user --scope --quiet true 2>/dev/null && CAP=1
cap() { if (( CAP )); then systemd-run --user --scope --quiet -p MemoryHigh=3G -p MemoryMax=6G "$@"; else "$@"; fi; }
# check_sums <dir> <file of "sha256  name">: sha256 in parallel, bounded memory
check_sums() { (cd "$1" && cap xargs -a "$2" -L 1 -P 8 sh -c 'echo "$0  $1" | sha256sum -c --quiet -'); }
cpu() { docker run --rm --network none --memory 6g --memory-swap 6g --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$WORK:$WORK" -v "$HUB:$HUB:ro" "$@"; }
hf() { docker run --rm --network host --memory 6g --memory-swap 6g --user "$(id -u):$(id -g)" -e HOME=/tmp -e HF_HUB_CACHE="$HUB" \
  ${HF_TOKEN:+-e HF_TOKEN} -v "$HUB:$HUB" --entrypoint hf "$IMAGE" download "$@"; }

node() {
  local R=$1 S=$HUB/models--${REPO//\//--}/snapshots/$REVISION
  [[ "$R" == 0 || "$R" == 1 ]] || die "--node needs 0 or 1"
  docker image inspect "$IMAGE" >/dev/null 2>&1 || die "$IMAGE is missing on $(hostname)"
  mkdir -p "$HUB"

  # ---------------------------------------------------------------- 2. weights and Engram
  log "Weights: $REPO @ ${REVISION:0:8} (~211 GB)"
  hf "$REPO" --revision "$REVISION" > "$LOGS/download-weights.log" 2>&1 || die "weight download failed ($LOGS/download-weights.log)"
  log "Engram tables: $ENGRAM_REPO @ ${ENGRAM_REVISION:0:8} shards 47-48 (~203 GB)"
  # shellcheck disable=SC2086
  hf "$ENGRAM_REPO" --revision "$ENGRAM_REVISION" --include $ENGRAM_FILES > "$LOGS/download-engram.log" 2>&1 ||
    die "Engram download failed ($LOGS/download-engram.log)"
  if [[ ! -f "$WORK/.engram-ok" ]]; then
    log "sha256 of the Engram tables"
    check_sums "$ENGRAM" "$HERE/SHA256SUMS.engram" || die "Engram sha256 does not match"
    touch "$WORK/.engram-ok"
  fi

  # ---------------------------------------------------------------- 3. flat checkpoint
  if [[ ! -f "$CKPT/.ok" ]]; then
    log "Flat checkpoint in $CKPT and sha256 against the Hub"
    rm -rf "$CKPT.new"; mkdir -p "$CKPT.new"
    cp -alL "$S/." "$CKPT.new/" 2>/dev/null || { log "Hard links not possible (another disk): copying"; rm -rf "$CKPT.new"; mkdir -p "$CKPT.new"; cp -rL "$S/." "$CKPT.new/"; }
    check_sums "$CKPT.new" "$HERE/SHA256SUMS.weights" || die "checkpoint sha256 does not match the Hub"
    rm -rf "$CKPT"; mv "$CKPT.new" "$CKPT"; touch "$CKPT/.ok"
  fi

  # ---------------------------------------------------------------- 4. kit, token map, chat template
  if [[ ! -d "$WORK/kit-repo/.git" ]]; then
    log "Kernel kit: $KIT_REPO @ ${KIT_REF:0:8}"
    git init -q "$WORK/kit-repo"; git -C "$WORK/kit-repo" remote add origin "$KIT_REPO"
    git -C "$WORK/kit-repo" fetch -q --depth 1 origin "$KIT_REF"; git -C "$WORK/kit-repo" checkout -q --detach FETCH_HEAD
  fi
  [[ "$(git -C "$WORK/kit-repo" rev-parse HEAD)" == "$KIT_REF" ]] || die "kit-repo is not at $KIT_REF"
  local TOOLS=/opt/tensorfold/tools/inputs
  [[ -s "$KIT/rope-plain-cos.f32" ]] || { log "RoPE tables from config.json"; cpu --entrypoint python3 "$IMAGE" $TOOLS/make_rope.py "$CKPT/config.json" "$KIT"; }
  [[ -s "$KIT/vision/gate_bias_vl.safetensors" ]] || { log "Vision bias (gate.bias_vl of 1017)"; cpu --entrypoint python3 "$IMAGE" $TOOLS/make_bias_vl.py "$CKPT" "$KIT/vision/gate_bias_vl.safetensors"; }
  local bad=0 sum path
  while read -r sum path; do
    [[ -f "$KIT/$path" && "$(sha256sum "$KIT/$path" | cut -d' ' -f1)" == "$sum" ]] && continue
    # The kit's MANIFEST lists V4.1-Flash's bias; 1017 has its own (expected to differ)
    [[ "$path" == vision/gate_bias_vl.safetensors ]] && continue
    log "Kit: $path does not match MANIFEST"; bad=1
  done < "$KIT/MANIFEST"
  (( bad == 0 )) || die "the kit does not match its MANIFEST"
  [[ -s "$TOKEN_MAP" ]] || { log "Engram token map"; cpu --entrypoint python3 "$IMAGE" $TOOLS/make_token_map.py "$CKPT" "$TOKEN_MAP"; }
  local tm want
  tm=$(sha256sum "$TOKEN_MAP" | cut -d' ' -f1)
  want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token_map_sha256"])' "$KIT/engram.json")
  [[ "$tm" == "$want" ]] && log "Token map: identical to the kit's" || log "WARNING: the token map ($tm) differs from the kit's"

  # The template in the weights does not compile in this TensorFold; make_chat_template.py adapts it. rm before cp:
  # the checkpoint file is a hard link to the Hugging Face cache, and the cache must stay untouched.
  local ORIG=$WORK/data/chat_template.orig.jinja TPL=$WORK/data/chat_template.jinja
  [[ -s "$ORIG" ]] || cp "$S/chat_template.jinja" "$ORIG"
  python3 "$HERE/make_chat_template.py" "$ORIG" "$TPL" >/dev/null
  cmp -s "$TPL" "$CKPT/chat_template.jinja" || { log "Chat template -> checkpoint"; rm -f "$CKPT/chat_template.jinja"; cp "$TPL" "$CKPT/chat_template.jinja"; }

  # ---------------------------------------------------------------- 5. weights for this GB10's rank
  local key
  key=$(cpu --entrypoint bash "$IMAGE" -c "tf-dsv41-rank-cache $CKPT /tmp/x $R 2 --index-only | sha256sum | cut -c1-20")
  if [[ ! -f "$RANK_CACHE/rank${R}of2-$key.bin" ]]; then
    log "Weights of rank $R (~106 GB, CPU)"
    rm -f "$RANK_CACHE"/rank${R}of2-*.bin "$RANK_CACHE"/.rank${R}of2.partial
    cpu --entrypoint tf-dsv41-rank-cache "$IMAGE" "$CKPT" "$RANK_CACHE" "$R" 2 > "$LOGS/rank-cache-$R.log" 2>&1 ||
      die "weights of rank $R failed ($LOGS/rank-cache-$R.log)"
  fi
  # What the app's cluster check looks for: this GB10 is prepared for this engine and these weights.
  echo "$ENGINE_COMMIT $REVISION" > "$WORK/.ready"
  log "Rank $R ready on $(hostname)"
}

if [[ "${1:-}" == --node ]]; then node "${2:-}"; exit 0; fi

need_link
log "This GB10: rank 0 on $IFACE ($MASTER_ADDR). Other GB10: $PEER. RDMA: $HCAS"
w true || die "passwordless SSH to $PEER refused. The cluster setup in NVIDIA Sync turns it on."

# ---------------------------------------------------------------- 1. image on both GB10
docker image inspect "$IMAGE" >/dev/null 2>&1 || { log "Building $IMAGE (about an hour)"; ./build.sh; }
if ! w "docker image inspect $IMAGE >/dev/null 2>&1"; then
  log "Image -> $PEER"
  docker save "$IMAGE" | wi "docker load" >/dev/null
fi

# Same recipe on the other GB10, then each one prepares itself (in parallel: downloads, sha256, rank weights)
w "mkdir -p $WORK/recipe"
tar -C "$HERE" -cf - . | wi "tar -C $WORK/recipe -xf -"
node 0 &
local_job=$!
w "bash $WORK/recipe/prepare.sh --node 1" 2>&1 | sed 's/^/  [rank 1] /' &
remote_job=$!
ok=1
wait "$local_job" || ok=0
wait "$remote_job" || ok=0
(( ok )) || die "preparation failed; run it again to resume"
w "test \"\$(cat $WORK/.ready 2>/dev/null)\" = \"$ENGINE_COMMIT $REVISION\"" || die "the other GB10 did not finish"
log "Ready: both GB10 have the image, weights, Engram tables, kit, token map and rank weights. Start it with ./run.sh"
