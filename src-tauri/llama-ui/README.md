# llama.cpp chat page

The chat page llama-server brings, from llama.cpp `b12359`, MIT licensed (see `LICENSE`). The app serves it for the
models that run in containers (vLLM, SGLang, TensorFold), which only have the API: see `src-tauri/src/proxy.rs`.

From `b12600` llama.cpp splits the page into about 220 files, and the proxy only serves the one-bundle layout, so this
copy stays on `b12359` even when the engine in `recipes/catalog.json` is newer. Models that run on llama.cpp get the
page of their own engine from llama-server.

To copy a one-bundle page again from a running llama-server:

```bash
scripts/vendor-llama-ui.sh http://127.0.0.1:8080
```
