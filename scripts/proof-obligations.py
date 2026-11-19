#!/usr/bin/env python3
"""Read the "Proof obligations" tables of the accepted tech designs (tech design 0009, requirement V6).

A table is enforced when a Rust function that it names exists in the core crate
(a line `fn <name>` in crates/core/src). Only the rows of enforced tables count.

Usage: proof-obligations.py [--root DIR] [--start-from]
  default       JSON: [{"design", "theorem", "functions"}] for each row of an enforced table
  --start-from  one Charon path (crate::<module>::<function>) per line, for scripts/translate.sh
"""

import argparse
import json
import pathlib
import re
import sys

FUNCTION = re.compile(r"`([a-z_][a-z0-9_]*)`")


def table_rows(text):
    """Yield (theorem, [functions]) for each row of the "Proof obligations" table."""
    section = re.search(r"^## Proof obligations\n(.*?)(?=^## |\Z)", text, re.S | re.M)
    if not section:
        return
    rows = [line for line in section.group(1).splitlines() if line.startswith("|")]
    for line in rows[2:]:  # skip the header and the separator
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        theorem = cells[0].strip("`")
        yield theorem, FUNCTION.findall(cells[-1])


def core_functions(root):
    """Map each function name in the core crate to its Charon path."""
    found = {}
    src = root / "crates/core/src"
    for path in sorted(src.rglob("*.rs")):
        module = path.relative_to(src).with_suffix("")
        parts = [] if module.name in ("lib", "mod") and module.parent == pathlib.Path(".") else list(module.parts)
        if parts and parts[-1] == "mod":
            parts = parts[:-1]
        for name in re.findall(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+([a-z_][a-z0-9_]*)\b", path.read_text(), re.M):
            found.setdefault(name, "::".join(["crate", *parts, name]))
    return found


def obligations(root):
    root = pathlib.Path(root)
    functions = core_functions(root)
    rows = []
    for design in sorted((root / "docs/tech-design").glob("*.md")):
        text = design.read_text(encoding="utf-8")
        if design.name.startswith("0000-") or not re.search(r"^- Status: accepted\b", text, re.M):
            continue
        table = list(table_rows(text))
        if not any(f in functions for _, fns in table for f in fns):
            continue
        rel = str(design.relative_to(root))
        rows += [{"design": rel, "theorem": t, "functions": fns} for t, fns in table]
    return rows, functions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--start-from", action="store_true")
    args = parser.parse_args()
    rows, functions = obligations(args.root)
    if args.start_from:
        names = sorted({f for row in rows for f in row["functions"] if f in functions})
        print("\n".join(functions[n] for n in names))
    else:
        json.dump(rows, sys.stdout, indent=2)
        print()


if __name__ == "__main__":
    main()
