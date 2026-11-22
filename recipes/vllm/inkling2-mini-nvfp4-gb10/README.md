# Inkling 2 Mini on one GB10 (SGLang)

| | |
|---|---|
| Model | Thinking Machines Inkling 2 Mini, 26B with 3.5B active per token (reads text and images) |
| Weights | [`thinkingmachines/Inkling-2-Mini`](https://huggingface.co/thinkingmachines/Inkling-2-Mini) @ `ed40e7d`, 20 GB (NVFP4 experts, MXFP8 elsewhere) |
| Engine | SGLang's Inkling 2 Mini build, `lmsysorg/sglang` pinned by digest |
| Speed | measured on one GB10: see `tokens_per_sec` in recipes/catalog.json |
| Context | 1M tokens, one request at a time |

Inkling 2 Mini is a looped model: its middle layers run three times per token, so it reasons like a much larger
model with few active parameters. This recipe follows SGLang's DGX Spark setup for it: Thinking Machines' own
checkpoint with the DSpark draft head that ships inside it, drafting 8 tokens per step.

Measured on one GB10, one request at a time:

| Engine | Decode | Reads a 16K-token prompt |
|---|---|---|
| SGLang, DSpark 8 (this recipe) | 37.6 tok/s | ~2,700 tok/s |
| SGLang, no draft | 34.1 tok/s | ~2,750 tok/s |
| vLLM, DSpark 7 (the only count it takes) | 38.0 tok/s | ~1,170 tok/s |
| vLLM, no draft | 34.9 tok/s | ~2,400 tok/s |
| llama.cpp b12613, Unsloth UD-Q4_K_XL (no draft head in a release yet) | 30.4 tok/s | ~350 tok/s |

Decode on the GB10 is bound by memory bandwidth, so the container engines land near the same speed; with its draft
head, SGLang reads long prompts more than twice as fast as vLLM. A 600K-token prompt was read in under 14 minutes and its buried fact was
recalled correctly.

## What you need

- A Dell Pro Max with GB10 with DGX OS and Docker.
- The engine image and the model:

  ```bash
  docker pull lmsysorg/sglang@sha256:efac853b46811ccb760545b7b29e7119bdf907537ccf6e863fac3011fe8b98de
  hf download thinkingmachines/Inkling-2-Mini --revision ed40e7da625f594fc9c4990f5f66be24873c22d7
  ```

  Ambassador Local does this by itself when you press Run; these commands are for running the recipe by hand.

- 85 GB of free memory: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `inkling-2-mini`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`. The key goes to SGLang in a private config file, never on the
command line.

The first start compiles kernels for the GB10 and keeps them in `~/.cache/ambassador-local/vllm/inkling2-mini-nvfp4-gb10/cache`
(about 5 minutes); later starts reuse them.

Thinking is on by default (effort 0.9). For shorter thinking send `"chat_template_kwargs": {"reasoning_effort": 0.3}`
(0.0 to 0.99; even 0.0 thinks a little). Thinking Machines uses `temperature` 1.0. The model can get stuck re-checking
its own work on hard debugging tasks, so cap `max_tokens` in agents.

## License

Inkling 2 Mini is released by Thinking Machines Lab under Apache 2.0, with a
[Model Acceptable Use Policy](https://thinkingmachines.ai/model-acceptable-use-policy/). SGLang is Apache 2.0.
