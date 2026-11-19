#!/usr/bin/env bash
# Translates the core crate to Lean: Rust -> Charon -> LLBC -> Aeneas -> lean/Strata/Generated/.
# The starting points are the functions of the enforced "Proof obligations" tables
# (scripts/proof-obligations.py), so derived impls stay out of the translation.
# Usage: scripts/translate.sh [--check]
#   --check  translate into a temporary folder and fail if lean/Strata/Generated/ differs (check 1 of 0009)
# CHARON and AENEAS select the binaries (default: on PATH). CORE selects the crate (default: crates/core).
set -euo pipefail
case "${1:-}" in
  "" | --check) ;;
  *) echo "usage: scripts/translate.sh [--check]" >&2; exit 2 ;;
esac
repo="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=/dev/null
. "$repo/toolchain.env"
CHARON=${CHARON:-charon}
AENEAS=${AENEAS:-aeneas}
core=${CORE:-$repo/crates/core}
gen="$repo/lean/Strata/Generated"

start=()
while read -r path; do start+=(--start-from "$path"); done < <(python3 "$repo/scripts/proof-obligations.py" --root "$repo" --start-from)
if [ ${#start[@]} -eq 0 ]; then
  echo "no enforced function: nothing to translate" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
(cd "$core" && RUSTUP_TOOLCHAIN=$CHARON_RUST "$CHARON" cargo --preset=aeneas --dest-file "$tmp/strata_core.llbc" "${start[@]}" -- --target-dir "$tmp/target")
mkdir -p "$tmp/out"
"$AENEAS" -backend lean -no-progress-bar -dest "$tmp/out" "$tmp/strata_core.llbc" >/dev/null

if [ "${1:-}" = "--check" ]; then
  if ! diff -r "$tmp/out" "$gen" >"$tmp/diff"; then
    echo "fail: lean/Strata/Generated/ is not the translation of the current Rust code. Run scripts/translate.sh." >&2
    grep -E '^(diff|Only in)' "$tmp/diff" | sed 's/^/  /' >&2 || true
    exit 1
  fi
  echo "ok: lean/Strata/Generated/ agrees with a fresh translation"
else
  rm -rf "$gen" && mkdir -p "$gen" && cp -r "$tmp/out/." "$gen/"
  echo "translated: ${start[*]}"
fi
