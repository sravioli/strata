"""Tests that the crates build on stable Rust (spec proof-verification, requirement "Crates build on stable Rust")."""

import pathlib
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check-toolchain.sh"


class Toolchain(unittest.TestCase):
    def setUp(self):
        self.root = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)

    @unittest.skipUnless(shutil.which("cargo") and shutil.which("rustup"), "needs cargo and rustup")
    def test_tc_9_05_nightly_feature_fails_on_stable(self):
        """TC-9-05: a Rust crate that uses a nightly feature fails to build on stable."""
        (self.root / "src").mkdir()
        (self.root / "Cargo.toml").write_text('[package]\nname = "nightly-probe"\nversion = "0.0.0"\nedition = "2024"\n\n[workspace]\n')
        (self.root / "src/lib.rs").write_text("#![feature(never_type)]\npub fn f() -> Option<!> { None }\n")
        result = subprocess.run(["cargo", "+stable", "build", "--quiet", "--manifest-path", str(self.root / "Cargo.toml")],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("E0554", result.stderr)

    def test_toolchain_file_fails(self):
        (self.root / "crates/core").mkdir(parents=True)
        (self.root / "crates/core/rust-toolchain").write_text("nightly\n")
        (self.root / "scripts/tests/fixtures/x").mkdir(parents=True)
        (self.root / "scripts/tests/fixtures/x/rust-toolchain.toml").write_text("")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "-A"], check=True)
        result = subprocess.run([str(SCRIPT), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("crates/core/rust-toolchain", result.stderr)
        self.assertNotIn("fixtures", result.stderr)


if __name__ == "__main__":
    unittest.main()
