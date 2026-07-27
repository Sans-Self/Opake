# CLAUDE.md — Opake

**opake.app** — An encrypted personal cloud built on the AT Protocol.

## What This Is

Opake uses a self-hosted PDS as the storage and identity layer, with custom lexicons for file management, encryption, and sharing. Encryption follows the git-crypt hybrid pattern: per-document AES-256-GCM content keys, wrapped asymmetrically to authorized DIDs' X25519 public keys. All crypto is client-side — the PDS only ever sees ciphertext.

The name comes from the Dutch-flavored spelling of "opaque."

## Why atproto?

A PDS is essentially cloud storage with an API. It stores signed, schema-validated records in a Merkle Search Tree, plus binary blobs. It doesn't understand or inspect the data — it just manages it. We define custom lexicons under `at.opake.*` to give structure to our files, encryption metadata, and sharing grants. The PDS handles identity (DID-based), authentication, blob storage (up to 50MB default), federation, and sync — all for free.

The PDS is external. It's already running. This project talks to it over XRPC.

## Key Design Decisions

1. **Encryption is client-side only.** No server-side processing (thumbnails, previews, full-text search over encrypted content). Tradeoff accepted.
2. **Grants are separate records**, not inline in the document. Independent creation/deletion, efficient querying, matches the atproto pattern.
3. **Two-layer key for keyrings.** Per-document content key wrapped under group key. Rotating the group key doesn't require re-encrypting blobs.
4. **No revocation guarantee for historical access.** Same as git-crypt. True revocation requires re-encrypting the blob with a new content key.
5. **All metadata is always encrypted.** Names, tags, MIME types, sizes — everything goes through `encryptedMetadata` (AES-256-GCM with the document's content key). Record-level fields are dummies (`name="encrypted"`, `mimeType="application/octet-stream"`). No server-side search/indexing without client-side decryption.
6. **Public keys as PDS records.** atproto DID docs only have signing keys. Opake publishes X25519 encryption public keys as `at.opake.publicKey/self` singleton records.
7. **Multi-device: seed phrase.** Identity keypairs are derived from a BIP-39 24-word mnemonic via PBKDF2 + HKDF. The seed phrase is the default identity creation path — no random keypair fallback. Recovery via `opake recover` (CLI) or the web UI.
8. **Storage trait in opake-core.** Config, Identity, Session types and the `Storage` trait live in core so both CLI (`FileStorage`, filesystem) and web (`IndexedDbStorage`, IndexedDB) share the same contract. Platform-specific I/O is injected, never imported.
9. **Domain API: `Opake` → `FileManager` / `WorkspaceAdmin`.** All CLI commands route through the `Opake` root context; raw functions are `pub(crate)` — the domain types ARE the public API. All mutations auto-persist sessions via `#[signoff]`. Live client state (workspace list, directory trees, inbox) is kept in SSE-patched keepers. Full API surface and construction patterns: [docs/ARCHITECTURE.md § Domain API](docs/ARCHITECTURE.md).
10. **Workspace is the domain concept.** Keyrings are crypto plumbing. The `Workspace` type wraps keyring data with domain semantics. CLI uses `opake workspace`, not `opake keyring`. Lexicon stays `at.opake.keyring` (wire format).
11. **Sensitive types auto-zeroize.** `RedactedDebug` derive macro generates `Zeroize + Drop` for `#[redact]` fields. ContentKey, Identity, DpopKeyPair, Session types are all zeroized on drop. Nested structs chain — dropping an OAuthSession also zeroizes its DpopKeyPair.
12. **WASM is the security boundary.** Tokens, DPoP keys, session credentials, and all crypto MUST live in WASM (opake-core) — JS cannot zeroize memory. Never expose a session accessor or token material to JS; sole exception is TTL-bounded `PendingLogin` state during redirect flows. Details: [docs/ARCHITECTURE.md § WASM Security Boundary](docs/ARCHITECTURE.md) and [docs/AUTH.md](docs/AUTH.md).
13. **Granular OAuth scopes.** Per-collection `repo:at.opake.*` scopes instead of the catch-all `transition:generic`. The scope string is built from `crate::scope::OPAKE_COLLECTIONS` — single source of truth. Adding a new collection means adding it to `OPAKE_COLLECTIONS` (compile-time test enforces this), the lexicon JSON, the permission set (`at.opake.authFullAccess`), and the indexer consumer if indexed.
14. **Core is the protocol; keep it legible.** opake-core is the surface a builder reads to understand Opake — the protocol is the product, clients are convenience. Client-sync machinery (SSE keepers, watchers, optimistic overlays) belongs in `opake-wasm`/SDK/`opake-react`, never core; only a genuine protocol contract earns core placement. Rationale and the keeper carve-out: [docs/ARCHITECTURE.md § Core Is the Protocol](docs/ARCHITECTURE.md).

## Spec Workflow

Specs and change artifacts are never hand-written. All spec work goes through the opsx skills, which drive the openspec CLI (`bunx @fission-ai/openspec` — bare `openspec` is not on PATH):

- `/opsx:propose` — create a change and generate its artifact set (proposal, delta specs, design, tasks)
- `/opsx:apply` — implement an approved change
- `/opsx:sync` — merge delta specs into canon under `openspec/specs/`
- `/opsx:archive` — archive a completed change (offers sync)

Scaffolding a change dir by hand, writing a delta spec outside a change, or editing canon specs directly instead of syncing a delta are all workflow violations — the CLI's scaffolding and status tracking are the source of truth for what a change contains and whether it's apply-ready. Federation-class changes require the spec delta reviewed (red-penned) before implementation starts. `just spec-lint` guards citation integrity; it does not check semantics, so cite the requirement that actually governs the behavior under test.

## Documentation

- **[README.md](README.md)** — Usage, roadmap, build instructions
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — System overview, encryption model, data model, identity derivation, domain API, core/client boundary
- **[docs/CRATE_STRUCTURE.md](docs/CRATE_STRUCTURE.md)** — Detailed file tree for all crates and web frontend
- **[docs/STORAGE.md](docs/STORAGE.md)** — Storage abstraction, local record cache, file permissions
- **[docs/AUTH.md](docs/AUTH.md)** — OAuth/DPoP authentication, multi-account, device pairing
- **[docs/CRYPTO.md](docs/CRYPTO.md)** — Algorithms, constants, key hierarchy, operation reference
- **[docs/FLOWS.md](docs/FLOWS.md)** — Sequence diagrams for every operation
- **[docs/indexer.md](docs/indexer.md)** — Indexer config, auth, API endpoints
- **[lexicons/README.md](lexicons/README.md)** — Full lexicon schema reference
- **[lexicons/EXAMPLES.md](lexicons/EXAMPLES.md)** — Annotated example records
- **[docs/LICENSING.md](docs/LICENSING.md)** — AGPL-3.0 implications for self-hosters, plugin devs, contributors
- **[SECURITY.md](SECURITY.md)** — Vulnerability reporting, scope, response timeline
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — Code style, testing, architecture overview

## Indexer (Elixir)

See **[docs/indexer.md](docs/indexer.md)** for tables, endpoints, deployment, and firehose details. Agent conventions, the new-collection checklist, and test setup live in [apps/indexer/CLAUDE.md](apps/indexer/CLAUDE.md), loaded when working under `apps/indexer/`.

## Communication Style

Conversational peer dynamic. The "don't narrate" rule applies to code output only — status updates, design discussion, and back-and-forth should feel like talking to a colleague, not reading CI logs.

## References

- AT Protocol specs: https://atproto.com/specs
- Lexicon spec: https://atproto.com/specs/lexicon
- PDS self-hosting: https://github.com/bluesky-social/pds
- Custom schemas guide: https://docs.bsky.app/docs/advanced-guides/custom-schemas
- Data model (blob format): https://atproto.com/specs/data-model
- atproto Rust crates: https://tangled.org/ngerakines.me/atproto-crates
- Lexicon community registry: https://github.com/lexicon-community/awesome-lexicons
