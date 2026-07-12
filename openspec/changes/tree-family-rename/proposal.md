# Proposal: tree-family-rename

## Why

The directory-tree canon is growing sibling capabilities: a spec for the cabinet's write model and a spec for shape invariants shared by both write models are in flight (change: cabinet-tree-and-cycle-rejection). "directory-chains" as a name frames all directory-tree canon as a workspace-chain matter — the exact framing that left cabinet semantics canon-homeless and let cabinet tests cite chain requirements. Specs cannot nest (the openspec CLI resolves exactly `specs/<capability>/spec.md`; a nested spec is invisible to it, and the citation grammar has no separator), so the hierarchy has to live in the names: a flat `tree-*` family.

Renaming is mechanical and independently land-able, so it ships as its own change rather than riding the cabinet/cycle work: `tree-chains` first, its siblings (`tree-topology`, `tree-cabinet`) when their change lands.

## What Changes

- The `directory-chains` capability renames to `tree-chains`: `git mv openspec/specs/directory-chains openspec/specs/tree-chains` plus the spec's title line. No requirement content changes — headings, scenarios, and scope stay byte-identical, so every citation stays semantically valid across the rename.
- Every citation tagged with the old capability name repoints to the new one in the same commit: 39 sites across 5 Rust/Elixir test files (chain_tests.rs, cascade_tests.rs, tree_tests.rs, authority_test.exs, authority_db_test.exs) and the e2e `cite("directory-chains", …)` calls in the cabinet specs.
- Prose pointers naming directory-chains in other canon specs (workspace-membership, sharing-grants, document-crypto) and any stragglers in docs/ update alongside.
- The cabinet scope-out bullet's narrowing is deliberately NOT here — it points at tree-topology and tree-cabinet, which do not exist until cabinet-tree-and-cycle-rejection lands. That change owns the bullet.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `directory-chains` → `tree-chains`: capability rename only. Zero requirement changes.

## Impact

- **Specs**: one canon directory renamed; ledger identity of every requirement changes capability prefix but nothing else.
- **Tests**: citation-comment edits only; no test behavior changes.
- **Sequencing**: lands before cabinet-tree-and-cycle-rejection, whose deltas already use the new names. Rename and sweep are one commit so `just spec-lint` never sees a dangling state.
