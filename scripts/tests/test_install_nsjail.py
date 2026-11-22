"""Tests of scripts/install-nsjail.sh (spec dev-services, requirement "The setup builds the pinned nsjail")."""

import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "install-nsjail.sh"
GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"]


class InstallNsjail(unittest.TestCase):
    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp())
        self.repo = self.dir / "repo"
        self.repo.mkdir()
        # A fake source: its Makefile writes a dummy nsjail, so no real build runs.
        (self.repo / "Makefile").write_text("nsjail:\n\tprintf '#!/bin/sh\\necho fake\\n' > nsjail\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("commit", "-q", "-m", "first")
        self.first = self.git("rev-parse", "HEAD")
        self.git("commit", "-q", "--allow-empty", "-m", "second")
        self.second = self.git("rev-parse", "HEAD")
        self.git("tag", "3.6")
        self.dest = self.dir / "dest"

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, *args):
        result = subprocess.run([*GIT, "-C", str(self.repo), *args], check=True, capture_output=True, text=True)
        return result.stdout.strip()

    def install(self, commit):
        pins = self.dir / "tools.env"
        pins.write_text(f"NSJAIL_TAG=3.6\nNSJAIL_COMMIT={commit}\n")
        env = {"PATH": os.environ["PATH"], "HOME": str(self.dir), "TOOLS_ENV": str(pins), "NSJAIL_REPO": self.repo.as_uri()}
        return subprocess.run([str(SCRIPT), str(self.dest)], env=env, capture_output=True, text=True)

    def test_a_source_that_is_not_the_pin(self):
        """Scenario "A source that is not the pin": the tag is at a different commit, so no binary."""
        result = self.install(self.first)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("not the pinned", result.stderr)
        self.assertFalse(self.dest.exists())

    def test_the_pinned_build(self):
        """Scenario "The pinned build": the tag is at the pin, so the script installs the binary."""
        result = self.install(self.second)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(subprocess.run([str(self.dest / "nsjail")], capture_output=True, text=True).stdout, "fake\n")


if __name__ == "__main__":
    unittest.main()
