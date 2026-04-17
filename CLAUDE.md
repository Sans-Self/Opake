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
5. **All metadata is always encrypted.** Names, tags, MIME types, sizes — everything goes through `encryptedMetadata` (AES-256-GCM with the document's content key). Record-level fields are dummies (`name="encrypted"`, `mimeType="application/octet-stream"`). No server-side search/indexing without client-side decryption.
6. **Public keys as PDS records.** atproto DID docs only have signing keys. Opake publishes X25519 encryption public keys as `app.opake.publicKey/self` singleton records.
7. **Multi-device: seed phrase.** Identity keypairs are derived from a BIP-39 24-word mnemonic via PBKDF2 + HKDF. The seed phrase is the default identity creation path — no random keypair fallback. Recovery via `opake recover` (CLI) or the web UI.
8. **Storage trait in opake-core.** Config, Identity, Session types and the `Storage` trait live in core so both CLI (`FileStorage`, filesystem) and web (`IndexedDbStorage`, IndexedDB) share the same contract. Platform-specific I/O is injected, never imported.
9. **Domain API: `Opake` → `FileManager` / `WorkspaceAdmin`.** The `Opake<T, R, S>` struct bundles client + identity + RNG + time + storage. All CLI commands route through Opake (sole holdout: `pair request` on a new device with no identity). Call `.file_context(workspace_name?)` + `.file_manager(&context)` for file ops, `.workspace_admin()` for membership ops (add/remove member, leave). Opake itself handles workspace CRUD, sharing, identity, pairing, config, maintenance. All mutations auto-persist sessions via `#[signoff]` (FileManager) or `#[signoff(self)]` (Opake). Raw functions are `pub(crate)`; the domain types ARE the public API. Live workspace-list state is kept in a `WorkspaceKeeper` (parallel to `TreeKeeper` for directory trees) — bootstrapped by `listWorkspaces`, patched incrementally by SSE `keyring:upsert` / `keyring:delete` events. Incoming shares are tracked in `InboxKeeper` — bootstrapped by `listInbox`, patched by SSE `grant:upsert` / `grant:delete` events (indexer fans both out to owner and recipient personal topics).
10. **Workspace is the domain concept.** Keyrings are crypto plumbing. The `Workspace` type wraps keyring data with domain semantics. CLI uses `opake workspace`, not `opake keyring`. Lexicon stays `app.opake.keyring` (wire format).
11. **Sensitive types auto-zeroize.** `RedactedDebug` derive macro generates `Zeroize + Drop` for `#[redact]` fields. ContentKey, Identity, DpopKeyPair, Session types are all zeroized on drop. Nested structs chain — dropping an OAuthSession also zeroizes its DpopKeyPair.
12. **WASM is the security boundary.** Tokens, DPoP keys, session credentials, and all crypto MUST live in WASM (opake-core). JS cannot zeroize memory — strings are immutable and GC'd on the runtime's schedule. The OAuth login flow itself runs in WASM (`startOAuthLogin`, `completeOAuthLogin`, `loginWithAppPasswordWasm`). Token expiry is checked via `tokenExpiresAt()` (returns only the timestamp). Refresh runs via `proactiveRefresh()` (calls `refresh_token` directly). JS never calls `session()` for auth state — that leaks tokens to the GC. Exception: `PendingLogin` state crosses the boundary during redirect flows (DPoP key in sessionStorage), bounded by a 10-minute TTL and auto-cleared on read.
13. **Granular OAuth scopes.** Per-collection `repo:app.opake.*` scopes instead of the catch-all `transition:generic`. The scope string is built from `crate::scope::OPAKE_COLLECTIONS` — single source of truth. Adding a new collection means adding it to `OPAKE_COLLECTIONS` (compile-time test enforces this), the lexicon JSON, the permission set (`app.opake.authFullAccess`), and the indexer consumer if indexed.

## Documentation

- **[README.md](README.md)** — Usage, roadmap, build instructions
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — System overview, encryption model, data model, identity derivation
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

See **[docs/indexer.md](docs/indexer.md)** for tables, endpoints, deployment, and firehose details.

### Conventions for agents

- Event parser returns tagged tuples or `:ignore`. Indexer dispatches via `dispatch/3` function clauses grouped by domain.
- All public functions have `@spec`. Schemas use `.t()` types.
- Query list functions return `{[results], cursor | nil}`. Cursor format: `"{iso8601}::{uri}"`.
- Workspace-scoped endpoints must check `KeyringQueries.is_member?/2` — returns 403 for non-members.
- `Pagination.build_next_cursor/1` expects items with `:uri` and `:indexed_at` fields. If your schema uses a different PK name, map it.

### Adding a new collection

1. `@collection` constant + parser in `event.ex`, `dispatch/3` clause in `indexer.ex`
2. Migration, schema, query module (follow existing patterns)
3. Collection string in `@wanted_collections` in `consumer.ex`
4. Controller + route if API endpoint needed
5. Tests: event parser, query, pipeline e2e, controller

### Test setup

- DataCase for queries (`async: true`), ConnCase for controllers (`async: false`)
- Controller tests need `set_mox_global`, `verify_on_exit!`, `:ets.delete_all_objects(:key_cache)` in setup
- Pipeline e2e tests feed JSON through `Indexer.process_message/2`

## Communication Style

Conversational peer dynamic. Crosslink's "don't narrate" rule applies to code output only — status updates, design discussion, and back-and-forth should feel like talking to a colleague, not reading CI logs.

## References

- AT Protocol specs: https://atproto.com/specs
- Lexicon spec: https://atproto.com/specs/lexicon
- PDS self-hosting: https://github.com/bluesky-social/pds
- Custom schemas guide: https://docs.bsky.app/docs/advanced-guides/custom-schemas
- Data model (blob format): https://atproto.com/specs/data-model
- atproto Rust crates: https://tangled.org/ngerakines.me/atproto-crates
- Lexicon community registry: https://github.com/lexicon-community/awesome-lexicons
