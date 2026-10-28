#!/usr/bin/env bash
# Copies llama.cpp's chat page from a running llama-server into src-tauri/llama-ui/, where the app serves it to
# vLLM and TensorFold models (src-tauri/src/proxy.rs). Run it with llama-server of the engine version in
# recipes/catalog.json, e.g. after `npm run tauri dev` started a llama.cpp model:
#   scripts/vendor-llama-ui.sh http://127.0.0.1:8080
set -euo pipefail

URL=${1:?usage: vendor-llama-ui.sh <llama-server URL>}
URL=${URL%/}
OUT="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/llama-ui"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

get() { curl -fsS --compressed "$URL/$1" -o "$TMP/$2"; }

get "" index.html
JS=$(grep -oE '_app/immutable/bundle\.[A-Za-z0-9_-]+\.js' "$TMP/index.html" | head -1)
CSS=$(grep -oE '_app/immutable/assets/bundle\.[A-Za-z0-9_-]+\.css' "$TMP/index.html" | head -1)
[ -n "$JS" ] && [ -n "$CSS" ] || { echo "index.html does not load one bundle .js and one .css: the page changed, update proxy.rs" >&2; exit 1; }
get "$JS" bundle.js
get "$CSS" bundle.css
for f in favicon.ico favicon.svg favicon-dark.ico favicon-dark.svg manifest.webmanifest build.json; do get "$f" "$f"; done
get _app/version.json version.json
grep -q '"version"' "$TMP/build.json" || { echo "build.json has no version" >&2; exit 1; }

gzip -9 -n "$TMP/bundle.js" "$TMP/bundle.css"
mkdir -p "$OUT"
cp "$TMP"/{index.html,bundle.js.gz,bundle.css.gz,favicon.ico,favicon.svg,favicon-dark.ico,favicon-dark.svg,manifest.webmanifest,build.json,version.json} "$OUT/"
echo "llama.cpp chat page $(cat "$TMP/build.json") copied to $OUT"
