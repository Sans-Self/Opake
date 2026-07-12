# tree-topology

Invariants of the directory tree's shape, independent of how the tree is mutated. Opake has two write models — the cabinet mutates records in place (tree-cabinet), workspaces evolve through supersede chains (tree-chains) — but both produce the same structure: a rooted tree of directories listing entries by target URI. This spec owns the properties that must hold of that shape in both contexts, enforced where the two write models converge: the domain API.

Context-specific machinery stays out: root identity and write semantics are tree-cabinet's and tree-chains' respectively, and crypto is document-crypto's. The vocabulary here is shared ground for future shape work — a listing entry whose target record is gone is *dangling* (visible from the tree, repairable); a record no listing references is an *orphan* (invisible, leaked). Deletion-ordering requirements in both context specs deliberately prefer dangling over orphaned on interruption.

## ADDED Requirements

### Requirement: A move that would create a cycle is refused at the domain API

`FileManager::move_entry` SHALL refuse a move whose target directory is the source or any descendant of the source, before any write, in both cabinet and workspace contexts (`check_cycle`, crates/opake-core/src/directories/move_entry.rs). A written cycle detaches the subtree from the root and makes it unreachable.

Client-layer checks (the CLI command's `check_cycle` call, the web Move dialog disabling the source and its descendants) remain as earlier, friendlier surfaces for the same rule, but the domain API is the enforcement boundary — a caller that skips the UI still cannot write a cycle.

In the workspace context the stakes are higher: a move runs as two sequential cascades — remove from source, then add to target — and a cycle written by the second cascade would detach the subtree on every member's view, not just the writer's. Enforcement is client-side only: the indexer does not validate cycles on directory supersedes, so a hostile or buggy client that bypasses the domain API can still write one. Whether the indexer should reject cycle-creating supersedes — and whether tree building on every consumer must tolerate a malicious cycle without hanging or dropping the subtree silently — is an open question.

#### Scenario: cabinet folder cannot move into its own descendant

- **GIVEN** cabinet folders `a/` and `a/b/`
- **WHEN** the owner attempts to move `a/` into `a/b/`
- **THEN** the move is refused and no directory record is written
- Verified end to end in "refuses moving a folder into its own descendant" (tests/e2e/specs/cabinet-move.spec.ts); decision logic in crates/opake-core/src/directories/move_entry_tests.rs

#### Scenario: workspace move into a descendant is refused before phase one

- **GIVEN** workspace directories `a/` and `a/b/` and a member with directory authority
- **WHEN** the member attempts to move `a/` into `a/b/`
- **THEN** the move is refused before the source-removal cascade writes anything; both listings are unchanged on every PDS
- Decision logic in crates/opake-core/src/directories/move_entry.rs::`check_cycle` and its tests (move_entry_tests.rs)

## Non-requirements

- Dangling-entry repair and orphan collection. Both deletion orderings prefer a dangling entry over an orphan on interruption; nothing sweeps either. Repair belongs to a future garbage-collection capability spec, which inherits this spec's vocabulary.
- Cycle tolerance in tree consumers. The domain-API guard covers honest clients only; whether every tree builder must terminate on maliciously cyclic input is carried as an open question above, not a requirement here.
