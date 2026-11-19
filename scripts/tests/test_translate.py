"""Test of scripts/translate.sh --check (spec proof-verification, requirement "Fresh translation").

Needs Charon and Aeneas: set CHARON and AENEAS, or put them on PATH (scripts/install-aeneas.sh).
"""

import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/translate.sh"
TOOLS = all(shutil.which(os.environ.get(t.upper(), t)) for t in ("charon", "aeneas"))


@unittest.skipUnless(TOOLS or os.environ.get("REQUIRE_PROOF_TOOLS"), "needs charon and aeneas (scripts/install-aeneas.sh)")
class Translate(unittest.TestCase):
    def run_check(self, change=None):
        tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copy(REPO / name, tmp / name)
        shutil.copytree(REPO / "crates", tmp / "crates")
        if change:
            path, old, new = change
            text = (tmp / path).read_text()
            self.assertIn(old, text)
            (tmp / path).write_text(text.replace(old, new))
        env = dict(os.environ, CORE=str(tmp / "crates/core"))
        return subprocess.run([str(SCRIPT), "--check"], env=env, capture_output=True, text=True)

    def test_current_translation_passes(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tc_9_01_stale_translation_fails(self):
        """TC-9-01: a change to a proved Rust function and no new translation makes the check fail."""
        result = self.run_check(("crates/core/src/job.rs", "1 => 30,", "1 => 31,"))
        self.assertEqual(result.returncode, 1)
        self.assertIn("StrataCore.lean", result.stderr)


if __name__ == "__main__":
    unittest.main()
