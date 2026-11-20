# Hy4-flash on one Dell Pro Max Tower T2

| | |
|---|---|
| Model | Tencent Hy4-flash, 220B with 12B active per token (reads text and images) |
| Speed | measured on one RTX PRO 6000: see `tokens_per_sec` in recipes/catalog.json |
| Disk | 161 GB of model + 12 GB of engine |
| Context | up to 192K tokens, as much as the free GPU memory allows |

The same weights and engine as the GB10 recipe (`hy4-flash-spark4bit-gb10`), built for the RTX PRO 6000 Blackwell:
Tencent's SPARK-4Bit-Fast GGUF (experts in NVIDIA's 4-bit float format) on llama.cpp with their patch, which adds
Hy4-flash, their 4-bit kernels and the model's MTP draft head. About 90 GB goes to the card; the Engram table
(63.5B of the 220B parameters, 59 GiB) stays in system memory and is read on demand.

Measured on an RTX PRO 6000 Blackwell, one request at a time:

- **Answers:** 128.5 tok/s with thinking on, 110–160 tok/s without it. That is about four times the GB10.
- **Long prompts:** read at about 2,700 tok/s, from 4K to 30K tokens.
- **Tuning:**
  - MTP 2 was the fastest of 1–3.
  - Tencent's engine settings (`engine.env`) make long prompts about 10% faster.
- **Rejected:** an 8-bit KV cache lets 256K tokens fit, but breaks the MTP drafts and halves the speed.

## What you need

- A Dell Pro Max Tower T2 with Linux, Docker and the NVIDIA Container Toolkit. Ubuntu's `docker.io` package is
  enough; buildx is not needed.
- About 60 GB of free system memory, for the Engram table.
- The engine's base image and the model:

  ```bash
  docker pull nvidia/cuda@sha256:b7ae301dea2c162444795462ce17a05f6a516e5a75944b57af5b88540a1a2266   # 13.0.3-devel-ubuntu24.04
  hf download AngelSlim/Hy4-flash-GGUF Hy4-flash-SPARK-4Bit-Fast.gguf mmproj-Hy4-flash-bf16.gguf \
    --revision 65acb8ae7f59847fe03a17291f1b72b89b141a9e
  ```

  Ambassador Local does this by itself when you press Run, and builds the engine too. These commands are for running
  the recipe by hand.

Not on Windows yet: Hy4-flash is not in a llama.cpp release, and the app's Windows engine is the official release.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `hy4-flash`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start builds Tencent's llama.cpp into an image, which takes a few minutes. Later starts reuse it. It serves
one request at a time.

**Context:** the weights leave only about 6 GB of the 96 GB card for context and work memory. Anything else on the
card, like the desktop or a browser, takes from that. So `run.sh` checks the free memory first and picks the largest
context that fits:

| Free GPU memory | Context |
|---|---|
| 96,950 MiB | 192K tokens |
| 94,950 MiB | 128K tokens |
| 92,850 MiB | 64K tokens |

`CONTEXT=131072 ./run.sh` sets it by hand.

**Thinking:** it is on by default. To answer directly, send `"chat_template_kwargs": {"reasoning_effort": "no_think"}`.
Tencent recommends `temperature` 0.9 and `top_p` 1.0.

## License

Hy4-flash is released by Tencent under the Tencent Hy License (Apache 2.0 with extra commercial terms for very large
services); the GGUF builds keep it. Read it before you use the model:
[LICENSE](https://huggingface.co/AngelSlim/Hy4-flash-GGUF/blob/main/LICENSE). llama.cpp is MIT.
