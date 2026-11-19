# Hy4-flash on one GB10

| | |
|---|---|
| Model | Tencent Hy4-flash, 220B with 12B active per token (reads text and images) |
| Speed | measured on one GB10: see `tokens_per_sec` in recipes/catalog.json |
| Disk | 161 GB of model + 7 GB of engine |
| Context | 192K tokens |

Tencent's own setup for Hy4-flash on the GB10 ([README_SPARK-4Bit.md](https://huggingface.co/AngelSlim/Hy4-flash-GGUF/blob/main/README_SPARK-4Bit.md)):
their SPARK-4Bit-Fast GGUF (experts in NVIDIA's 4-bit float format) on llama.cpp with their patch, which adds
Hy4-flash, kernels tuned for the GB10 and the model's MTP draft head. The Engram tables (63.5B of the 220B parameters)
stay on disk and are read on demand, so the model fits in one GB10 with memory to spare.

On the GB10 this is faster than the two-GB10 vLLM recipe (`hy4-flash-nvfp4-gb10x2`) for one user, on prose most of
all, and leaves the second GB10 free. vLLM reads long prompts about twice as fast.

## What you need

- A Dell Pro Max with GB10 with DGX OS and Docker.
- The engine's base image and the model:

  ```bash
  docker pull nvidia/cuda@sha256:b7ae301dea2c162444795462ce17a05f6a516e5a75944b57af5b88540a1a2266   # 13.0.3-devel-ubuntu24.04
  hf download AngelSlim/Hy4-flash-GGUF Hy4-flash-SPARK-4Bit-Fast.gguf mmproj-Hy4-flash-bf16.gguf \
    --revision 65acb8ae7f59847fe03a17291f1b72b89b141a9e
  ```

- 110 GB of free memory: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `hy4-flash`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start builds Tencent's llama.cpp into an image (about 4 minutes); later starts reuse it and load the model
in about 3 minutes. It serves one request at a time.

Thinking is on by default. To answer directly, send `"chat_template_kwargs": {"reasoning_effort": "no_think"}`.
Tencent recommends `temperature` 0.9 and `top_p` 1.0.

## License

Hy4-flash is released by Tencent under the Tencent Hy License (Apache 2.0 with extra commercial terms for very large
services); the GGUF builds keep it. Read it before you use the model:
[LICENSE](https://huggingface.co/AngelSlim/Hy4-flash-GGUF/blob/main/LICENSE). llama.cpp is MIT.
