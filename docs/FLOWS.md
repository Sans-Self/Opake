<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the operation flows 
  or data models in this file, you MUST also update the corresponding MDX 
  content in `web/src/content/` to prevent documentation drift. 
-->

# Opake — Operation Flows

This document has been split into per-topic files for maintainability. See [flows/README.md](flows/README.md) for the index.

| File | Topic |
|------|-------|
| [flows/authentication.md](flows/authentication.md) | Login, token refresh |
| [flows/documents.md](flows/documents.md) | Upload, download, list, delete |
| [flows/directories.md](flows/directories.md) | Create, delete, recursive delete, path resolution |
| [flows/sharing.md](flows/sharing.md) | Resolve, share, revoke |
| [flows/crypto.md](flows/crypto.md) | Key wrapping, content encryption primitives |
| [flows/keyrings.md](flows/keyrings.md) | Create, list, add/remove member, keyring upload/download |
| [flows/revisions.md](flows/revisions.md) | Collaborative editing via revision records (planned) |
| [flows/pairing.md](flows/pairing.md) | Device-to-device identity transfer via PDS relay |
| [flows/multi-device.md](flows/multi-device.md) | Seed phrase identity, BIP-39 derivation (planned) |
