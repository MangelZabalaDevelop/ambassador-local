# Hy4-flash on two GB10

| | |
|---|---|
| Model | Tencent Hy4-flash, 220B with 12B active per token (reads text and images) |
| Speed | measured on two GB10: see `tokens_per_sec` in recipes/catalog.json |
| Disk | on each GB10: 165 GB of model + 25 GB of engine |
| Context | 256K tokens |

Tencent's Hy4-flash in an NVFP4 build (the experts in 4-bit NVFP4, the Engram n-gram tables in FP8), split across two
GB10 with tensor parallelism over the ConnectX cable. It drafts tokens with DFly, the small drafter built into the
model, which is why it is fast on code, JSON and rewrites and slower on free prose.

For one user, Tencent's GGUF on a single GB10 (`hy4-flash-spark4bit-gb10`) is faster, on prose most of all, and
leaves the other GB10 free. This recipe reads long prompts about twice as fast and serves up to four requests at once.

## What you need

- Two Dell Pro Max with GB10 with DGX OS and Docker, linked by the ConnectX cable, with the cluster setup of NVIDIA Sync
  done (it leaves the passwordless SSH between them ready).
- On **each** GB10, the engine and the model:

  ```bash
  docker pull vllm/vllm-openai@sha256:5e2c1badbca60595d6591c60feb4b8547f59da96e7a91710d40239250e721aba
  hf download atakans/Hy4-flash-NVFP4 --revision 922cbf11f2abf547951d22eabc76b876f2a9d03b
  ```

- 108 GB of free memory on each GB10: stop any other model first.

## Start it

On one of the two GB10 (it serves the API; the other one runs the second half of the model):

```bash
./run.sh            # Ctrl+C stops it on both
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `hy4-flash`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start adds six small files to vLLM's image on each GB10 (about a minute) and compiles and tunes kernels
(about 10 minutes); later starts reuse both.

Thinking is on by default. To answer directly, send `"chat_template_kwargs": {"reasoning_effort": "no_think"}`.
Tencent recommends `temperature` 0.9 and `top_p` 1.0.

## License

Hy4-flash is released by Tencent under the Tencent Hy License (Apache 2.0 with extra commercial terms for very large
services); the NVFP4 build keeps it. Read it before you use the model:
[LICENSE](https://huggingface.co/tencent/Hy4-flash/blob/main/LICENSE).

Based on [dgx-spark-recipes](https://github.com/atakann/dgx-spark-recipes) by atakann (MIT), recipe
`llm/hy4-flash-nvfp4-vllm-tp2`, whose six vLLM files the Dockerfile adds. Weights:
[atakans/Hy4-flash-NVFP4](https://huggingface.co/atakans/Hy4-flash-NVFP4).
