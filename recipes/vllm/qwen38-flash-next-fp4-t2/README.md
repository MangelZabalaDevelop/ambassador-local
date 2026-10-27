# Qwen3.8 Flash Next FP4 on the Dell Pro Max Tower T2 (vLLM)

A Miguel Tuned recipe: NVIDIA's FP4-Local checkpoint of Qwen3.8 Flash Next on one RTX PRO 6000 Blackwell (96 GB),
served by vLLM with b12x kernels and MTP speculative decoding.

| | |
|---|---|
| Weights | [`nvidia/Qwen3.8-Flash-Next-FP4-Local`](https://huggingface.co/nvidia/Qwen3.8-Flash-Next-FP4-Local) @ `b9ce18a`, ~102 GB |
| Engine | `ghcr.io/local-inference-lab/vllm`, pinned by digest |
| Serving | 256K context, up to 4 requests at once, MTP drafting 3 tokens, thinking on (effort `xhigh`) |

## What you need

- Ubuntu with Docker and the NVIDIA container runtime, usable without sudo.
- The engine image: `docker pull` the image in `config.sh`.
- The weights in your Hugging Face cache: `hf download nvidia/Qwen3.8-Flash-Next-FP4-Local --revision b9ce18a6887e18c089fdea8509a33e63429a3300`
- A free GPU: stop any other model first.

## Steps

```bash
./run.sh            # stays in the foreground with the log; Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

The API is OpenAI-compatible at `http://127.0.0.1:8000/v1`, model `qwen3.8-flash-next`. To reach it from your
network, serve on every interface with a key: `HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start compiles kernels for your GPU and keeps them in `~/.cache/ambassador-local/vllm/qwen38-flash-next-fp4-t2/cache`;
later starts reuse them.
