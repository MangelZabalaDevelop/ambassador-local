#!/bin/bash
# dev-build.sh: builds this checkout as a development version and installs it for the current user as
# "Ambassador Local (dev)", next to the published app, to try a change (a new recipe, say) the way a user would,
# before it goes out in a release. Linux with Docker; no sudo, nothing is published.
#
# The version is the next patch with -dev.<date> (0.1.9 → 0.1.10-dev.202611191530): the release that follows still
# arrives as an update, and the published app is never offered as one. Both apps share their settings and downloads;
# close one before opening the other.
#
#   scripts/dev-build.sh            build and install
#   scripts/dev-build.sh --remove   remove the dev build
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="$HOME/.local/bin/ambassador-local-dev"
DESKTOP="$HOME/.local/share/applications/ambassador-local-dev.desktop"
ICON="$HOME/.local/share/icons/hicolor/128x128/apps/ambassador-local-dev.png"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ambassador-local-dev"   # crates, npm packages and the Rust build

if [[ "${1:-}" == --remove ]]; then
  rm -f "$BIN" "$DESKTOP" "$ICON"
  echo "Removed the dev build (its build cache stays in $CACHE)."
  exit 0
fi

docker info >/dev/null 2>&1 || { echo "Docker is not reachable without sudo." >&2; exit 1; }
base=$(sed -n 's/^  "version": "\([0-9]*\.[0-9]*\)\.\([0-9]*\)".*/\1 \2/p' src-tauri/tauri.conf.json)
[[ -n "$base" ]] || { echo "No version in src-tauri/tauri.conf.json." >&2; exit 1; }
read -r major_minor patch <<< "$base"
VERSION="$major_minor.$((patch + 1))-dev.$(date +%Y%m%d%H%M)"

# The builder image is named after its Dockerfile: a change to it builds a new one.
IMAGE="ambassador-local/devbuild:$(sha256sum scripts/dev-build/Dockerfile | cut -c1-12)"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "Building the builder image $IMAGE (a few minutes, once)"
  docker build -q -t "$IMAGE" - < scripts/dev-build/Dockerfile >/dev/null
fi

mkdir -p "$CACHE"/{cargo,npm,target,home}
echo "Building Ambassador Local $VERSION"
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/src" -v "$CACHE:/cache" -w /src \
  -e HOME=/cache/home -e CARGO_HOME=/cache/cargo -e CARGO_TARGET_DIR=/cache/target -e npm_config_cache=/cache/npm \
  "$IMAGE" bash -c 'npm ci --no-audit --no-fund --loglevel=error && npx tauri build --no-bundle --config "$1"' _ \
  "{\"version\": \"$VERSION\", \"bundle\": {\"createUpdaterArtifacts\": false}}"

mkdir -p "$(dirname "$BIN")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"
install -m 755 "$CACHE/target/release/ambassador-local" "$BIN"
install -m 644 src-tauri/icons/128x128.png "$ICON"
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=Ambassador Local (dev)
Comment=Development build $VERSION, not released yet
Exec=$BIN
Icon=ambassador-local-dev
Categories=Development;
Terminal=false
EOF
command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$DESKTOP")" 2>/dev/null || true
echo "Installed Ambassador Local (dev) $VERSION: open it from the app menu, or run $BIN"
