# vLLM's official Hy4-flash image (arm64, vLLM 77766db with the Hy4-flash pull request, FlashInfer 0.7.4) plus six
# Python files from atakann/dgx-spark-recipes (MIT, images/vllm-hy4-flash @ e764efa), fetched pinned and checked by
# sha256. The image's Hy4-flash code takes FP8 only and needs Hopper or data-center Blackwell for attention; the files add:
#   sm120.py            Hy4's per-head learnable attention sink through FlashInfer's SM120 sparse MLA kernel
#   attention.py        Hy4-flash layers pick that backend on compute capability 12 (sparse and sliding-window)
#   hy_v4_attention.py  the same choice for the DFly drafter's layers
#   mla_attention.py    fp8_ds_mla KV cache on sliding-window MLA (as vllm-project/vllm#68365)
#   model.py            ModelOpt NVFP4 experts: weight, block scale, global scale and input scale for each expert
#   dfly_config.py      DFly accepts a ModelOpt NVFP4 target (the drafter's own layers stay BF16)
# No weights inside.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}
ARG SRC=https://raw.githubusercontent.com/atakann/dgx-spark-recipes/e764efa3efed3e21fe543c3fffc927eb6a00bff8/images/vllm-hy4-flash
ARG V=/usr/local/lib/python3.12/dist-packages/vllm
ADD --chmod=644 --checksum=sha256:b260fc169c926b5f026081a993650bfe43cce6cd53b8b80fdcabbc6e4ec0f5df ${SRC}/sm120.py ${V}/models/hy_v4_flash/nvidia/sm120.py
ADD --chmod=644 --checksum=sha256:b67afe0ff16b384261dec87338c559ae089843ad8bcc094f26671661c119f8b2 ${SRC}/attention.py ${V}/models/hy_v4_flash/nvidia/attention.py
ADD --chmod=644 --checksum=sha256:69865a097d847d446fdcdfb81672249a426a8abfea4bc8da278efb2e79fa2716 ${SRC}/model.py ${V}/models/hy_v4_flash/nvidia/model.py
ADD --chmod=644 --checksum=sha256:269d390eeaec8a6cf5f85c014542ce4426f4d62e904c82499d6d34633c1938dd ${SRC}/hy_v4_attention.py ${V}/models/hy_v4/nvidia/attention.py
ADD --chmod=644 --checksum=sha256:9513efde31a50279fe892ba01d27a546dc15b494fcd7b628639e8f82b26dafe8 ${SRC}/dfly_config.py ${V}/models/hy_v4/dfly_config.py
ADD --chmod=644 --checksum=sha256:9540ec2427fe3d747944165a2d416812f2a8e64c74c062e24c9e1b44e96aec67 ${SRC}/mla_attention.py ${V}/model_executor/layers/attention/mla_attention.py
ENV CUTE_DSL_ARCH=sm_121a VLLM_WORKER_MULTIPROC_METHOD=spawn
