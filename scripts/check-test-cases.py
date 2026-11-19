#!/usr/bin/env python3
"""Check that each test case ID has an automated test or a PENDING marker.

Rules: the `test-case-check` OpenSpec spec, and `docs/agents/testing.md`.

Usage: check-test-cases.py [--report] [ROOT]   (ROOT defaults to the current directory, a git work tree)
  default   print each failure. Exit code 1 when a failure exists, 0 when none exists.
  --report  print the state of each ID, its design, its spec and its tests. Exit code 0.
"""

import argparse
import pathlib
import re
import subprocess
import sys

ID = re.compile(r"\bTC-\d+-\d+\b")
DESIGN_ROW = re.compile(r"^\|\s*(TC-\d+-\d+)\s*\|")
SCENARIO = re.compile(r"^####\s+Scenario:")
PENDING = re.compile(r"^\s*-\s+\*\*PENDING\*\*")
SELF_TEST = "scripts/tests/test_check_test_cases.py"  # its example IDs are not real test cases


def tracked_files(root):
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], check=True, capture_output=True, text=True)
    return [p for p in out.stdout.split("\0") if p]


def is_test_file(path):
    parts = path.split("/")
    if path.endswith(".md") or parts[0] in ("docs", "openspec") or path == SELF_TEST:
        return False
    return "fixtures" not in parts[:-1]


def is_spec(path):
    if not path.endswith("/spec.md"):
        return False
    if path.startswith("openspec/specs/"):
        return True
    return path.startswith("openspec/changes/") and not path.startswith("openspec/changes/archive/")


def is_design(path):
    name = path.removeprefix("docs/tech-design/")
    return name != path and "/" not in name and name.endswith(".md") and not name.startswith("0000-")


def design_rows(text):
    """Yield (id, automated-test cell) for each row of a test case table."""
    for line in text.splitlines():
        m = DESIGN_ROW.match(line)
        if m:
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            yield m.group(1), cells[-1]


def spec_scenarios(text):
    """Yield (id, pending) for each test case ID in a scenario name."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if not SCENARIO.match(line):
            continue
        pending = False
        for body in lines[i + 1:]:
            if body.startswith("#"):
                break
            pending = pending or bool(PENDING.match(body))
        for tc in ID.findall(line):
            yield tc, pending


def collect(root):
    """Return (designs, specs, carried): the design rows, spec scenarios and test files of each ID."""
    root = pathlib.Path(root)
    carried = {}  # id -> test files that carry it
    designs = {}  # id -> list of (file, cell)
    specs = {}  # id -> list of (file, pending)
    for path in tracked_files(root):
        if not (is_test_file(path) or is_spec(path) or is_design(path)):
            continue
        try:
            text = (root / path).read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):  # binary file, symlink to a folder, or deleted file
            continue
        if is_design(path):
            for tc, cell in design_rows(text):
                designs.setdefault(tc, []).append((path, cell))
        elif is_spec(path):
            for tc, pending in spec_scenarios(text):
                specs.setdefault(tc, []).append((path, pending))
        else:
            for tc in ID.findall(text):
                carried.setdefault(tc, []).append(path)
    return designs, specs, carried


def check(root):
    designs, specs, carried = collect(root)
    failures = []
    for tc, places in sorted(specs.items()):
        if tc in carried:
            continue
        for path, pending in places:
            if not pending:
                failures.append(f"{tc}: {path}: the scenario has no test that carries the ID and no PENDING marker")
    for tc, places in sorted(designs.items()):
        if tc in specs or tc in carried:
            continue
        for path, cell in places:
            if cell != "pending":
                failures.append(f"{tc}: {path}: the row is not pending and no test carries the ID")
    for tc, paths in sorted(carried.items()):
        if tc not in specs and tc not in designs:
            failures += [f"{tc}: {p}: the test carries an ID that no tech design and no spec defines" for p in paths]
    return failures


def report(root):
    """Yield one line for each ID: the ID, its state and the files that define or carry it."""
    designs, specs, carried = collect(root)
    failing = {line.split(":", 1)[0] for line in check(root)}
    for tc in sorted(designs.keys() | specs.keys() | carried.keys(), key=lambda t: tuple(map(int, t.split("-")[1:]))):
        if tc in failing:
            state = "failing"
        elif tc in carried:
            state = "implemented"
        elif tc in specs:
            state = "planned"
        else:
            state = "approved"
        files = [("design", [p for p, _ in designs.get(tc, [])]), ("spec", [p for p, _ in specs.get(tc, [])]),
                 ("tests", carried.get(tc, []))]
        yield " ".join([tc, state] + [f"{name}={','.join(paths) or '-'}" for name, paths in files])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", action="store_true")
    parser.add_argument("root", nargs="?", default=".")
    args = parser.parse_args()
    if args.report:
        for line in report(args.root):
            print(line)
        return 0
    failures = check(args.root)
    for line in failures:
        print(line)
    if failures:
        print(f"{len(failures)} test case failure(s). See docs/agents/testing.md.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
