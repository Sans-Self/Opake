# tree-cabinet

Define the personal (non-workspace) tree: a single-repo directory tree on the owner's own PDS with a fixed root, mutated by in-place record updates rather than supersede chains. The cabinet is the simple case the federation machinery deliberately does not apply to — one writer, one repo, no curatorial turns — and its semantics are correspondingly simpler: writes are `putRecord`/`applyWrites` updates against stable URIs, moves are atomic, and deletion orders target records before listing entries so an interruption leaves a visible dangling reference rather than an invisible orphan.

This spec owns cabinet-specific tree structure and write semantics only. What a document or directory record encrypts and how (direct wrapping to the owner) is document-crypto's; everything chain-shaped — supersedes, additivity, cascades, forks — is tree-chains', which scopes cabinet chain machinery out. Shape invariants that hold in both contexts are tree-topology's: cycle refusal at the domain API (`spec:tree-topology § A move that would create a cycle is refused at the domain API`) applies to cabinet moves exactly as it does to workspace moves.

## ADDED Requirements

### Requirement: The cabinet tree has a fixed root on the owner's PDS

The cabinet root SHALL be the directory record at `at://<did>/at.opake.directory/self` (`ROOT_DIRECTORY_RKEY`), and every record in the tree — directories and documents — SHALL live in the owner's own repo. Root resolution is deterministic construction of that URI (crates/opake-core/src/directories/mod.rs::`root_directory_uri`); there is no flag-marked chain to forward-walk and no indexer head lookup. Content keys are wrapped directly to the owner (`spec:document-crypto § A document is encrypted in exactly one of two modes`); cabinet records carry no `workspaceId`.

#### Scenario: root resolves without any lookup

- **GIVEN** an authenticated owner with DID `did:plc:x`
- **WHEN** a client builds the cabinet tree
- **THEN** the root is `at://did:plc:x/at.opake.directory/self`, constructed rather than discovered
- Verified in `root_directory_uri` (crates/opake-core/src/directories/mod.rs)

### Requirement: Cabinet curatorial writes mutate directory records in place

A cabinet listing change SHALL be an update of the existing directory record — add-entry and remove-entry rewrite `entries` and `modifiedAt` on the same rkey (`prepare_add_entry` / `prepare_remove_entry`, crates/opake-core/src/directories/entries.rs), and a rename re-encrypts `encryptedMetadata` and `putRecord`s the same record (crates/opake-core/src/manager/rename.rs::`rename_directory`, cabinet arm). Cabinet records SHALL NOT carry `supersedes`; a record's URI is stable for its lifetime, so parent listings keep pointing at it and no cascade exists.

Consumers SHALL resolve cabinet entries by target URI. The `targetCid` pin records the version observed at link time; an in-place update of the target changes its CID without touching the parent, so a cabinet pin goes stale by design and MUST NOT be treated as an integrity claim.

#### Scenario: rename keeps the record URI

- **GIVEN** a cabinet directory at URI `U` named "drafts"
- **WHEN** the owner renames it to "essays" and reloads from a fresh snapshot
- **THEN** the directory still resolves at `U` with the new name; no second record exists for the path
- Verified end to end in "renames a directory and the new name is canonical after reload" (tests/e2e/specs/cabinet-folders.spec.ts)

#### Scenario: created folder appears under its parent after a fresh load

- **GIVEN** an owner creating a folder at the cabinet root
- **WHEN** the client reloads and rebuilds the tree from an indexer snapshot
- **THEN** the folder is listed by the root record itself — the same record, updated — not by any successor
- Verified end to end in "creates a folder at the cabinet root and it survives reload" (tests/e2e/specs/cabinet-folders.spec.ts)

### Requirement: A cabinet move is one atomic applyWrites

Moving an entry between cabinet directories SHALL batch the source-directory update (entry removed) and the target-directory update (entry added, pinned at the target's observed CID) into a single `applyWrites` (crates/opake-core/src/manager/move_entry.rs, cabinet arm) — the entry is never in both listings or neither on the PDS. A move to the entry's current directory SHALL be refused before any write, and adding a target already present in a listing SHALL be refused (`prepare_add_entry` duplicate check).

#### Scenario: moved document lands in exactly one listing

- **GIVEN** a document listed by directory A
- **WHEN** the owner moves it to directory B and reloads from a fresh snapshot
- **THEN** A's listing no longer carries the entry and B's does
- Verified end to end in "moves a document between folders and it survives reload" (tests/e2e/specs/cabinet-move.spec.ts)

### Requirement: Deletion removes target records before the parent listing entry

Deleting a cabinet document SHALL batch the record delete and the parent-listing update into a single `applyWrites` (crates/opake-core/src/manager/delete.rs, cabinet arm). Recursive directory deletion SHALL delete descendant records post-order — children before parents — then the target directory's record, then update the parent listing (crates/opake-core/src/directories/remove.rs); these writes are sequential, and an interruption leaves a dangling listing entry (visible, repairable) rather than an orphaned record with no listing (invisible, leaked). A non-empty directory SHALL NOT be deleted without the recursive flag, and the refusal names the descendant counts. The root SHALL NOT be deleted by a non-recursive call; a recursive root delete is a full reset — it deletes the subtree and the root record itself, and the next write recreates the root on demand.

#### Scenario: recursive delete takes the whole subtree

- **GIVEN** a directory containing a document and a subdirectory
- **WHEN** the owner deletes it recursively and reloads from a fresh snapshot
- **THEN** the directory, its document, and its subdirectory are all gone from the tree
- Verified end to end in tests/e2e/specs/cabinet-delete-recursive.spec.ts; ordering and refusals in crates/opake-core/src/directories/remove.rs

#### Scenario: non-empty directory refuses a plain delete

- **GIVEN** a directory with one document
- **WHEN** the owner deletes it without the recursive flag
- **THEN** the operation is refused with the document/subdirectory counts and nothing is written
- Verified in crates/opake-core/src/directories/remove.rs (`remove_directory` empty check)

### Requirement: A missing root is created on demand

An operation that needs the cabinet root when no root record exists SHALL create it rather than fail or silently do nothing (`FileManager::ensure_root` → `get_or_create_root`, crates/opake-core/src/manager/directory.rs). A missing root is a normal state, not an error: a fresh account has never written one, and a recursive root delete removes it deliberately. Clients SHALL NOT treat a missing root as "nothing to do" — a client that short-circuits a write on a missing root strands the user in a cabinet that can never receive its first file.

#### Scenario: first write on a fresh cabinet creates the root

- **GIVEN** an account that has never written a directory record
- **WHEN** the owner uploads a document or creates a folder
- **THEN** the root record is created as part of the operation and the write lands under it
- Verified in crates/opake-core/src/manager/upload.rs (directory `None` → `ensure_root`)

#### Scenario: write after a recursive root delete recreates the root

- **GIVEN** a cabinet whose root was recursively deleted (record gone)
- **WHEN** the owner performs the next tree write
- **THEN** a fresh root record is created on demand and the write succeeds
- Verified via `get_or_create_root` (crates/opake-core/src/directories/get_or_create_root.rs)

## Non-requirements

- Dangling-entry repair and orphan collection. The deletion ordering deliberately prefers a dangling listing entry over an orphaned record on interruption; the tree builder tolerates the dangle and nothing sweeps it. Repair belongs to a future garbage-collection capability spec, deferred from tree-topology (which owns the dangling/orphan vocabulary), not here.
