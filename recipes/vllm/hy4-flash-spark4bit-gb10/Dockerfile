# Tencent's llama.cpp for Hy4-flash on the GB10: upstream llama.cpp at the commit AngelSlim pins, plus their
# hy4-flash-spark.patch (Hy4-flash support, SPARK-4Bit kernels tuned for the GB10, the model's MTP draft head).
# Hy4-flash is not in a llama.cpp release yet (ggml-org/llama.cpp#32517), so the engine is built here for sm_121a.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}
ARG LLAMA_COMMIT=2cbca389f515df455412dafa07e2b1359cc72a6d
ARG JOBS=12
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake git ca-certificates libgomp1 \
 && rm -rf /var/lib/apt/lists/*
ADD --chmod=644 --checksum=sha256:6ec50722c8b181222a928e0a66cda0159e8dff3e72f8193334f6296f6213dfff \
  https://huggingface.co/AngelSlim/Hy4-flash-GGUF/resolve/65acb8ae7f59847fe03a17291f1b72b89b141a9e/patch/hy4-flash-spark.patch \
  /src/hy4-flash-spark.patch
RUN git init -q /src/llama.cpp && cd /src/llama.cpp \
 && git fetch -q --depth 1 https://github.com/ggml-org/llama.cpp.git "$LLAMA_COMMIT" \
 && git checkout -q FETCH_HEAD && [ "$(git rev-parse HEAD)" = "$LLAMA_COMMIT" ] \
 && git apply /src/hy4-flash-spark.patch \
 && cmake -S . -B build -DGGML_CUDA=ON -DGGML_NATIVE=OFF -DCMAKE_BUILD_TYPE=Release -DCMAKE_CUDA_ARCHITECTURES=121a \
      -DBUILD_SHARED_LIBS=OFF -DLLAMA_OPENSSL=OFF -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF \
 && cmake --build build --config Release --target llama-server --parallel "$JOBS" \
 && install -m 755 build/bin/llama-server /usr/local/bin/llama-server \
 && cd / && rm -rf /src/llama.cpp
LABEL io.github.ambassadorlocal.engine="ggml-org/llama.cpp@2cbca389f515df455412dafa07e2b1359cc72a6d+AngelSlim/hy4-flash-spark.patch@65acb8a"
ENTRYPOINT ["/usr/local/bin/llama-server"]
