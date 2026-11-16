# Qwen3.8 Flash Next FP4 on one GB10

| | |
|---|---|
| Model | Qwen3.8 Flash Next FP4 |
| Speed | 36.9 tok/s |
| Disk | 120 GB: 102 GB of model + 18 GB of engine |
| Context | 256K tokens |

## What you need

- A Dell Pro Max with GB10 with DGX OS and Docker.
- The engine: `docker pull` the image in `config.sh`.
- The model: `hf download nvidia/Qwen3.8-Flash-Next-FP4-Local --revision b9ce18a6887e18c089fdea8509a33e63429a3300`
- 100 GB of free memory: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `qwen3.8-flash-next`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start takes about 17 minutes; later starts are much faster.

Based on NVIDIA's single-Spark recipe for this model.
