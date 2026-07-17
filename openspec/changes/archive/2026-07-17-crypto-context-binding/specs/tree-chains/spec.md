# tree-chains — delta for crypto-context-binding

## MODIFIED Requirements

### Requirement: The workspace root is a flag-marked chain, forward-walked from genesis

The workspace-root path SHALL have no deterministic URI or rkey convention. Every record in the root chain SHALL be an ordinary directory record with a client-generated TID rkey (`spec:lineage § Records that seal ciphertexts to their own URI choose their own rkey` — the genesis root's metadata AAD binds its own URI, so the URI must be known before encryption), marked `isWorkspaceRoot: true` and carrying `workspaceId` set to the genesis keyring URI (`spec:workspace-identity § Genesis URI is the workspace identity`). Like every directory record, root-chain records after genesis carry `lineage` set to the root chain's genesis URI (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`).

Lineage names the chain; it does not resolve the head. A consumer SHALL find the current root by taking the flagged record that no other record supersedes, and SHALL advance to the head by walking `supersedes` back-edges forward, never by pinning the genesis record.

Creating the first directory in a fresh workspace SHALL genesis-cascade the root: when no indexed root head exists, the write builds a genesis root leaf carrying the new entry rather than superseding a root that was never created.

#### Scenario: root resolves to the chain head, not genesis

- **GIVEN** a workspace-root chain that has been superseded at least once
- **WHEN** a client builds its tree from an indexer snapshot containing the whole root chain
- **THEN** `root_uri` is the flagged record with no successor, reached by forward-walking from the genesis candidate
- Verified in `DirectoryTree::set_root` and `from_records` (crates/opake-core/src/directories/tree.rs); without the forward walk `root_uri` pins to the immutable superseded genesis and stale entries leak into the snapshot
