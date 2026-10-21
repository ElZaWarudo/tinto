#!/usr/bin/env bash
# Runs Tinto's Rust checks on Linux from a Windows checkout, through WSL.
#
# Building straight from /mnt/c is slow (every file read crosses into
# Windows), so this keeps a copy of the working tree on WSL's own disk,
# syncs only what changed into it, and builds there incrementally.
#
# From PowerShell on Windows (pass a Windows path through /mnt/c):
#   wsl -d Ubuntu-24.04 -- bash -l /mnt/c/<checkout>/scripts/check-linux.sh [filter]
#
#   filter       run only the tests whose path contains it, e.g. `delivery::`
#   --no-clippy  skip clippy
#   --no-test    skip the tests
#
# Uncommitted changes are included: it copies the working tree, not a commit.
# The copy lives in ~/tinto-linux and builds into ~/tinto-target (override
# with TINTO_LINUX_COPY and CARGO_TARGET_DIR).
set -euo pipefail

source "$HOME/.cargo/env" 2>/dev/null || true

src="$(cd "$(dirname "$0")/.." && pwd)"
copy="${TINTO_LINUX_COPY:-$HOME/tinto-linux}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/tinto-target}"

clippy=1
test=1
filter=""
for arg in "$@"; do
  case "$arg" in
    --no-clippy) clippy=0 ;;
    --no-test) test=0 ;;
    *) filter="$arg" ;;
  esac
done

command -v rsync >/dev/null || {
  echo "check-linux: rsync is missing in WSL (sudo apt install rsync)" >&2
  exit 1
}

# Only the Rust crate and what it reads at build time: the built frontend,
# which the app embeds (an empty one is enough to compile). Checksums,
# because files on /mnt/c report coarse timestamps.
mkdir -p "$copy/dist" "$copy/.pumarejo"
rsync -a --delete --checksum --exclude '/target*/' "$src/src-tauri/" "$copy/src-tauri/"
# The optional E2E plugin is a path dependency that Cargo resolves even when
# it is off.
if [ -d "$src/.pumarejo/provider" ]; then
  rsync -a --delete --checksum --exclude target/ "$src/.pumarejo/provider/" "$copy/.pumarejo/provider/"
fi
if [ -d "$src/dist" ]; then
  rsync -a --delete --checksum "$src/dist/" "$copy/dist/"
fi

cd "$copy/src-tauri"
status=0
if [ "$clippy" = 1 ]; then
  echo "== clippy"
  cargo clippy --lib --tests -- -D warnings || status=1
fi
if [ "$test" = 1 ]; then
  echo "== tests${filter:+ ($filter)}"
  cargo test --lib -- $filter || status=1
fi
exit $status
