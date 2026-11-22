"""Scenario "No global change to the restriction" (spec dev-services, requirement "CI runs the sandbox tests")."""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
# A write of 0 to the sysctl: `sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`, or `echo 0 > /proc/sys/...`.
SETS_ZERO = re.compile(r"apparmor_restrict_unprivileged_userns\s*=\s*0|echo\s+0\b.*apparmor_restrict_unprivileged_userns")


class NoGlobalUserns(unittest.TestCase):
    def test_no_step_sets_the_sysctl_to_zero(self):
        files = [ROOT / ".github/workflows/ci.yml", ROOT / "Makefile", *sorted((ROOT / "scripts").glob("*.sh"))]
        for path in files:
            for number, line in enumerate(path.read_text().splitlines(), 1):
                self.assertIsNone(SETS_ZERO.search(line), f"{path.relative_to(ROOT)}:{number}: {line}")

    def test_the_pattern_finds_a_write_of_zero(self):
        self.assertIsNotNone(SETS_ZERO.search("sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0"))
        self.assertIsNotNone(SETS_ZERO.search("echo 0 | sudo tee /proc/sys/kernel/apparmor_restrict_unprivileged_userns"))
        self.assertIsNone(SETS_ZERO.search("cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns"))


if __name__ == "__main__":
    unittest.main()
