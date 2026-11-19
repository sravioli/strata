"""Tests of scripts/check-proofs.py on small Lean projects that use core Lean only (no Mathlib).

Spec proof-verification: TC-9-02, TC-9-03, TC-9-04 and the refinement theorem rule.
"""

import importlib.util
import pathlib
import shutil
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check-proofs.py"
spec = importlib.util.spec_from_file_location("check_proofs", SCRIPT)
cp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cp)

REPO = pathlib.Path(__file__).resolve().parents[2]
DESIGN = """- Status: accepted

## Proof obligations

| Theorem | Requirement | Plain English | Lean statement | Rust function it is about |
|---|---|---|---|---|
| `f_ok` | R1 | text | `f = 1` | `f` |
"""
GOOD = "namespace Strata\ndef f : Nat := 1\ntheorem f_ok : f = 1 := rfl\ntheorem f_refines : f = 1 := rfl\nend Strata\n"


@unittest.skipUnless(shutil.which("lake"), "needs the Lean toolchain (lake)")
class CheckProofs(unittest.TestCase):
    def check(self, lean, design=DESIGN):
        root = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        (root / "docs/tech-design").mkdir(parents=True)
        (root / "docs/tech-design/0001-a.md").write_text(design)
        (root / "crates/core/src").mkdir(parents=True)
        (root / "crates/core/src/lib.rs").write_text("pub fn f() -> u32 { 1 }\n")
        (root / "lean/Strata").mkdir(parents=True)
        shutil.copy(REPO / "lean/lean-toolchain", root / "lean/lean-toolchain")
        (root / "lean/lakefile.toml").write_text('name = "strata"\n\n[[lean_lib]]\nname = "Strata"\n')
        (root / "lean/Strata.lean").write_text("import Strata.Main\n")
        (root / "lean/Strata/Main.lean").write_text(lean)
        return cp.check(root)

    def test_complete_project_passes(self):
        self.assertEqual(self.check(GOOD), [])

    def test_tc_9_02_sorry_fails(self):
        """TC-9-02: a theorem that contains sorry makes the verification job fail."""
        failures = self.check(GOOD.replace("theorem f_ok : f = 1 := rfl", "theorem f_ok : f = 1 := by sorry"))
        self.assertTrue(any("unfinished proof" in f and "Main.lean" in f for f in failures), failures)
        self.assertIn("Strata.f_ok: unfinished proof (depends on sorryAx), the theorem `f_ok` of docs/tech-design/0001-a.md", failures)

    def test_tc_9_03_aeneas_axiom_fails(self):
        """TC-9-03: a theorem that depends on an axiom that Aeneas generated fails, and the message names both."""
        lean = GOOD.replace("theorem f_ok : f = 1 := rfl",
                            "axiom core.num.U64.opaque_fn : f = 1\ntheorem f_ok : f = 1 := core.num.U64.opaque_fn")
        failures = self.check(lean)
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("Strata.f_ok", failures[0])
        self.assertIn("core.num.U64.opaque_fn", failures[0])

    def test_tc_9_04_missing_theorem_fails(self):
        """TC-9-04: a theorem that an accepted table names, and that does not exist, fails, and the message names it."""
        failures = self.check(GOOD.replace("theorem f_ok : f = 1 := rfl\n", ""))
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("Strata.f_ok: missing", failures[0])
        self.assertIn("0001-a.md", failures[0])

    def test_function_with_no_refinement_theorem_fails(self):
        failures = self.check(GOOD.replace("theorem f_refines : f = 1 := rfl\n", ""))
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("Strata.f_refines: missing", failures[0])
        self.assertIn("`f`", failures[0])


if __name__ == "__main__":
    unittest.main()
