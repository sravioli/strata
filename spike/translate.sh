#!/usr/bin/env bash
# Regenerate the Lean files from the Rust crates: Rust -> (Charon) -> LLBC -> (Aeneas) -> Lean.
# Usage: spike/translate.sh
# CHARON and AENEAS can be set in the environment; the defaults are the paths in spike/README.md.
set -euo pipefail
spike="$(cd "$(dirname "$0")" && pwd)"
CHARON="${CHARON:-$HOME/source/aeneas/charon/bin/charon}"
AENEAS="${AENEAS:-$spike/.toolchain/aeneas-build/default/main.exe}"
gen="$spike/lean/StrataSpike/Generated"
mkdir -p "$gen"

# The lifecycle slice. --start-from keeps the derived Debug/PartialEq impls out
# of the translation (they pull in standard-library functions as Lean axioms).
cd "$spike/domain-core"
"$CHARON" cargo --preset=aeneas \
  --start-from crate::step --start-from crate::searchable --start-from crate::new_revision
"$AENEAS" -backend lean -no-progress-bar -dest "$gen" domain_core.llbc

# The integer overflow probe.
cd "$spike/probes"
"$CHARON" cargo --preset=aeneas \
  --start-from crate::next_number_plain --start-from crate::next_number_checked
"$AENEAS" -backend lean -no-progress-bar -dest "$gen" probes.llbc
