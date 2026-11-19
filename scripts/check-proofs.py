#!/usr/bin/env python3
"""Checks 2 to 5 of tech design 0009 on the Lean project.

For each row of an enforced "Proof obligations" table (scripts/proof-obligations.py):
  - the theorem Strata.<name> exists (check 5),
  - it depends only on propext, Classical.choice and Quot.sound (checks 2 and 3; sorryAx is an unfinished proof),
and for each Rust function that a table names, Strata.<function>_refines exists with the same axioms (check 4).
It also fails on the words sorry and admit in lean/Strata/ (check 2).

Usage: check-proofs.py [--root DIR]   (the Lean project is DIR/lean; it is built first)
Exit code 1 when a check fails, 0 when all pass.
"""

import argparse
import importlib.util
import pathlib
import re
import subprocess
import sys
import tempfile

STANDARD = {"propext", "Classical.choice", "Quot.sound"}
UNFINISHED = re.compile(r"\b(sorry|admit)\b")

_spec = importlib.util.spec_from_file_location("proof_obligations", pathlib.Path(__file__).with_name("proof-obligations.py"))
proof_obligations = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(proof_obligations)


def required(root):
    """Return [(lean name, what it is for)] for each theorem that must exist."""
    rows, _ = proof_obligations.obligations(root)
    names = [(f"Strata.{r['theorem']}", f"theorem `{r['theorem']}` of {r['design']}") for r in rows]
    fns = sorted({f for r in rows for f in r["functions"]})
    names += [(f"Strata.{f}_refines", f"refinement theorem of the Rust function `{f}`") for f in fns]
    return names


def axioms_of(lean_dir, names):
    """Run `#print axioms` for each name. Return {name: set of axioms, or None if it does not exist}."""
    with tempfile.NamedTemporaryFile("w", suffix=".lean", dir=lean_dir, delete=False) as f:
        f.write("import Strata\n" + "".join(f"#print axioms {n}\n" for n, _ in names))
        audit = pathlib.Path(f.name)
    try:
        out = subprocess.run(["lake", "env", "lean", audit.name], cwd=lean_dir, capture_output=True, text=True)
    finally:
        audit.unlink()
    text = out.stdout + out.stderr
    result = {n: None for n, _ in names}
    for m in re.finditer(r"'([^']+)' depends on axioms: \[(.*?)\]", text, re.S):
        result[m.group(1)] = {a.strip() for a in m.group(2).split(",") if a.strip()}
    for m in re.finditer(r"'([^']+)' does not depend on any axioms", text):
        result[m.group(1)] = set()
    return result


def check(root):
    root = pathlib.Path(root)
    lean_dir = root / "lean"
    failures = []
    for path in sorted((lean_dir / "Strata").rglob("*.lean")):
        for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if UNFINISHED.search(line.split("--", 1)[0]):
                failures.append(f"{path.relative_to(root)}:{i}: unfinished proof (`sorry` or `admit`)")
    build = subprocess.run(["lake", "build", "Strata"], cwd=lean_dir, capture_output=True, text=True)
    if build.returncode != 0:
        return failures + ["lake build failed:\n" + build.stdout[-4000:] + build.stderr[-2000:]]
    names = required(root)
    found = axioms_of(lean_dir, names)
    for name, what in names:
        axioms = found[name]
        if axioms is None:
            failures.append(f"{name}: missing, the {what}")
        elif "sorryAx" in axioms:
            failures.append(f"{name}: unfinished proof (depends on sorryAx), the {what}")
        else:
            for axiom in sorted(axioms - STANDARD):
                failures.append(f"{name}: depends on the axiom {axiom}, the {what}")
    return failures


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    failures = check(parser.parse_args().root)
    for line in failures:
        print(line)
    if failures:
        print(f"{len(failures)} proof failure(s). See tech design 0009, 'The definition of proved'.")
        return 1
    print("ok: each enforced theorem exists and depends only on the standard axioms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
