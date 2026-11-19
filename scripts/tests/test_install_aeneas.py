"""Tests of scripts/install-aeneas.sh (spec proof-verification, requirement "Pinned package of Aeneas")."""

import hashlib
import io
import pathlib
import subprocess
import tarfile
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "install-aeneas.sh"
NAMES = ["aeneas", "charon", "charon-driver", "rust-toolchain"]


class InstallAeneas(unittest.TestCase):
    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp())
        self.pkg = self.dir / "pkg.tar.gz"
        with tarfile.open(self.pkg, "w:gz") as tar:
            for name in NAMES:
                info = tarfile.TarInfo(name)
                data = f"fake {name}\n".encode()
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        self.sha = hashlib.sha256(self.pkg.read_bytes()).hexdigest()
        self.dest = self.dir / "dest"

    def tearDown(self):
        subprocess.run(["rm", "-rf", str(self.dir)], check=True)

    def install(self, sha, urls):
        env_file = self.dir / "toolchain.env"
        env_file.write_text(f"AENEAS_RELEASE=test\nAENEAS_PACKAGE=pkg.tar.gz\nAENEAS_SHA256={sha}\n")
        env = {"PATH": "/usr/bin:/bin", "AENEAS_ENV": str(env_file), "AENEAS_URLS": " ".join(urls)}
        return subprocess.run([str(SCRIPT), str(self.dest)], env=env, capture_output=True, text=True)

    def test_tc_9_06_wrong_sha_stops(self):
        """TC-9-06: a package with a SHA-256 that is not the pin stops the job before it uses the package."""
        bad = self.dir / "bad.tar.gz"
        bad.write_bytes(self.pkg.read_bytes() + b"x")
        # The second source is good: a script that goes on after a bad SHA-256 would install it.
        result = self.install(self.sha, [bad.as_uri(), self.pkg.as_uri()])
        self.assertEqual(result.returncode, 1)
        self.assertIn("SHA-256", result.stderr)
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_first_source_not_available_uses_second(self):
        result = self.install(self.sha, [(self.dir / "missing.tar.gz").as_uri(), self.pkg.as_uri()])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("not available", result.stderr)
        self.assertEqual(sorted(p.name for p in self.dest.iterdir()), sorted(NAMES))


if __name__ == "__main__":
    unittest.main()
