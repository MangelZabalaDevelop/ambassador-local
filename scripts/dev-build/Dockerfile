# Builder for development builds of Ambassador Local on Linux (scripts/dev-build.sh): the Ubuntu 24.04 the release
# CI builds on, with Tauri's build dependencies, Node 22 and Rust. Nothing from this image is shipped.
FROM ubuntu:24.04@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8
ARG TARGETARCH
ARG NODE_VERSION=v22.23.5
ARG RUSTUP_VERSION=1.29.3
ARG RUST_TOOLCHAIN=1.99.1
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      build-essential pkg-config file curl ca-certificates xz-utils libwebkit2gtk-4.1-dev librsvg2-dev libssl-dev \
 && rm -rf /var/lib/apt/lists/*
RUN case "$TARGETARCH" in \
      arm64) node=linux-arm64 node_sha=2babb9eb5264860d540849f4b064a6fbdebcdc323657d9d382ce23d313fd5e09 \
             rust=aarch64-unknown-linux-gnu rust_sha=9f608c142e0e38a78957d0661ba1dfaf0b6fb37184cfaa4b9a0aa57af6184282 ;; \
      amd64) node=linux-x64 node_sha=cdeb8670a0c81f51b4dc4ac61919f9b1ad917b304bf8221eb939570663d240f8 \
             rust=x86_64-unknown-linux-gnu rust_sha=b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55 ;; \
      *) echo "no builder for $TARGETARCH" >&2; exit 1 ;; \
    esac \
 && curl -fsSLo /tmp/node.tar.xz "https://nodejs.org/dist/$NODE_VERSION/node-$NODE_VERSION-$node.tar.xz" \
 && echo "$node_sha  /tmp/node.tar.xz" | sha256sum -c - \
 && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --exclude='*.md' --exclude=LICENSE \
 && curl -fsSLo /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/$rust/rustup-init" \
 && echo "$rust_sha  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo /tmp/rustup-init -y --no-modify-path --profile minimal \
      --default-toolchain "$RUST_TOOLCHAIN" \
 && rm -f /tmp/node.tar.xz /tmp/rustup-init
# The toolchain stays read-only in /opt; dev-build.sh points CARGO_HOME (the crate cache) at a mounted folder.
ENV RUSTUP_HOME=/opt/rustup PATH=/opt/cargo/bin:$PATH
