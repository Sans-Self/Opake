# Opake — Operation Flows

Sequence diagrams for every operation. All crypto happens client-side — the PDS only stores and serves opaque bytes. Two contexts recur throughout: the **cabinet** (a user's private files, rooted at a `directory/self` singleton on their own PDS) and a **workspace** (a shared, federated keyring chain whose directory records live across every member's PDS).

| File | Topic |
|------|-------|
| [crypto.md](crypto.md) | Key wrapping, content encryption primitives |
| [keyrings.md](keyrings.md) | Workspace create, list, membership supersedes, workspace upload/download, keyring tombstones |
| [documents.md](documents.md) | Upload, download, list, delete |
| [directories.md](directories.md) | Create, delete, recursive delete, path resolution, workspace cascades |
| [revisions.md](revisions.md) | Cross-author editing via superseding records and curatorial substitute cascades |
| [sharing.md](sharing.md) | Resolve, share, revoke, pending shares |
| [authentication.md](authentication.md) | Login, token refresh |
| [pairing.md](pairing.md) | Device-to-device identity transfer via PDS relay |
| [seed-phrase-recovery.md](seed-phrase-recovery.md) | Seed phrase derivation, identity recovery |
