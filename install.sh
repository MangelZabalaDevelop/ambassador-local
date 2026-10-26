#!/usr/bin/env bash
# Ambassador Local installer for Linux (Dell Pro Max Tower T2 and GB10, Ubuntu 24.04 / DGX OS).
#
#   curl -fsSL https://raw.githubusercontent.com/MangelZabalaDevelop/ambassador-local/main/install.sh | bash
#
# Downloads the .deb for this CPU from the latest GitHub release, checks its SHA256
# against the one GitHub publishes for that file, and installs it with apt.
set -euo pipefail

REPO="MangelZabalaDevelop/ambassador-local"

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Linux ] || die "This installer is for Linux. On Windows, download the installer from https://github.com/$REPO/releases/latest"
command -v apt-get >/dev/null || die "This installer needs Ubuntu or another Debian-based Linux (apt)."
command -v python3 >/dev/null || die "python3 is required (it comes with Ubuntu)."
case "$(uname -m)" in
  x86_64) ARCH=amd64 ;;
  aarch64 | arm64) ARCH=arm64 ;;
  *) die "Unsupported CPU: $(uname -m)" ;;
esac

# While the repository is private, GitHub only answers to a signed-in user: use the GitHub CLI login if there is one.
TOKEN="${GITHUB_TOKEN:-}"
if [ -z "$TOKEN" ] && command -v gh >/dev/null; then TOKEN="$(gh auth token 2>/dev/null || true)"; fi
AUTH=()
[ -n "$TOKEN" ] && AUTH=(-H "Authorization: Bearer $TOKEN")

say "Looking for the latest Ambassador Local release"
JSON="$(curl -fsSL "${AUTH[@]}" -H "Accept: application/vnd.github+json" "https://api.github.com/repos/$REPO/releases/latest")" \
  || die "Could not reach the release on GitHub. If the repository is private, sign in first with: gh auth login"

# Tab-separated, because the .deb name has a space in it.
IFS=$'\t' read -r TAG NAME URL DIGEST SIZE < <(printf '%s' "$JSON" | python3 -c '
import json, sys
r = json.load(sys.stdin)
arch = sys.argv[1]
for a in r.get("assets", []):
    if a["name"].endswith("_" + arch + ".deb"):
        print("\t".join([r["tag_name"], a["name"], a["url"], a.get("digest") or "", str(a["size"])]))
        break
' "$ARCH") || true
[ -n "${URL:-}" ] || die "The latest release has no Linux $ARCH package."
case "$DIGEST" in sha256:*) WANT="${DIGEST#sha256:}" ;; *) die "GitHub did not publish a SHA256 for $NAME, so it cannot be verified." ;; esac

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
DEB="$TMP/ambassador-local_${ARCH}.deb"

say "Downloading $NAME ($TAG, $((SIZE / 1048576)) MB)"
curl -fL --progress-bar "${AUTH[@]}" -H "Accept: application/octet-stream" "$URL" -o "$DEB" || die "Download failed."

say "Checking SHA256"
HAVE="$(sha256sum "$DEB" | cut -d' ' -f1)"
[ "$HAVE" = "$WANT" ] || die "SHA256 does not match (expected $WANT, got $HAVE). Nothing was installed."

say "Installing (your password may be asked)"
chmod 644 "$DEB"
chmod 755 "$TMP"
sudo apt-get install -y "$DEB"

say "Done. Open Ambassador Local from your app menu, or run: ambassador-local"
