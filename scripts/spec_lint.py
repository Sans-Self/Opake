#!/usr/bin/env python3
"""Check provenance citations in openspec/specs/ against the repository.

The specs cite three kinds of evidence: repository file paths, bug__
regression test names, and commit hashes. openspec's own validator only
checks document structure, so nothing else notices when a cited test is
renamed, a file moves, or a hash is fabricated. This lint turns those
citations from decoration into a checked contract.

Exit code is non-zero if any citation dangles.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PATH_RE = re.compile(
    r"\b((?:crates|apps|docs|lexicons|packages|scripts)/[\w./-]+"
    r"\.(?:rs|exs|ex|md|tsx|ts|json|py))"
)
TEST_RE = re.compile(r"bug__\w+")
HASH_RE = re.compile(r"`([0-9a-f]{7,40})`")


def commit_exists(rev: str) -> bool:
    return (
        subprocess.run(
            ["git", "cat-file", "-e", f"{rev}^{{commit}}"],
            cwd=ROOT,
            capture_output=True,
        ).returncode
        == 0
    )


def symbol_exists(name: str) -> bool:
    return (
        subprocess.run(
            ["git", "grep", "-q", "--fixed-strings", name, "--", "*.rs", "*.exs"],
            cwd=ROOT,
            capture_output=True,
        ).returncode
        == 0
    )


def main() -> int:
    specs = sorted((ROOT / "openspec" / "specs").rglob("spec.md"))
    if not specs:
        print("spec-lint: no specs found under openspec/specs/", file=sys.stderr)
        return 1

    errors: list[str] = []
    checked = {"paths": 0, "tests": 0, "hashes": 0}

    for spec in specs:
        rel = spec.relative_to(ROOT)
        text = spec.read_text()

        for path in sorted(set(PATH_RE.findall(text))):
            checked["paths"] += 1
            if not (ROOT / path).exists():
                errors.append(f"{rel}: cited path does not exist: {path}")

        for test in sorted(set(TEST_RE.findall(text))):
            checked["tests"] += 1
            if not symbol_exists(test):
                errors.append(f"{rel}: cited regression test not found: {test}")

        for rev in sorted(set(HASH_RE.findall(text))):
            checked["hashes"] += 1
            if not commit_exists(rev):
                errors.append(f"{rel}: cited commit not found: {rev}")

    for e in errors:
        print(e, file=sys.stderr)
    print(
        f"spec-lint: {len(specs)} specs, "
        f"{checked['paths']} paths, {checked['tests']} tests, "
        f"{checked['hashes']} hashes checked, {len(errors)} dangling"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
