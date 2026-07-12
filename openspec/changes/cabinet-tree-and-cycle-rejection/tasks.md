# Tasks: cabinet-tree-and-cycle-rejection

Depends on tree-family-rename having landed (directory-chains already renamed to tree-chains).

## 1. Specs (red-pen gate)

- [x] 1.1 tree-cabinet delta reviewed and approved
- [x] 1.2 tree-topology delta reviewed and approved
- [x] 1.3 tree-chains scope-out edit (design.md, outside the delta grammar) reviewed and approved

## 2. Core: cycle check at the domain API

- [ ] 2.1 `FileManager::move_entry` refuses cycle-creating moves before any write, both contexts — manager derives descendants from its own reads per design.md (no `DirectoryTree` parameter); documents short-circuit
- [ ] 2.2 Unit regression: cabinet move into own descendant refused by the domain API (not the CLI layer)
- [ ] 2.3 Unit regression: workspace move into own descendant refused before the phase-1 cascade
- [ ] 2.4 Defensive unit test: `collect_descendants`/tree building terminates on cyclic input (if it hangs, file a ticket rather than shipping silently)
- [ ] 2.5 CLI and web behavior unchanged (their own checks stay as friendlier earlier surfaces) — verify no double-error UX regression

## 3. Web: missing root creates instead of short-circuiting

- [ ] 3.1 Upload and new-folder on a rootless cabinet route through root creation (pass no directory so core's `ensure_root` runs) instead of short-circuiting on a null `rootUri`
- [ ] 3.2 Sweep apps/web for other null-`rootUri` short-circuits on write paths (move/delete targets, editor save)
- [ ] 3.3 E2e: fresh account's first web write creates the root and lands the file (the previously blocked "fresh web-only user" scenario) — cite tree-cabinet § A missing root is created on demand

## 4. Citation repoints (semantic fixes, no test behavior changes)

- [ ] 4.1 cabinet-folders "creates a folder at the cabinet root…" — tree-chains § Consumers build the live tree from chain heads only → tree-cabinet § Cabinet curatorial writes mutate directory records in place
- [ ] 4.2 cabinet-folders "renames a directory…" — tree-chains § A path's canonical state is the head of a supersede chain → tree-cabinet § Cabinet curatorial writes mutate directory records in place
- [ ] 4.3 cabinet-move "moves a document between folders…" — tree-chains § Consumers build the live tree from chain heads only → tree-cabinet § A cabinet move is one atomic applyWrites
- [ ] 4.4 cabinet-move "refuses moving a folder into its own descendant" — add cite: tree-topology § A move that would create a cycle is refused at the domain API
- [ ] 4.5 cabinet-delete-recursive — add cite: tree-cabinet § Deletion removes target records before the parent listing entry; update its "no cite applies" header comment
- [ ] 4.6 cabinet-documents delete-file test — add same deletion cite; update its header comment
- [ ] 4.7 Unit-test cites: move_entry_tests.rs / remove_tests.rs cite the new requirements where they verify them

## 5. Gates

- [ ] 5.1 `just spec-lint` green (0 dangling; ledger shows tree-topology and tree-cabinet cited from day one)
- [ ] 5.2 `just validate` green
- [ ] 5.3 `just e2e-web` green zero-retry (fresh stack)

## 6. Archive

- [ ] 6.1 Sync deltas to canon (new openspec/specs/{tree-topology,tree-cabinet}/spec.md; tree-chains scope-out bullet)
- [ ] 6.2 Archive the change
