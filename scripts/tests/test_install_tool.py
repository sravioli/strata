"""Tests of scripts/install-tool.sh: a pinned release binary installs only when its SHA-256 is the pin."""

import hashlib
import io
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "install-tool.sh"


class InstallTool(unittest.TestCase):
    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp())
        self.pkg = self.dir / "pkg.tar.gz"
        with tarfile.open(self.pkg, "w:gz") as tar:
            data = b"#!/bin/sh\necho fake\n"
            info = tarfile.TarInfo("release-1.0/my-tool")
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        self.sha = hashlib.sha256(self.pkg.read_bytes()).hexdigest()
        self.dest = self.dir / "bin"

    def tearDown(self):
        shutil.rmtree(self.dir)

    def run_script(self, sha, name="my-tool"):
        pins = self.dir / "tools.env"
        pins.write_text(f"MY_TOOL_URL=file://{self.pkg}\nMY_TOOL_SHA256={sha}\n")
        return subprocess.run(
            ["bash", str(SCRIPT), name, str(self.dest)],
            env={"PATH": "/usr/bin:/bin", "TOOLS_ENV": str(pins)},
            capture_output=True,
            text=True,
        )

    def test_pinned_hash_installs_the_binary(self):
        result = self.run_script(self.sha)
        self.assertEqual(result.returncode, 0, result.stderr)
        tool = self.dest / "my-tool"
        self.assertTrue(tool.is_file())
        self.assertTrue(tool.stat().st_mode & 0o111)

    def test_other_hash_stops_and_installs_nothing(self):
        result = self.run_script("0" * 64)
        self.assertEqual(result.returncode, 1)
        self.assertIn("is not the pinned", result.stderr)
        self.assertFalse((self.dest / "my-tool").exists())

    def test_tool_with_no_pin_stops(self):
        result = self.run_script(self.sha, name="other-tool")
        self.assertEqual(result.returncode, 2)
        self.assertIn("no pin for other-tool", result.stderr)


if __name__ == "__main__":
    unittest.main()
