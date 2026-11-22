# Inkling 2 Mini on the Dell Pro Max Tower T2 (SGLang)

| | |
|---|---|
| Model | Thinking Machines Inkling 2 Mini, 26B with 3.5B active per token (reads text and images) |
| Weights | [`thinkingmachines/Inkling-2-Mini`](https://huggingface.co/thinkingmachines/Inkling-2-Mini) @ `ed40e7d`, 20 GB (NVFP4 experts, MXFP8 elsewhere) |
| Engine | SGLang's Inkling 2 Mini build, `lmsysorg/sglang` pinned by digest |
| Context | 1M tokens, one request at a time |

Inkling 2 Mini is a looped model: its middle layers run three times per token, so it reasons like a much larger
model with few active parameters. This recipe follows SGLang's RTX PRO 6000 setup for it: Thinking Machines' own
checkpoint with the DSpark draft head that ships inside it, drafting 8 tokens per step. On Linux the app runs this
recipe; on Windows it runs Unsloth's GGUF with llama.cpp instead (256K context).

Measured on an RTX PRO 6000 Blackwell, one request at a time:

| Engine | Decode | Reads a 16K-token prompt |
|---|---|---|
| SGLang, DSpark 8 (this recipe) | 200–215 tok/s | ~14,000 tok/s |
| SGLang, no draft | 165 tok/s | ~14,500 tok/s |
| vLLM, no draft (its DSpark has no kernels for this card yet) | 171 tok/s | ~2,400 tok/s |
| llama.cpp b12613, Unsloth UD-Q8_K_XL (the Windows recipe) | 81.5 tok/s | ~1,850 tok/s |
| llama.cpp b12613, Unsloth UD-Q4_K_XL | 88.3 tok/s | ~2,000 tok/s |

SGLang gets 80% of the card: 20 GB of weights and draft head, then about 50 GB of KV cache, enough for 1M tokens
plus a 5% share for the sliding-window layers. The GB10 recipe, with the same engine and settings, read a
600K-token prompt and recalled the fact buried in it.

## What you need

- Ubuntu with Docker and the NVIDIA container runtime, usable without sudo.
- The engine image and the model:

  ```bash
  docker pull lmsysorg/sglang@sha256:efac853b46811ccb760545b7b29e7119bdf907537ccf6e863fac3011fe8b98de
  hf download thinkingmachines/Inkling-2-Mini --revision ed40e7da625f594fc9c4990f5f66be24873c22d7
  ```

  Ambassador Local does this by itself when you press Run; these commands are for running the recipe by hand.

- A free GPU: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `inkling-2-mini`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`. The key goes to SGLang in a private config file, never on the
command line.

The first start compiles kernels for your GPU and keeps them in `~/.cache/ambassador-local/vllm/inkling2-mini-nvfp4-t2/cache`
(a few minutes); later starts reuse them.

Thinking is on by default (effort 0.9). For shorter thinking send `"chat_template_kwargs": {"reasoning_effort": 0.3}`
(0.0 to 0.99; even 0.0 thinks a little). Thinking Machines uses `temperature` 1.0. The model can get stuck re-checking
its own work on hard debugging tasks, so cap `max_tokens` in agents.

## License

Inkling 2 Mini is released by Thinking Machines Lab under Apache 2.0, with a
[Model Acceptable Use Policy](https://thinkingmachines.ai/model-acceptable-use-policy/). Unsloth's GGUF builds keep
the license. SGLang is Apache 2.0; llama.cpp is MIT.
