# DeepSeek V4 Flash Vision on one GB10

| | |
|---|---|
| Model | DeepSeek V4 Flash Vision (reads text and images) |
| Speed | 23.3 tok/s |
| Disk | 138 GB: 116 GB of model + 22 GB of engine |
| Context | 240K tokens |

This version of the model has its refusals removed by its authors.

## What you need

- A Dell Pro Max with GB10 with DGX OS and Docker.
- The engine: `docker pull` the image in `config.sh`.
- The model: `hf download GaelicThunder/DeepSeek-V4-Flash-Vision-Exp-ablit-EXL3-Kalibrated --revision 178652ce2ef562c9135488def954a2418db6f5e5`
- 110 GB of free memory: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `deepseek-v4-flash-vision`. Send images inside the chat message.
To use it from another computer: `HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

The first start takes about 25 minutes; later starts take about 90 seconds. The model uses almost all the memory,
so `run.sh` also watches it and stops the model before the GB10 runs out.

Based on [DeepSeek-V4-Flash-Vision-One-DGX-Spark](https://github.com/GaelicThunder/DeepSeek-V4-Flash-Vision-One-DGX-Spark)
(Apache-2.0), with a fix so it starts.
