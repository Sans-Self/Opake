---
name: opsx-reviewer-cite
description: Add spec: citations to the tests that exercise requirements openspec-reviewer's coverage ledger lists as uncited. Edits test titles on confirmation.
allowed-tools: Bash(openspec-reviewer:*), Bash(openspec:*)
metadata:
  generatedBy: openspec-reviewer 0.1.0
  checksum: 425b78feaf4533a5
---
Add `spec:` citations to the tests that already exercise uncited
requirements. This skill edits test files only, and only their titles
or a comment beside them; it never changes what a test does and never
edits `openspec/specs/` or anything under `openspec/`.

## Steps

1. Run `openspec-reviewer lint --coverage --format json`. Under
   `coverage.capabilities`, collect every entry with `citing_files` of
   `0`; each is `capability § requirement`.
2. For each, search the `source_roots` from `openspec/reviewer.toml` for
   the test that exercises the requirement. Match on the behaviour the
   requirement describes, not on its words: read the requirement's
   scenarios and find the test whose assertions cover them.
3. When you find one, add the citation to the test's title, or to a
   comment on the line above it when the title cannot carry it, in
   double quotes or backticks:

   ```
   spec:<capability> § <requirement name>
   ```

   Copy the requirement name from canon exactly. When the test framework
   has a `cite()` helper listed as `cite_helper` in the configuration,
   use it instead. Do not rename, move or edit the body of the test.
4. When no test exercises a requirement, list it under "no test found".
   Do not write a test; that is a different task with its own review.
5. Show every edit before making it.
6. Run `openspec-reviewer lint --coverage --format json` again and report
   the coverage line, cited over total, before and after, followed by
   the "no test found" list.

## Depends on

- `citations § A citation names a capability and a requirement`
- `citations § Citations are scanned in specs, deltas and source`
- `citations § Coverage lists citing tests per requirement`
