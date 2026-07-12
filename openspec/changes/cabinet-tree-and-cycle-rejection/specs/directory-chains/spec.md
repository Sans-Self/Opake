# directory-chains

## ADDED Requirements

### Requirement: A workspace move that would create a cycle is refused before any cascade

A workspace move SHALL be refused when the target directory is the source or any descendant of the source, before the first cascade writes (`check_cycle` called from `FileManager::move_entry`, crates/opake-core/src/manager/move_entry.rs). A workspace move runs as two sequential cascades — remove from source, then add to target — and a cycle written by the second cascade would detach the subtree from the root on every member's view, not just the writer's.

Enforcement is client-side only: the indexer does not validate cycles on directory supersedes, so a hostile or buggy client that bypasses the domain API can still write one. Whether the indexer should reject cycle-creating supersedes — and whether tree building on every consumer must tolerate a malicious cycle without hanging or dropping the subtree silently — is an open question.

#### Scenario: move into a descendant is refused before phase one

- **GIVEN** workspace directories `a/` and `a/b/` and a member with directory authority
- **WHEN** the member attempts to move `a/` into `a/b/`
- **THEN** the move is refused before the source-removal cascade writes anything; both listings are unchanged on every PDS
- Decision logic in crates/opake-core/src/directories/move_entry.rs::`check_cycle` and its tests (move_entry_tests.rs)
