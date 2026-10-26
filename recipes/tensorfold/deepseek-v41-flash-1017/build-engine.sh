#!/bin/bash
# Builds the TensorFold engine with its DeepSeek-V4.1 family (dsv41), unpatched, inside the PyTorch base image.
# Runs once, as root, from the Dockerfile. Every source is pinned to a commit and the Zig toolchain to its sha256.
#   /opt/tensorfold       ReleaseSafe, generic aarch64 (the engine author's build flags): the one run.sh uses
#   /opt/tensorfold-fast  ReleaseFast, native CPU (FLAVOR=fast in run.sh, for A/B tests only)
set -euxo pipefail
: "${TF_REPO:?}" "${TF_REF:?}" "${INPUTS_REF:?}" "${ZIG_VERSION:?}" "${ZIG_SHA256:?}"
JOBS=${JOBS:-6}
NVCC=/usr/local/cuda/bin/nvcc

python3 -m pip install --no-cache-dir safetensors "huggingface_hub==${HF_HUB_VERSION:?}"
curl -fsSL -o /tmp/zig.tar.xz "https://ziglang.org/download/$ZIG_VERSION/zig-aarch64-linux-$ZIG_VERSION.tar.xz"
echo "$ZIG_SHA256  /tmp/zig.tar.xz" | sha256sum -c -
mkdir -p /opt/zig && tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1 && rm /tmp/zig.tar.xz
/opt/zig/zig version

fetch() {
  git init -q "$1"; git -C "$1" remote add origin "$TF_REPO"
  git -C "$1" fetch -q --depth 1 origin "$2"; git -C "$1" checkout -q --detach FETCH_HEAD
  test "$(git -C "$1" rev-parse HEAD)" = "$2"
}

fetch /src/engine "$TF_REF"
cd /src/engine
/opt/zig/zig build install native -Dtarget=aarch64-linux-gnu -Doptimize=ReleaseSafe -Dnvcc=$NVCC -Dsm=121 \
  --prefix /opt/tensorfold -j"$JOBS"
test -x /opt/tensorfold/native/bin/tensorfold-native && test -x /opt/tensorfold/bin/tf-dsv41-lanes
/opt/zig/zig build install native -Doptimize=ReleaseFast -Dnvcc=$NVCC -Dsm=121 --prefix /opt/tensorfold-fast -j"$JOBS"
test -x /opt/tensorfold-fast/native/bin/tensorfold-native && test -x /opt/tensorfold-fast/bin/tf-dsv41-lanes
mkdir -p /opt/tensorfold/share/doc/tensorfold
for f in LICENSE NOTICE ATTRIBUTION.md THIRD_PARTY_NOTICES.md LICENSES tools/dsv41-zig/ATTRIBUTION.md; do
  [ -e "$f" ] && cp -a "$f" /opt/tensorfold/share/doc/tensorfold/ || true
done
echo "$TF_REPO $TF_REF" > /opt/tensorfold/share/doc/tensorfold/SOURCE

# The per-rank weight builder and the kit's input scripts live on another branch of the same repository.
fetch /src/inputs "$INPUTS_REF"
cd /src/inputs
/opt/zig/zig build install -Dtarget=aarch64-linux-gnu -Doptimize=ReleaseSafe -Dnvcc=$NVCC -Dsm=121 --prefix /opt/inputs -j"$JOBS"
cp /opt/inputs/bin/tf-dsv41-rank-cache /opt/tensorfold/bin/
mkdir -p /opt/tensorfold/tools/inputs
cp tools/dsv41-zig/inputs/make_token_map.py tools/dsv41-zig/kit/make_rope.py tools/dsv41-zig/kit/make_bias_vl.py /opt/tensorfold/tools/inputs/
cp LICENSE /opt/tensorfold/share/doc/tensorfold/LICENSE-inputs-branch
echo "$TF_REPO $INPUTS_REF (inputs)" >> /opt/tensorfold/share/doc/tensorfold/SOURCE
rm -rf /src /opt/inputs /root/.cache/zig
echo BUILD-OK
