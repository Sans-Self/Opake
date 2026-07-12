# Tasks: tree-family-rename

## 1. Specs (red-pen gate)

- [x] 1.1 Rename delta reviewed and approved

## 2. Rename (one commit, lint-guarded)

- [x] 2.1 `git mv openspec/specs/directory-chains openspec/specs/tree-chains`; retitle the spec's `# directory-chains` heading
- [x] 2.2 Repoint all citations tagged `directory-chains` → `tree-chains`: 39 sites across chain_tests.rs, cascade_tests.rs, tree_tests.rs, authority_test.exs, authority_db_test.exs, plus the e2e `cite("directory-chains", …)` calls in tests/e2e/specs/cabinet-*.spec.ts
- [x] 2.3 Update prose pointers in workspace-membership, sharing-grants, and document-crypto canon specs; grep docs/ and comment headers for stragglers
- [x] 2.4 Verify zero remaining `directory-chains` references outside archives and this change's own artifacts

## 3. Gates

- [x] 3.1 `just spec-lint` green (0 dangling; ledger capability column reads tree-chains)
- [x] 3.2 `just validate` green

## 4. Archive

- [x] 4.1 Canon reflects the rename (the rename itself is the sync)
- [x] 4.2 Archive the change
