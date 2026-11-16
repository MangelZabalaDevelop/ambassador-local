# gitcommit90's published image with the recipe's later weight-loader fix (repo commit 952310a, overlay/exl3.py).
# The image copies each of 148,608 expert tensors to the GPU straight from the safetensors mmap (~3.4 ms each, about
# 12 minutes per start); cloning into RAM first takes ~0.35 ms. Only those two lines are applied here: the repo's
# exl3.py lacks other fixes the image adds at build time, so the file is not replaced.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}
RUN python3 - <<'EOF'
from pathlib import Path
p = Path("/usr/local/lib/python3.12/dist-packages/vllm/model_executor/layers/quantization/exl3.py")
s = p.read_text()
fixes = [
    ("        dest.copy_(loaded_weight)\n\n    def process_weights_after_loading",
     "        if loaded_weight.device.type == \"cpu\":\n"
     "            loaded_weight = loaded_weight.clone()  # load-time fix\n"
     "        dest.copy_(loaded_weight)\n\n    def process_weights_after_loading"),
    ("        loaded = loaded_weight.detach().contiguous()\n",
     "        loaded = loaded_weight.detach()\n"
     "        loaded = loaded.clone() if loaded.device.type == \"cpu\" else loaded.contiguous()  # load-time fix\n"),
]
for old, new in fixes:
    assert s.count(old) == 1, f"expected exactly one match for: {old[:60]!r}"
    s = s.replace(old, new)
p.write_text(s)
EOF
