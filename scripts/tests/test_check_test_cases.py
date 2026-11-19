"""Unit tests for scripts/check-test-cases.py: one test for each scenario of the test-case-check spec."""

import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check-test-cases.py"
spec = importlib.util.spec_from_file_location("check_test_cases", SCRIPT)
ctc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ctc)

DESIGN = "docs/tech-design/0009-x.md"
SPEC = "openspec/specs/cap/spec.md"
TEST = "crates/core/tests/t.rs"


def design(tc, cell):
    return f"| ID | Level | Text | Decided by | Automated test |\n|---|---|---|---|---|\n| {tc} | L0 | text | owner | {cell} |\n"


def scenario(tc, pending=False):
    marker = "- **PENDING** not written yet\n" if pending else ""
    return f"### Requirement: R\n\nText.\n\n#### Scenario: {tc} name\n\n- **WHEN** x\n- **THEN** y\n{marker}"


class CheckTestCases(unittest.TestCase):
    def run_check(self, files):
        with tempfile.TemporaryDirectory() as root:
            for path, text in files.items():
                p = pathlib.Path(root, path)
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(text)
            subprocess.run(["git", "init", "-q", root], check=True)
            subprocess.run(["git", "-C", root, "add", "-A"], check=True)
            return ctc.check(root)

    def test_scenario_with_test_passes(self):
        self.assertEqual(self.run_check({SPEC: scenario("TC-9-02"), TEST: "// TC-9-02\n"}), [])

    def test_scenario_with_pending_marker_passes(self):
        self.assertEqual(self.run_check({SPEC: scenario("TC-9-02", pending=True)}), [])

    def test_scenario_with_no_test_and_no_marker_fails(self):
        failures = self.run_check({SPEC: scenario("TC-9-02")})
        self.assertEqual(len(failures), 1)
        self.assertIn("TC-9-02", failures[0])
        self.assertIn(SPEC, failures[0])

    def test_marker_of_next_scenario_does_not_count(self):
        text = scenario("TC-9-02") + "\n#### Scenario: other\n\n- **WHEN** x\n- **THEN** y\n- **PENDING** z\n"
        self.assertEqual(len(self.run_check({SPEC: text})), 1)

    def test_spec_overrides_pending_design_row(self):
        failures = self.run_check({DESIGN: design("TC-9-02", "pending"), SPEC: scenario("TC-9-02")})
        self.assertEqual(len(failures), 1)
        self.assertIn("TC-9-02", failures[0])

    def test_spec_ignores_design_column(self):
        files = {DESIGN: design("TC-9-02", "`old_test`"), SPEC: scenario("TC-9-02", pending=True)}
        self.assertEqual(self.run_check(files), [])

    def test_design_only_id_with_pending_passes(self):
        self.assertEqual(self.run_check({DESIGN: design("TC-5-01", "pending")}), [])

    def test_design_only_id_with_no_pending_and_no_test_fails(self):
        failures = self.run_check({DESIGN: design("TC-5-01", "`job_test`")})
        self.assertEqual(len(failures), 1)
        self.assertIn("TC-5-01", failures[0])
        self.assertIn(DESIGN, failures[0])

    def test_test_with_unknown_id_fails(self):
        failures = self.run_check({TEST: "// TC-99-01\n"})
        self.assertEqual(len(failures), 1)
        self.assertIn("TC-99-01", failures[0])
        self.assertIn(TEST, failures[0])

    def test_unknown_id_names_each_test_file(self):
        failures = self.run_check({TEST: "// TC-99-01\n", "lean/T.lean": "-- TC-99-01\n"})
        self.assertEqual(len(failures), 2)

    def test_excluded_paths_do_not_count(self):
        files = {
            "scripts/tests/test_check_test_cases.py": "TC-99-01",
            "lean/fixtures/Sorry.lean": "TC-99-02",
            "openspec/changes/archive/old/specs/cap/spec.md": scenario("TC-99-03"),
            "docs/tech-design/0000-template-rfc.md": design("TC-99-04", "`t`"),
            "README.md": "TC-99-05",
        }
        self.assertEqual(self.run_check(files), [])

    def test_other_script_test_counts(self):
        files = {SPEC: scenario("TC-9-02"), "scripts/tests/test_other.py": "# TC-9-02\n"}
        self.assertEqual(self.run_check(files), [])

    def test_active_change_spec_counts(self):
        failures = self.run_check({"openspec/changes/c/specs/cap/spec.md": scenario("TC-9-02")})
        self.assertEqual(len(failures), 1)


if __name__ == "__main__":
    unittest.main()
