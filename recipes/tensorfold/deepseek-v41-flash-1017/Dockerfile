# TensorFold engine for DeepSeek-V4.1-Flash-1017 on two linked GB10 (aarch64, sm_121).
# Build with ./build.sh; the tag must match "image" in recipes/catalog.json.
FROM nvcr.io/nvidia/pytorch:26.07-py3@sha256:2140e699b3beaf7f96a0081fd9c9406bc3832b435cdb60dfa2d261f7d2f34a1c

ARG TF_REPO=https://github.com/bertholomus/TensorFold.git
# Branch deepseek-v41-zig, 2026-10-20
ARG TF_REF=6366d2c585b4f05fe33bfe5cbdb33ea15264841a
# Branch deepseek-v41-zig-tp4: tf-dsv41-rank-cache and the kit's input scripts
ARG INPUTS_REF=59402086381a42c74d53053443224eec2790b4c5
ARG ZIG_VERSION=0.17.0
ARG ZIG_SHA256=9e8d11661d4ae3bd57702a3832781e23ad151dde5798e16a5ccd503f65234ff8
ARG HF_HUB_VERSION=0.35.3
ARG JOBS=6

COPY build-engine.sh /tmp/build-engine.sh
RUN TF_REPO=$TF_REPO TF_REF=$TF_REF INPUTS_REF=$INPUTS_REF ZIG_VERSION=$ZIG_VERSION ZIG_SHA256=$ZIG_SHA256 \
    HF_HUB_VERSION=$HF_HUB_VERSION JOBS=$JOBS bash /tmp/build-engine.sh && rm /tmp/build-engine.sh

ENV PATH=/opt/tensorfold/native/bin:/opt/tensorfold/bin:$PATH
WORKDIR /workspace
# The app's cluster check reads this label to confirm the image runs the pinned engine.
LABEL io.github.ambassadorlocal.engine="bertholomus/TensorFold@${TF_REF}" \
      io.github.ambassadorlocal.inputs="${INPUTS_REF}"
