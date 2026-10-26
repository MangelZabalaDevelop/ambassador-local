# DeepSeek-V4.1-Flash-1017 on two linked GB10 (TensorFold)

A Miguel Tuned recipe: DeepSeek-V4.1-Flash-1017 served tensor-parallel across two Dell Pro Max with GB10, with the
TensorFold engine and its `dsv41` DeepSeek-V4.1 family, from a Docker image you build from pinned sources.

| | |
|---|---|
| Weights | [`LibertAIDAI/DeepSeek-V4.1-Flash-1017-EXL3-2.9bpw`](https://huggingface.co/LibertAIDAI/DeepSeek-V4.1-Flash-1017-EXL3-2.9bpw) @ `0be63c3`, EXL3 2.9 bpw (MIT), ~211 GB on each GB10 |
| Engram tables | shards 47 and 48 of [`deepseek-ai/DeepSeek-V4.1-Flash-1017`](https://huggingface.co/deepseek-ai/DeepSeek-V4.1-Flash-1017) @ `1bae0e1` (MIT), ~203 GB on each GB10 |
| Engine | [`bertholomus/TensorFold`](https://github.com/bertholomus/TensorFold) branch `deepseek-v41-zig` @ `6366d2c`, unpatched |
| Kernel kit | [`bertholomus/deepseek-v4.1-tensorfold-tp2-2xgb10`](https://github.com/bertholomus/deepseek-v4.1-tensorfold-tp2-2xgb10) @ `abe07c7` (Apache-2.0) |
| Base image | `nvcr.io/nvidia/pytorch:26.07-py3`, pinned by digest |
| Serving | 1M context, 2 streams, 262,144 output tokens by default, reasoning effort `max` by default |

Every source is pinned: the base image by digest, the engine and the kit by commit, the Zig toolchain and every
weight file by sha256 (`SHA256SUMS.weights`, `SHA256SUMS.engram`). Nothing is patched; the only change is the chat
template, which `make_chat_template.py` adapts so this engine can compile it.

## What you need

- Two GB10 linked by the ConnectX cable, with the cluster setup from NVIDIA Sync done (passwordless SSH between them).
- Docker with the NVIDIA runtime on both, usable without sudo.
- About 650 GB free on each GB10: weights, Engram tables and the per-rank weight file (~106 GB), plus the 25 GB image.
- No other model running: each GB10 needs 112 GiB of free memory.

## Steps

Run everything on the GB10 that will serve the API. The scripts reach the other GB10 over the direct cable.

```bash
./prepare.sh   # once: builds the image, downloads and checks everything on both GB10 (hours; can be resumed)
./run.sh       # serves on http://127.0.0.1:8090/v1, model "deepseek-v41-flash-1017"; Ctrl+C stops both GB10
./stop.sh      # stops both ranks if run.sh was killed
```

To use it from other devices, serve on the network with a key:

```bash
HOST=0.0.0.0 API_KEY_FILE=~/my-api-key.txt ./run.sh
```

Settings live in `config.sh`; any of them can be overridden from the environment (`CONTEXT`, `PARALLEL`, `PORT`, …).
The ConnectX interface, the other GB10's address and the RDMA devices are detected; set `IFACE`, `PEER` or `HCAS`
if the detection picks the wrong ones. Both GB10 must have the same interface and RDMA device names.

The app's cluster check (Run screen) shows whether both GB10 have the image, the weights and the prepared files.

## Measured

With this folder's `run.sh` on two GB10 (1M context, 2 streams, the settings above), one request at a time, no
reasoning, speeds from the server's own counters, three runs each:

| | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| Decode, 1,500 tokens | 54.0 tok/s | 53.9 tok/s | 53.8 tok/s |
| Time to first token, short prompt | 0.22 s | 0.20 s | 0.19 s |
| Prefill, 25,630-token prompt (no cache) | 1,708 tok/s (15.0 s) | 1,718 tok/s (14.9 s) | 1,695 tok/s (15.1 s) |

Loading both ranks takes about 35 s.

Earlier, with `--context 524288 --parallel 4`, two runs of a small quality and speed suite on the same day:

| | Run 1 | Run 2 |
|---|---|---|
| One request, no reasoning | 54.1 tok/s, 0.12 s to first token | 25.0 tok/s, 0.18 s to first token |
| Two requests at once (total) | 83.9 tok/s | 42.9 tok/s |
| Four requests at once (total) | 114.0 tok/s | 114.2 tok/s |
| Quality suite | 29 of 30 | 29 of 30 |

Run 2's single-request speed was the outlier; the three runs above agree with run 1. The quality suite covers math,
code run in a sandbox, instructions, knowledge, tool calls, images and three needles in ~49K tokens.

## Image

The image is built locally (`build.sh`, about an hour) and copied to the second GB10 with `docker save | docker load`.
It is not published to a registry.
