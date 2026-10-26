#!/bin/bash
# build.sh: builds the engine image on this GB10 (about an hour; the base image is ~25 GB).
# prepare.sh calls it when the image is missing and then copies the image to the second GB10.
set -euo pipefail
cd "$(dirname "$0")"; source ./config.sh
# A GB10 shares its RAM with the GPU: few parallel compile jobs, so a build never starves a model that is serving.
docker build --build-arg JOBS="${JOBS:-6}" -t "$IMAGE" .
docker image inspect -f '{{index .Config.Labels "io.github.ambassadorlocal.engine"}}' "$IMAGE"
log "Built $IMAGE"
