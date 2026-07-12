#!/usr/bin/env python3
"""Check provenance citations in openspec/specs/ against the repository.

The specs cite three kinds of evidence: repository file paths, bug__
regression test names, and commit hashes. openspec's own validator only
checks document structure, so nothing else notices when a cited test is
renamed, a file moves, or a hash is fabricated. This lint turns those
citations from decoration into a checked contract.

The reverse direction is checked too: tests may cite the spec scenario
they exercise as `spec:<capability> § <requirement name>`. Each citation
must resolve to a `### Requirement:` heading in
openspec/specs/<capability>/spec.md, so renaming a requirement without
updating its tests (or citing a requirement that never existed) fails.
Test citations live in three places, all scanned the same way:

- TS/e2e tests under tests/ — literal `spec:` strings and `cite(...)` calls.
- Rust tests under crates/ — comment lines `// spec:<cap> § <req>` (also
  `//!`/`///`). All .rs is scanned, not only *_tests.rs; a citation
  outside test code is harmless and still resolves.
- Elixir tests under apps/indexer/test/ — comment lines `# spec:<cap> § <req>`.

Dangling test citations report file:line so the offending comment is easy
to find.

Pass --coverage to print the requirement-coverage ledger: per capability,
each requirement with the number of citing tests across all languages,
flagging any requirement no test cites. The table is informational — it
never changes the exit code.

Specs may cite each other with the same syntax. Cross-spec citations are
resolved in main specs and in non-archived change deltas; a delta's
citations resolve against the main spec set plus requirements ADDED in the
same change, so a change can reference what it introduces. Self-citations
are legal and resolve like any other.

Changes also get a blast-radius check: a requirement listed under
`## REMOVED Requirements` in a delta while another spec or test still
cites it is an error, unless the citing spec has its own delta in the same
change. A `## MODIFIED Requirements` entry with outside citers prints a
non-fatal `note:` line naming them — the impact readout for proposal
review. Semantic staleness (a sibling spec whose assumptions the change
invalidates without touching any cited name) is out of reach for a lint;
that half lives in the spec-crossref-review skill.

Exit code is non-zero if any citation dangles.
"""

from __future__ import annotations

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
# `spec:workspace-membership § Leave guards — no orphaned workspaces` inside
# a backtick span (specs) or a double-quoted/backtick string (tests); the
# requirement name runs to the closing delimiter. Apostrophes are legal in
# requirement names ("the document's rotation"), so single quotes do NOT
# terminate — cite inside double quotes or backticks, never single quotes.
CITE_RE = re.compile(r"spec:([a-z0-9-]+)\s*§\s*([^\"`\n]+)")
# Tests may cite via the `cite("capability", "Requirement name")` helper
# (tests/e2e/fixtures.ts), which builds the `spec:` tag at runtime — so the
# literal never appears in source. Recognize the call form directly.
# Double-quoted args only (apostrophes are legal inside requirement names, so
# single-quoted delimiters would truncate them — same rule as CITE_RE);
# tolerant of multi-line calls and a trailing comma.
CITE_CALL_RE = re.compile(
    r'cite\(\s*"([a-z0-9-]+)"\s*,\s*"([^"]+)"\s*,?\s*\)'
)
REQUIREMENT_RE = re.compile(r"^### Requirement: (.+)$", re.MULTILINE)
DELTA_SECTION_RE = re.compile(r"^## (ADDED|MODIFIED|REMOVED) Requirements\b")


def normalize(name: str) -> str:
    return " ".join(name.split())


def load_requirements(specs_root: Path) -> dict[str, set[str]]:
    return {
        spec.parent.name: {
            normalize(m) for m in REQUIREMENT_RE.findall(spec.read_text())
        }
        for spec in specs_root.rglob("spec.md")
    }


