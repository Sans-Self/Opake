# CLAUDE.md — Opake

**opake.app** — An encrypted personal cloud built on the AT Protocol.

## What This Is

Opake uses a self-hosted PDS as the storage and identity layer, with custom lexicons for file management, encryption, and sharing. Encryption follows the git-crypt hybrid pattern: per-document AES-256-GCM content keys, wrapped asymmetrically to authorized DIDs' X25519 public keys. All crypto is client-side — the PDS only ever sees ciphertext.

The name comes from the Dutch-flavored spelling of "opaque."

## Why atproto?

A PDS is essentially cloud storage with an API. It stores signed, schema-validated records in a Merkle Search Tree, plus binary blobs. It doesn't understand or inspect the data — it just manages it. We define custom lexicons under `app.opake.*` to give structure to our files, encryption metadata, and sharing grants. The PDS handles identity (DID-based), authentication, blob storage (up to 50MB default), federation, and sync — all for free.

The PDS is external. It's already running. This project talks to it over XRPC.

## Key Design Decisions

1. **Encryption is client-side only.** No server-side processing (thumbnails, previews, full-text search over encrypted content). Tradeoff accepted.
2. **Grants are separate records**, not inline in the document. Independent creation/deletion, efficient querying, matches the atproto pattern.
3. **Two-layer key for keyrings.** Per-document content key wrapped under group key. Rotating the group key doesn't require re-encrypting blobs.
4. **No revocation guarantee for historical access.** Same as git-crypt. True revocation requires re-encrypting the blob with a new content key.
5. **Plaintext metadata is opt-in transparency.** Names and tags unencrypted by default for AppView indexing. Full opacity via dummy values + encrypted metadata payload.
6. **Public keys as PDS records.** atproto DID docs only have signing keys. Opake publishes X25519 encryption public keys as `app.opake.publicKey/self` singleton records.
7. **Multi-device: seed phrase** (future). MVP uses plaintext keypair at `~/.config/opake/accounts/<did>/identity.json`.
8. **Storage trait in opake-core.** Config, Identity, Session types and the `Storage` trait live in core so both CLI (`FileStorage`, filesystem) and web (`IndexedDbStorage`, IndexedDB) share the same contract. Platform-specific I/O is injected, never imported.

## Documentation

- **[README.md](README.md)** — Usage, roadmap, build instructions
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Crate structure, encryption model, data model, storage layout, file permissions
- **[docs/FLOWS.md](docs/FLOWS.md)** — Sequence diagrams for every operation
- **[docs/appview.md](docs/appview.md)** — AppView config, auth, API endpoints
- **[lexicons/README.md](lexicons/README.md)** — Full lexicon schema reference
- **[lexicons/EXAMPLES.md](lexicons/EXAMPLES.md)** — Annotated example records
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — Code style, testing, architecture overview

## References

- AT Protocol specs: https://atproto.com/specs
- Lexicon spec: https://atproto.com/specs/lexicon
- PDS self-hosting: https://github.com/bluesky-social/pds
- Custom schemas guide: https://docs.bsky.app/docs/advanced-guides/custom-schemas
- Data model (blob format): https://atproto.com/specs/data-model
- atproto Rust crates: https://tangled.org/ngerakines.me/atproto-crates
- Lexicon community registry: https://github.com/lexicon-community/awesome-lexicons
