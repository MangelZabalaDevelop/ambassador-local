# GLM-5.3-Flash on one GB10

| | |
|---|---|
| Model | GLM-5.3-Flash (reads text and images) |
| Speed | 19.6 tok/s |
| Disk | 109 GB: 88 GB of model + 21 GB of engine |
| Context | 256K tokens |

To go faster it uses a small helper model whose license allows **noncommercial use only** (CC BY-NC-ND 4.0).

## What you need

- A Dell Pro Max with GB10 with DGX OS and Docker.
- The engine: `docker pull` the image in `config.sh`.
- The model and its helper:

  ```bash
  hf download turboderp/GLM-5.3-Flash-exl3 --revision 51058cd551c7e570d87bd32a4adee720edce2349
  hf download incoai/GLM-5.3-Flash-DFlash2 --revision bf582e4eacc1810f76656d1811693ff6c6737d2a
  ```

- 110 GB of free memory: stop any other model first.

## Start it

```bash
./run.sh            # Ctrl+C stops it
./stop.sh           # from another terminal, if you need to
```

It answers at `http://127.0.0.1:8000/v1`, model `glm-5.3-flash`. To use it from another computer:
`HOST=0.0.0.0 API_KEY_FILE=~/key.txt ./run.sh`.

It takes about 3.5 minutes to start.

Based on [glm-5.3-one-spark](https://github.com/gitcommit90/glm-5.3-one-spark) (MIT), with a fix so it starts
faster.
