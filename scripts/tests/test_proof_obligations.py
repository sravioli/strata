"""Tests of scripts/proof-obligations.py (spec proof-verification, requirement "Each named theorem exists")."""

import importlib.util
import pathlib
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "proof-obligations.py"
spec = importlib.util.spec_from_file_location("proof_obligations", SCRIPT)
po = importlib.util.module_from_spec(spec)
spec.loader.exec_module(po)

TABLE = """
## Proof obligations

| Theorem | Requirement | Plain English | Lean statement | Rust function it is about |
|---|---|---|---|---|
| `step_total` | R1 | text | `x` | `step` |
| `scope_ok` | R2 | text | `y` | `scope`, `step` |

## Impacts
"""


def design(status, body=TABLE):
    return f"- Issue: #1\n- Status: {status}\n\n# A design\n{body}"


class ProofObligations(unittest.TestCase):
    def rows(self, files):
        with tempfile.TemporaryDirectory() as root:
            for path, text in files.items():
                p = pathlib.Path(root, path)
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(text)
            rows, _ = po.obligations(root)
            return rows

    def test_enforced_table_gives_all_rows(self):
        rows = self.rows({"docs/tech-design/0001-a.md": design("accepted"), "crates/core/src/lib.rs": "pub fn step() {}\n"})
        self.assertEqual([(r["theorem"], r["functions"]) for r in rows], [("step_total", ["step"]), ("scope_ok", ["scope", "step"])])
        self.assertEqual(rows[0]["design"], "docs/tech-design/0001-a.md")

    def test_table_with_no_function_in_the_core_crate_is_not_enforced(self):
        rows = self.rows({"docs/tech-design/0001-a.md": design("accepted"), "crates/core/src/lib.rs": "pub fn other() {}\n"})
        self.assertEqual(rows, [])

    def test_design_that_is_not_accepted_does_not_count(self):
        rows = self.rows({"docs/tech-design/0001-a.md": design("draft"), "crates/core/src/lib.rs": "pub fn step() {}\n"})
        self.assertEqual(rows, [])

    def test_section_na_gives_no_rows(self):
        files = {"docs/tech-design/0001-a.md": design("accepted", "\n## Proof obligations\n\nN/A. No theorems.\n"),
                 "crates/core/src/lib.rs": "pub fn step() {}\n"}
        self.assertEqual(self.rows(files), [])

    def test_const_fn_counts(self):
        rows = self.rows({"docs/tech-design/0001-a.md": design("accepted"), "crates/core/src/lib.rs": "pub const fn step() {}\n"})
        self.assertEqual(len(rows), 2)

    def test_charon_path_uses_the_module(self):
        with tempfile.TemporaryDirectory() as root:
            p = pathlib.Path(root, "crates/core/src/job.rs")
            p.parent.mkdir(parents=True)
            p.write_text("pub fn job_step() {}\n")
            pathlib.Path(root, "crates/core/src/lib.rs").write_text("pub mod job;\n")
            self.assertEqual(po.core_functions(pathlib.Path(root))["job_step"], "crate::job::job_step")


if __name__ == "__main__":
    unittest.main()
