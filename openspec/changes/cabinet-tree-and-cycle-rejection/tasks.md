# Tasks: cabinet-tree-and-cycle-rejection

## 1. Specs (red-pen gate)

- [ ] 1.1 cabinet-tree delta reviewed and approved
- [ ] 1.2 directory-chains cycle-refusal delta reviewed and approved

## 2. Core: cycle check at the domain API

- [ ] 2.1 `FileManager::move_entry` calls `check_cycle` before any write, both contexts. The check needs the loaded `DirectoryTree`; work out whether the callers pass it in or the manager loads it (the cabinet arm currently touches no tree; the workspace arm resolves paths but never builds one)
- [ ] 2.2 Unit regression: cabinet move into own descendant refused by the domain API (not the CLI layer)
- [ ] 2.3 Unit regression: workspace move into own descendant refused before the phase-1 cascade
- [ ] 2.4 CLI and web behavior unchanged (their own checks stay as friendlier earlier surfaces) — verify no double-error UX regression

## 3. Web: missing root creates instead of short-circuiting

- [ ] 3.1 Upload and new-folder on a rootless cabinet route through root creation (pass no directory so core's `ensure_root` runs) instead of short-circuiting on a null `rootUri`
- [ ] 3.2 Sweep apps/web for other null-`rootUri` short-circuits on write paths (move/delete targets, editor save)
- [ ] 3.3 E2e: fresh account's first web write creates the root and lands the file (the previously blocked "fresh web-only user" scenario) — cite cabinet-tree § A missing root is created on demand

## 4. Citation repoints (semantic fixes, no test behavior changes)

- [ ] 4.1 cabinet-folders "creates a folder at the cabinet root…" — directory-chains § Consumers build the live tree from chain heads only → cabinet-tree § Cabinet curatorial writes mutate directory records in place
- [ ] 4.2 cabinet-folders "renames a directory…" — directory-chains § A path's canonical state is the head of a supersede chain → cabinet-tree § Cabinet curatorial writes mutate directory records in place
- [ ] 4.3 cabinet-move "moves a document between folders…" — directory-chains § Consumers build the live tree from chain heads only → cabinet-tree § A cabinet move is one atomic applyWrites
- [ ] 4.4 cabinet-move "refuses moving a folder into its own descendant" — add cite: cabinet-tree § A move that would create a cycle is refused at the domain API
- [ ] 4.5 cabinet-delete-recursive — add cite: cabinet-tree § Deletion removes target records before the parent listing entry; update its "no cite applies" header comment
- [ ] 4.6 cabinet-documents delete-file test — add same deletion cite; update its header comment
- [ ] 4.7 Unit-test cites: move_entry_tests.rs / remove_tests.rs cite the new requirements where they verify them

## 5. Gates

- [ ] 5.1 `just spec-lint` green (0 dangling; coverage ledger shows cabinet-tree cited from day one)
- [ ] 5.2 `just validate` green
- [ ] 5.3 `just e2e-web` green zero-retry (fresh stack)

## 6. Archive

- [ ] 6.1 Sync deltas to canon (new openspec/specs/cabinet-tree/spec.md; directory-chains requirement added)
- [ ] 6.2 Archive the change
