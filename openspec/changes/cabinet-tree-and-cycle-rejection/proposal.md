# Proposal: cabinet-tree-and-cycle-rejection

## Why

The cabinet's tree semantics are canon-homeless. The directory-chains spec explicitly scopes personal (non-workspace) trees out of its coverage — "no supersede chain, additivity check, or fork handling" — but no spec covers what the cabinet does instead: a fixed `self`-rkey root, in-place curatorial updates, atomic single-`applyWrites` moves, and target-records-first deletion ordering. The gap has an observable cost: the cabinet feature-coverage e2e tests could only cite workspace-chain requirements, and two of those citations are semantically misfiled — a cabinet rename test cites "A path's canonical state is the head of a supersede chain" when a cabinet rename is an in-place `putRecord` with no chain at all. The citation linter checks heading existence, not meaning, so misfiled citations silently corrupt the requirement-coverage ledger.

Separately, auditing the tree code surfaced a real enforcement gap: cycle rejection (a directory cannot move into itself or a descendant) exists in core as `check_cycle`, but nothing in the domain API calls it. The CLI calls it at the command layer; the web duplicates the rule in the Move dialog's UI. A raw SDK caller reaches `FileManager::move_entry` with no guard in either context, and for workspace moves nothing anywhere — client or indexer — enforces the invariant. A written cycle detaches a subtree from the root and makes it unreachable. The invariant is tree topology, not chain machinery — it belongs to neither context alone, which is itself evidence the capability layout was wrong: "directory-chains" framed all directory-tree canon as a workspace matter.

## What Changes

- The directory-tree canon becomes a flat `tree-*` capability family (specs cannot nest; prefixes carry the hierarchy):
  - `tree-topology` (new): shape invariants shared by both write models — cycle refusal at the domain API — plus the dangling/orphan vocabulary both deletion orderings and the future garbage-collection spec build on.
  - `tree-cabinet` (new): the personal tree's write model — fixed root identity, in-place curatorial writes, move atomicity, deletion ordering, root-on-demand.
  - `tree-chains` (the workspace supersede-chain write model; renamed from `directory-chains` by the tree-family-rename change, which lands first): only the cabinet scope-out bullet changes here, narrowing to chain machinery and pointing at the two new specs.
- `FileManager::move_entry` refuses cycle-creating moves before any write, in both contexts — lowering the invariant from per-client duplication to the domain API. The CLI's and web's own checks remain as earlier, friendlier surfaces for the same rule. The tree-chains side carries an honest note: the indexer does not validate cycles, so client-side enforcement is the only line of defense today.
- A missing cabinet root is canon as a normal, recoverable state: operations create it on demand (core's `ensure_root` already does; the requirement makes it a contract). The web client's empty-cabinet short-circuit — upload and new-folder silently do nothing on a null `rootUri`, stranding a web-only user before their first file — is the known violation and is fixed in this change. Recursive root deletion is sanctioned as a full reset under the same rule: the record deletes, the next write recreates it.
- Dangling-entry repair and orphan collection are explicitly deferred to a future garbage-collection capability spec, which inherits tree-topology's vocabulary.
- The misfiled cabinet e2e citations repoint from tree-chains requirements to tree-cabinet/tree-topology ones, and the previously uncitable cabinet tests (cycle refusal, deletion) gain citations. (The mechanical rename sweep of all existing citations is tree-family-rename's, not this change's.)

## Capabilities

### New Capabilities

- `tree-topology`: shape invariants of the directory tree, independent of write model. Deliberately small; grows with the consumer-cycle-tolerance question and grounds the future GC spec.
- `tree-cabinet`: the personal tree's structure and write semantics. Document/directory crypto stays in document-crypto; everything chain-shaped stays in tree-chains.

### Modified Capabilities

- `tree-chains`: cabinet scope-out bullet narrowed to chain machinery only. No requirement content changes.

## Impact

- **Specs**: two new canon specs; tree-chains scope-out bullet reworded. The coverage ledger gains tree-topology and tree-cabinet requirements with citations from day one.
- **Citations**: the semantically misfiled e2e cites repoint (they were citing chain requirements for cabinet behavior); mechanical rename sweep is tree-family-rename's.
- **opake-core** (`crates/opake-core/src/manager/move_entry.rs`): cycle check before the context match; the manager derives descendant knowledge from its own reads (see design.md). Unit regression per context.
- **Web** (apps/web cabinet write paths): rootless-cabinet writes route through root creation instead of short-circuiting; sweep for other null-`rootUri` early returns.
- **Tests** (`tests/e2e/specs/cabinet-*.spec.ts`): citation repoints and additions; one new e2e — a fresh account's first web write creates the root.
