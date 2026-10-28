# llama.cpp chat page

The chat page llama-server brings, from llama.cpp `b11827` (the engine version in `recipes/catalog.json`), MIT
licensed (see `LICENSE`). The app serves it for the models that run in containers (vLLM, TensorFold), which only have
the API: see `src-tauri/src/proxy.rs`.

When the engine version changes, copy the page again from a running llama-server of that version:

```bash
scripts/vendor-llama-ui.sh http://127.0.0.1:8080
```
