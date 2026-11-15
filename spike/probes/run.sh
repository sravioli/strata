#!/usr/bin/env bash
# Translate each probe function alone and report which tool accepts it.
# Usage: spike/probes/run.sh   (CHARON and AENEAS as in spike/README.md)
set -u
here="$(cd "$(dirname "$0")" && pwd)"
spike="$(dirname "$here")"
CHARON="${CHARON:-$HOME/source/aeneas/charon/bin/charon}"
AENEAS="${AENEAS:-$spike/.toolchain/aeneas-build/default/main.exe}"
out="$spike/.toolchain/probes-out"
rm -rf "$out"; mkdir -p "$out"

fns="next_number_plain next_number_checked is_newer number_dyn apply_closure
max_number_loop max_number_iter count_ready is_blank make_title next_number_result"

printf '%-22s %-8s %-8s %-8s %s\n' function charon aeneas lean axioms
cd "$here" || exit 1
for f in $fns; do
  d="$out/$f"; mkdir -p "$d"
  c=FAIL; a=-; l=-; ax=-
  if "$CHARON" cargo --preset=aeneas --start-from "crate::$f" \
       --dest-file "$d/p.llbc" >"$d/charon.log" 2>&1 && [ -f "$d/p.llbc" ]; then
    c=ok
    if grep -qi 'error' "$d/charon.log"; then c=ok+err; fi
    a=FAIL
    if "$AENEAS" -backend lean -no-progress-bar -dest "$d" "$d/p.llbc" \
         >"$d/aeneas.log" 2>&1; then
      a=ok
      if grep -q '^\[Error\]' "$d/aeneas.log"; then a=ok+err; fi
    fi
    gen="$(ls "$d"/*.lean 2>/dev/null | head -1)"
    if [ -n "$gen" ]; then
      ax="$(grep -c '^axiom\|^opaque' "$gen")"
      l=FAIL
      if (cd "$spike/lean" && lake env lean "$gen" >"$d/lean.log" 2>&1); then
        l=ok
        if grep -q 'sorry' "$gen"; then l=ok+sorry; fi
      fi
    fi
  fi
  printf '%-22s %-8s %-8s %-8s %s\n' "$f" "$c" "$a" "$l" "$ax"
done
