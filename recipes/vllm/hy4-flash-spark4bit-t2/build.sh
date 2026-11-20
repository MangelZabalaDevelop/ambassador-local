#!/bin/bash
# build.sh: builds the engine image (Tencent's llama.cpp with their Hy4-flash patch, from IMAGE + Dockerfile) if it is
# not there yet, pulling IMAGE first if needed. The app runs it before run.sh; run.sh runs it too. A few minutes, once.
# build.sh --check only answers: exit 0 if the image is already built.
# No --progress flag: Ubuntu's docker.io has no buildx, and its legacy builder rejects it (its output is plain anyway).
set -euo pipefail
cd "$(dirname "$0")" || exit 1; source ./config.sh

docker image inspect "$FIXED_IMAGE" >/dev/null 2>&1 && exit 0
[[ "${1:-}" == --check ]] && exit 1
docker info >/dev/null 2>&1 || die "Docker is not reachable without sudo"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  log "Pulling the engine's base image $IMAGE"
  docker pull "$IMAGE" || die "could not pull $IMAGE"
fi
log "Building $FIXED_IMAGE (Tencent's llama.cpp for Hy4-flash, a few minutes the first time)"
docker build --build-arg BASE_IMAGE="$IMAGE" -t "$FIXED_IMAGE" - < Dockerfile 2>&1 \
  || die "could not build $FIXED_IMAGE"
log "Built $FIXED_IMAGE"