def load_delta_sections(delta: Path) -> dict[str, set[str]]:
    """Requirement names grouped by ADDED/MODIFIED/REMOVED delta section."""
    sections: dict[str, set[str]] = {}
    current = None
    for line in delta.read_text().splitlines():
        section = DELTA_SECTION_RE.match(line)
        if section:
            current = section.group(1)
            continue
        requirement = re.match(r"^### Requirement: (.+)$", line)
        if requirement and current:
            sections.setdefault(current, set()).add(normalize(requirement.group(1)))
    return sections


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
    specs_root = ROOT / "openspec" / "specs"
    specs = sorted(specs_root.rglob("spec.md"))
    if not specs:
        print("spec-lint: no specs found under openspec/specs/", file=sys.stderr)
        return 1

    errors: list[str] = []
    notes: list[str] = []
    checked = {"paths": 0, "tests": 0, "hashes": 0, "citations": 0}
    requirements = load_requirements(specs_root)
    # (capability, requirement) -> [(citing file, citing capability | None)]
    citers: dict[tuple[str, str], list[tuple[Path, str | None]]] = {}

    # Requirements ADDED by active (non-archived) changes resolve everywhere,
    # not just inside their own delta files: tests written while a change is
    # in flight cite the new requirements before the sync-to-canon at archive.
    # An abandoned change makes such cites dangle again on the next run.
    changes_root = ROOT / "openspec" / "changes"
    changes = (
        sorted(c for c in changes_root.iterdir() if c.is_dir() and c.name != "archive")
        if changes_root.exists()
        else []
    )
    change_sections = {
        change: {
            d.parent.name: load_delta_sections(d)
            for d in sorted((change / "specs").rglob("spec.md"))
        }
        for change in changes
    }
    in_flight = {
        (cap, req)
        for sections in change_sections.values()
        for cap, secs in sections.items()
        for req in secs.get("ADDED", set())
    }

    def check_citation(
        loc: str,
        capability: str,
        req: str,
        extra: set[tuple[str, str]] = frozenset(),
    ) -> None:
        checked["citations"] += 1
        req = normalize(req)
        if (capability, req) in extra or (capability, req) in in_flight:
            return
        if capability not in requirements:
            errors.append(f"{loc}: cited capability does not exist: {capability}")
        elif req not in requirements[capability]:
            errors.append(f"{loc}: no requirement '{req}' in spec {capability}")

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

        for capability, req in CITE_RE.findall(text):
            check_citation(str(rel), capability, req)
            citers.setdefault((capability, normalize(req)), []).append(
                (rel, spec.parent.name)
            )

    for test_file in sorted((ROOT / "tests").rglob("*.ts")):
        if "node_modules" in test_file.parts:
            continue
        rel = test_file.relative_to(ROOT)
        text = test_file.read_text()
        for capability, req in CITE_RE.findall(text) + CITE_CALL_RE.findall(text):
            check_citation(str(rel), capability, req)
            citers.setdefault((capability, normalize(req)), []).append((rel, None))

    # Rust and Elixir tests carry citations as comment lines. Scan line by
    # line so a dangling citation reports file:line, and scan all .rs under
    # crates/ (not just *_tests.rs) — a citation outside test code resolves
    # the same way and is harmless.
    source_files = sorted((ROOT / "crates").rglob("*.rs")) + sorted(
        (ROOT / "apps" / "indexer" / "test").rglob("*.exs")
    )
    for source_file in source_files:
        if "node_modules" in source_file.parts or "target" in source_file.parts:
            continue
        rel = source_file.relative_to(ROOT)
        for lineno, line in enumerate(source_file.read_text().splitlines(), start=1):
            for capability, req in CITE_RE.findall(line):
                check_citation(f"{rel}:{lineno}", capability, req)
                citers.setdefault((capability, normalize(req)), []).append((rel, None))

    for change in changes:
        deltas = {
            d.parent.name: d for d in sorted((change / "specs").rglob("spec.md"))
        }
        sections = change_sections[change]

        for cap, delta in deltas.items():
            rel = delta.relative_to(ROOT)
            for capability, req in CITE_RE.findall(delta.read_text()):
                check_citation(str(rel), capability, req)

        for cap, secs in sections.items():
            for req in sorted(secs.get("REMOVED", set())):
                for citing_file, citing_cap in citers.get((cap, req), []):
                    if citing_cap == cap or citing_cap in deltas:
                        continue
                    errors.append(
                        f"{change.relative_to(ROOT)}: removes '{cap} § {req}' "
                        f"still cited by {citing_file}"
                        + (
                            f" (no delta for {citing_cap} in this change)"
                            if citing_cap
                            else ""
                        )
                    )
            for req in sorted(secs.get("MODIFIED", set())):
                outside = [
                    str(f) for f, c in citers.get((cap, req), []) if c != cap
                ]
                if outside:
                    notes.append(
                        f"note: {change.name} modifies '{cap} § {req}' "
                        f"cited by: {', '.join(outside)}"
                    )

    for n in notes:
        print(n)
    for e in errors:
        print(e, file=sys.stderr)
    print(
        f"spec-lint: {len(specs)} specs, {len(changes)} changes, "
        f"{checked['paths']} paths, {checked['tests']} tests, "
        f"{checked['hashes']} hashes, "
        f"{checked['citations']} citations checked, {len(errors)} dangling"
    )

    if "--coverage" in sys.argv:
        print_coverage(requirements, citers)

    return 1 if errors else 0


def print_coverage(
    requirements: dict[str, set[str]],
    citers: dict[tuple[str, str], list[tuple[Path, str | None]]],
) -> None:
    """Requirement-coverage ledger: per capability, each requirement with
    the count of citing tests (non-spec source citations) across all
    languages. Informational — the caller keeps its own exit code."""
    # Only test/source citations count toward coverage; spec-to-spec
    # cross-references (citing_cap set) are cross-links, not test evidence.
    def test_count(capability: str, req: str) -> int:
        return sum(
            1 for _, cap in citers.get((capability, req), []) if cap is None
        )

    total = 0
    cited = 0
    print("\nrequirement coverage (test citations across all languages)")
    for capability in sorted(requirements):
        print(f"\n{capability}")
        for req in sorted(requirements[capability]):
            count = test_count(capability, req)
            total += 1
            if count:
                cited += 1
            flag = "" if count else "  <- uncited"
            print(f"  [{count}] {req}{flag}")
    print(f"\ncoverage: {cited}/{total} requirements cited")


if __name__ == "__main__":
    sys.exit(main())
