# Proposal: cabinet-tree-and-cycle-rejection

## Why

The cabinet's tree semantics are canon-homeless. The directory-chains spec explicitly scopes personal (non-workspace) trees out of its coverage — "no supersede chain, additivity check, or fork handling" — but no spec covers what the cabinet does instead: a fixed `self`-rkey root, in-place curatorial updates, atomic single-`applyWrites` moves, and target-records-first deletion ordering. The gap has an observable cost: the cabinet feature-coverage e2e tests could only cite workspace-chain requirements, and two of those citations are semantically misfiled — a cabinet rename test cites "A path's canonical state is the head of a supersede chain" when a cabinet rename is an in-place `putRecord` with no chain at all. The citation linter checks heading existence, not meaning, so misfiled citations silently corrupt the requirement-coverage ledger.

Separately, auditing the tree code surfaced a real enforcement gap: cycle rejection (a directory cannot move into itself or a descendant) exists in core as `check_cycle`, but nothing in the domain API calls it. The CLI calls it at the command layer; the web duplicates the rule in the Move dialog's UI. A raw SDK caller reaches `FileManager::move_entry` with no guard in either context, and for workspace moves nothing anywhere — client or indexer — enforces the invariant. A written cycle detaches a subtree from the root and makes it unreachable.

## What Changes

- A new `cabinet-tree` capability spec defines the personal tree model: fixed root identity, in-place curatorial writes, move atomicity, deletion ordering, and cycle refusal — the counterpart to directory-chains for the single-repo case.
- The directory-chains spec gains a cycle-refusal requirement for workspace moves, with an honest note that the indexer does not validate cycles and client-side enforcement is the only line of defense today.
- `FileManager::move_entry` calls `check_cycle` before any write, in both contexts — lowering the invariant from per-client duplication to the domain API. The CLI's and web's own checks remain as earlier, friendlier surfaces for the same rule.
- A missing cabinet root is canon as a normal, recoverable state: operations create it on demand (core's `ensure_root` already does; the requirement makes it a contract). The web client's empty-cabinet short-circuit — upload and new-folder silently do nothing on a null `rootUri`, stranding a web-only user before their first file — is the known violation and is fixed in this change. Recursive root deletion is sanctioned as a full reset under the same rule: the record deletes, the next write recreates it.
- Dangling-entry repair and orphan collection are explicitly deferred to a future garbage-collection capability spec.
- The misfiled cabinet e2e citations repoint from directory-chains to cabinet-tree requirements, and the previously uncitable cabinet tests (cycle refusal, deletion) gain citations to the new capability.

## Capabilities

### New Capabilities

- `cabinet-tree`: the personal tree's structure and write semantics. Deliberately small — document/directory crypto stays in document-crypto, and everything chain-shaped stays in directory-chains.

### Modified Capabilities

- `directory-chains`: ADDED requirement — a workspace move that would create a cycle is refused before any cascade.

## Impact

- **opake-core** (`crates/opake-core/src/manager/move_entry.rs`): cycle check before the context match; needs the loaded tree, which the callers already hold — signature or lookup consequences to be worked out in implementation. Unit regression per context.
- **Web** (apps/web cabinet write paths): rootless-cabinet writes route through root creation instead of short-circuiting; sweep for other null-`rootUri` early returns.
- **Tests** (`tests/e2e/specs/cabinet-*.spec.ts`): citation repoints and additions; one new e2e — a fresh account's first web write creates the root.
- **Specs**: new `cabinet-tree` canon spec; directory-chains delta. `just spec-lint` stays green; the coverage ledger gains cabinet-tree requirements with citations from day one.
