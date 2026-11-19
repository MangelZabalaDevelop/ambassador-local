#!/bin/bash
# dev-build-remote.sh <user@host> [ssh options]: sends this checkout, uncommitted changes and new files included, to a
# Linux machine (a GB10, say) and runs scripts/dev-build.sh there. Works from Git Bash on Windows: the files go with
# LF endings. Example: scripts/dev-build-remote.sh me@gb10 -i ~/.ssh/gb10_key
set -euo pipefail
cd "$(dirname "$0")/.."
host=${1:?usage: scripts/dev-build-remote.sh user@host [ssh options]}
shift
# The ~ is expanded by the remote shell, on the other machine.
# shellcheck disable=SC2088
dest='~/.cache/ambassador-local-dev/src'

# Snapshot the working tree through a throwaway index (git add -A minus .gitignore), so the real index is untouched.
idx=$(mktemp)
trap 'rm -f "$idx"' EXIT
cp "$(git rev-parse --git-path index)" "$idx"
GIT_INDEX_FILE="$idx" git add -A
tree=$(GIT_INDEX_FILE="$idx" git write-tree)

git -c core.autocrlf=false archive --format=tar "$tree" |
  ssh "$@" "$host" "rm -rf $dest && mkdir -p $dest && tar -xf - -C $dest && bash $dest/scripts/dev-build.sh"
