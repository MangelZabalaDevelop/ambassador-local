# Ambassador Local

A local inference launcher for the **Dell Pro Precision Ambassador** community.
Pick your operating system, your workstation and a model, press **Run**, and the app starts
`llama-server` (llama.cpp) on `http://127.0.0.1` with a recipe already tested on your hardware.

## How to use it

1. Install the app (Windows: `.exe` · Linux x64: `.deb`/`.AppImage` · GB10: ARM64 `.deb`).
2. **System → Hardware → Family → Model → Run.** System and hardware are detected for you.
3. When it says *Running*:
   - **Open chat** opens the llama.cpp web chat.
   - **Copy URL for Hermes**: in `hermes setup` pick *Custom OpenAI-compatible endpoint* and paste the URL (`http://127.0.0.1:PORT/v1`).

## Security

- The server listens on **127.0.0.1 only**; nobody else on your network can reach it.
- Downloads come only from `github.com` (official llama.cpp builds) and `huggingface.co`, redirects included.
- Every file is checked against the **SHA256** in the catalog. If it doesn't match, it is deleted and never run.
- Recipes are **data, not commands**: they can only set the fields below. The app always adds `--host 127.0.0.1` and `--jinja`.
- No admin rights, no telemetry.
- **Updates are optional and signed.** At startup the app asks GitHub once whether this repo has a newer release (nothing is downloaded or sent). It installs only when you click **Update** and confirm, and only a package signed with the project's key: the public key is built into the app, so a tampered or third-party package is rejected. New models come with the update, since the catalog is part of the app.

Files live in the app data folder
(Windows: `%LOCALAPPDATA%\io.github.ambassadorlocal.app`, Linux: `~/.local/share/io.github.ambassadorlocal.app`).

## Adding recipes

Everything lives in [`recipes/catalog.json`](recipes/catalog.json). It points to [`recipes/catalog.schema.json`](recipes/catalog.schema.json), so editors like VS Code autocomplete every field and flag mistakes while you type.

- `engine`: the pinned llama.cpp version and its files per OS/architecture.
- `hardware`: the program's workstations. `gpu_match` is used to detect them with `nvidia-smi`. Product images go in `public/hardware/<id>.webp`.
- `families`: the lab cards (name, series, description, color). Logos go in `src/assets/labs/<id>.svg`.
- `models`: each model with its GGUF files and one `recipe` per hardware id. A model without a recipe for a workstation is hidden on that workstation.

What ambassadors see in the model table (no jargon, all icons):

| Field | Where | Shown as |
|---|---|---|
| `size_class` | model | Size: `small` (4–14B), `medium` (24–32B), `large` (~70B, up to 96 GB), `xlarge` (two linked GB10) |
| `intelligence` (1–5) | model | Light bulbs |
| `best_for` (1–3 labels) | model | Tags, e.g. `["Chat", "Coding"]` |
| `speed` (1–5) | recipe | Lightning bolts, per workstation |
| `tokens_per_sec` | recipe | Average tokens/s you measured on that workstation (optional; shows "Not measured yet") |
| `ctx_size` | recipe | Memory: books (8K = 1 … over 64K = 5) and approximate words |

Allowed recipe fields:

| Field | llama-server | Default |
|---|---|---|
| `speed` (1–5) | — (only for the table) | required |
| `tokens_per_sec` | — (only for the table) | not measured |
| `ctx_size` (512–262144) | `-c` | required |
| `quant` + `files` | `-m` (other GGUF for this workstation) | the model's `quant` and `files` |
| `mtp.draft_n_max` (1–16) | `--spec-type draft-mtp --spec-draft-n-max` (MTP speculative decoding) | off |
| `mtp.draft` | `-md` (separate `mtp-*.gguf`; leave out when the MTP head is inside the model) | — |
| `gpu_layers` | `-ngl` | 999 |
| `flash_attn` | `-fa on/off` | `true` |
| `mmap` | `--no-mmap` when `false` | `true` |
| `temp`, `top_p`, `top_k`, `min_p` | sampling | llama.cpp defaults |

The app always runs one slot (`-np 1`): one user gets the whole context and the full speed.

Each file needs a URL pinned to a commit (`/resolve/<commit-sha>/file.gguf`), its `sha256` and its `size`.
Hugging Face lists them at `https://huggingface.co/api/models/<repo>/tree/main` (`lfs.oid` is the sha256).

Validate the catalog:

```bash
cd src-tauri && cargo test
```

Real end-to-end test (downloads the engine and the first model, starts it and asks it something):

```bash
cd src-tauri && cargo test --release -- --ignored --nocapture
```

## Building from source

Requirements: Node 22+, stable Rust and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev
```

Installer:

```bash
npm run tauri build
```

Pushing a `v*` tag makes GitHub Actions build Windows x64, Linux x64 and Linux ARM64 (GB10) and create a draft release.

## Shipping an update

Ambassadors get a new version (new models or features) when you publish a release:

1. Bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`.
2. Push a tag, e.g. `git tag v0.2.0 && git push origin v0.2.0`. Actions builds and **signs** every installer and adds `latest.json`, all in a **draft** release.
3. Test the draft installers on your workstations. The release notes you write are shown in the app.
4. Click **Publish**. Only then does the app offer the update (it reads `releases/latest/download/latest.json`; drafts are invisible to it).

Signing needs two repository secrets: `TAURI_SIGNING_PRIVATE_KEY` (the contents of the private key file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never commit the private key, and keep a backup: if it is lost, installed apps can no longer receive updates. The matching public key is `plugins.updater.pubkey` in `tauri.conf.json`.

The repository must be public for ambassadors' apps to reach the releases.

## Credits

Lab logos from [Lobe Icons](https://github.com/lobehub/lobe-icons) (MIT). Logos and names are trademarks of their respective owners.
