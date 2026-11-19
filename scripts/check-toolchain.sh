#!/usr/bin/env bash
# The crates build on stable Rust (tech design 0009): no tracked rust-toolchain file outside the test fixtures.
# Usage: scripts/check-toolchain.sh [ROOT]
set -euo pipefail
root=${1:-$(cd "$(dirname "$0")/.." && pwd)}
files=$(git -C "$root" ls-files | grep -E '(^|/)rust-toolchain(\.toml)?$' | grep -v '^scripts/tests/fixtures/' || true)
if [ -n "$files" ]; then
  echo "fail: a rust-toolchain file pins a toolchain. The crates build on stable; only scripts/translate.sh uses the nightly of Charon:" >&2
  echo "$files" >&2
  exit 1
fi
echo "ok: no rust-toolchain file"
