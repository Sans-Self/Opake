# CLAUDE.md — Opake

**opake.app** — An encrypted personal cloud built on the AT Protocol.

## Project Overview

Opake uses a self-hosted PDS as the storage and identity layer, with custom lexicons for file management, encryption, and sharing. The encryption model follows the same hybrid pattern as git-crypt: content is encrypted with per-document symmetric keys (AES-256-GCM), and those keys are wrapped (asymmetrically encrypted) to authorized DIDs' public keys.

The PDS doesn't need modification — it stores encrypted blobs as opaque bytes and encryption metadata as standard atproto records. All crypto happens client-side. The name comes from the Dutch-flavored spelling of "opaque" — because that's exactly what your data is to everyone without the key.

## Core Concepts

### Why atproto?

A PDS is essentially cloud storage with an API. It stores signed, schema-validated records in a Merkle Search Tree, plus binary blobs. It doesn't understand or inspect the data — it just manages it. We define custom lexicons under `app.opake.cloud.*` to give structure to our files, encryption metadata, and sharing grants. The PDS handles identity (DID-based), authentication, blob storage (up to 50MB default), federation, and sync — all for free.

### Encryption Model

Every file is encrypted before upload. The PDS and the network only ever see ciphertext.

```
Plaintext file
  → encrypt with random AES-256-GCM key K → ciphertext blob (uploaded to PDS)
  → wrap K with owner's DID public key → stored in document record
  → to share: wrap K with recipient's DID public key → stored in grant record
```

There are two sharing modes:

**Direct encryption** — the content key is wrapped individually to each authorized DID. Good for ad-hoc sharing of individual files.

**Keyring encryption** — a named group has a shared group key (GK), wrapped to each member's DID. Individual documents have their content key wrapped under GK. Adding a member to the keyring gives them access to all documents under it without per-document changes. This is the git-crypt named-key equivalent.

### Data Stays Put

When sharing a file with a user on another PDS, no data is copied. The recipient's client fetches the document record and blob directly from the owner's PDS via standard atproto APIs (`com.atproto.repo.getRecord`, `com.atproto.sync.getBlob`). The owner remains the single source of truth. Revocation means deleting the grant record (and optionally re-encrypting with a new key).

### Plaintext Metadata Tradeoff

File names, tags, MIME types, and descriptions are intentionally stored unencrypted in the document record. This allows a personal AppView to index and search files server-side without access to encryption keys. If full opacity is needed, these fields can be set to dummy values with real metadata stored inside the encrypted blob — the schema supports both approaches.

## Lexicon Schema

All lexicons live under the `app.opake.cloud.*` namespace (owner controls the `opake.app` domain for NSID authority).

### `app.opake.cloud.defs`
Shared type definitions:
- **wrappedKey** — a symmetric key encrypted to a specific DID's public key. Fields: `did`, `ciphertext` (bytes), `algo` (e.g. `ECDH-ES+A256KW`).
- **encryptionEnvelope** — describes content encryption: `algo` (e.g. `aes-256-gcm`), `nonce` (bytes), and `keys` (array of wrappedKey).
- **keyringRef** — reference to a keyring record plus the content key wrapped under the group key.
- **visibility** — hint string: `private`, `shared`, or `public`.

### `app.opake.cloud.document`
The core file record. Key type: `tid`.
- `name` (string) — plaintext filename
- `mimeType` (string) — original MIME type of unencrypted content
- `size` (integer) — original unencrypted size in bytes
- `blob` (blob) — the encrypted file content, uploaded as `application/octet-stream`
- `encryption` (union) — either `directEncryption` (inline envelope with wrapped keys) or `keyringEncryption` (reference to a keyring + wrapped content key)
- `tags` (array of strings) — plaintext tags for search/categorization
- `parent` (at-uri, optional) — reference to parent document for folder hierarchy
- `visibility`, `description`, `createdAt`, `modifiedAt`

### `app.opake.cloud.keyring`
A named group for shared access. Key type: `tid`.
- `name` (string) — human-readable group name (e.g. "family-photos")
- `algo` (string) — symmetric algorithm the group key targets
- `members` (array of wrappedKey) — the group key wrapped to each member's DID
- `rotation` (integer) — incremented on key rotation after member removal
- `createdAt`, `modifiedAt`

### `app.opake.cloud.grant`
An ad-hoc share grant. Key type: `tid`.
- `document` (at-uri) — the document being shared
- `recipient` (did) — who gets access
- `wrappedKey` (wrappedKey) — the document's content key wrapped to the recipient
- `permissions` (string) — advisory: `read` or `read-write`
- `expiresAt` (datetime, optional) — advisory expiration
- `note` (string, optional) — message to recipient
- `createdAt`

## Architecture

```
┌──────────────────────────┐
│  opake CLI (Rust)    │  ← this is the project
│  - encrypt/decrypt files │
│  - key management        │
│  - DID key resolution    │
│  - keyring/grant CRUD    │
│  - upload/download blobs │
└──────────┬───────────────┘
           │ XRPC (HTTPS)
           ▼
   Your existing PDS          ← external, already running
   (any implementation)

┌──────────────────────────┐
│  AppView + SPA (later)   │  ← future phase
│  - Rust/Axum JSON API    │
│  - indexes metadata      │
│  - TS or Yew frontend    │
│  - client-side crypto    │
└──────────────────────────┘
```

The CLI talks directly to the PDS over XRPC. No middleware, no AppView needed for the core workflow. The AppView becomes relevant later when you want a web UI with search, file browsing, and "shared with me" views.

## Implementation Plan

### Phase 1: CLI Foundation
- [x] Project scaffold: Rust binary with clap, config file for PDS URL + credentials
- [x] Auth: create session via `com.atproto.server.createSession`, manage tokens
- [x] `upload <file>` — generate AES-256-GCM key, encrypt file, upload blob via `com.atproto.repo.uploadBlob`, create `app.opake.cloud.document` record
- [x] `download <at-uri>` — fetch document record, fetch blob via `com.atproto.sync.getBlob`, decrypt, write to disk
- [x] `ls` — list document records via `com.atproto.repo.listRecords`
- [x] `rm <at-uri>` — delete document record (and blob becomes orphaned/GC'd)
- [x] Local keystore for the user's own wrapped keys (so you can decrypt your own files)
- [x] Multi-account support (`--as` flag, `logout`, `set-default`, `accounts` commands)
- [x] Automatic token refresh via `com.atproto.server.refreshSession`

### Phase 2: Sharing
- [x] `app.opake.cloud.publicKey` singleton record for encryption key discovery
- [x] Auto-publish encryption public key on login via `putRecord` (idempotent)
- [x] `resolve <handle-or-did>` — resolve a DID, fetch DID document, extract public key from `app.opake.cloud.publicKey/self`
- [x] `share <at-uri> <did>` — wrap content key to recipient's pubkey, create grant record
- [x] `revoke <grant-at-uri>` — delete grant record
- [x] `download --grant <grant-uri>` — cross-PDS shared file download via grant URI (permanent zero-trust mode; explicit grant selection without relying on discovery)
- [x] `shared` — list grants you've created
- [ ] `inbox` — list grants where you are the recipient (requires AppView — grants live on the owner's PDS, not the recipient's; blocked on Phase 4 minimal AppView)

### Phase 3: Keyrings
- [ ] `keyring create <name>` — generate group key, wrap to self, create keyring record
- [ ] `keyring add-member <keyring> <did>` — wrap group key to new member's pubkey
- [ ] `keyring remove-member <keyring> <did>` — rotate group key, re-wrap to remaining members
- [ ] `keyring ls` — list keyrings
- [ ] `upload <file> --keyring <name>` — encrypt under a keyring instead of direct keys

### Phase 4: AppView + Web UI
- [ ] Minimal Axum AppView: subscribe to PDS event streams, index grants + keyring membership by recipient DID
- [ ] `inbox` command queries AppView for incoming grants
- [ ] Local grant cache: `download --grant` caches grant metadata for offline/zero-trust inbox view
- [ ] SPA frontend (TypeScript or Yew) with client-side crypto via Web Crypto API
- [ ] File browser, search, upload/download, grant management UI

### Phase 5: Stretch
- [ ] Folder hierarchy via `parent` references
- [ ] Versioning (new document records referencing previous versions)
- [ ] Large file sidecar service if 50MB limit becomes a problem
- [ ] Additional record types: notes, bookmarks, etc. under `app.opake.cloud.*`

## Technology

- **Rust CLI** is the primary deliverable. All core functionality (encrypt, upload, download, decrypt, keyring/grant management) lives here.
- The atproto Rust ecosystem exists: see `atproto-crates` on Tangled for identity, OAuth, XRPC, record handling.
- **PDS is external.** The project is PDS-agnostic — it talks to whatever PDS you point it at via XRPC. A PDS is already running; it's not part of this project.
- **Web UI is a later phase.** Either a TypeScript SPA or a Yew (Rust/WASM) app, TBD. The Rust AppView would be a JSON API server (Axum) that the SPA talks to.
- **Infrastructure (Caddy, DNS, VPS) is already in place** and not part of this project.

## Key Design Decisions

1. **Encryption is client-side only.** The PDS never sees plaintext content. This means no server-side processing (thumbnails, previews, full-text search over encrypted content). Tradeoff accepted.

2. **Grants are separate records**, not inline in the document. This allows independent creation/deletion, efficient querying ("what's shared with me?"), and matches the atproto pattern of small, independent records.

3. **Two-layer key for keyrings.** Documents under a keyring still have their own per-document content key, wrapped under the group key. This means rotating the group key doesn't require re-encrypting every document's blob — only re-wrapping the group key to remaining members.

4. **No revocation guarantee for historical access.** Same limitation as git-crypt. If someone had the key and cached the blob, they can still read it. True revocation requires re-encrypting the blob with a new content key and deleting the old blob. The schema supports this workflow but doesn't enforce it.

5. **Plaintext metadata is opt-in transparency.** Names and tags are unencrypted by default for usability. Users who need full opacity can use dummy values and embed real metadata in the encrypted payload.

6. **50MB blob limit is fine for now.** Covers documents, photos, and short media. Large file support (video, archives) can come later via a sidecar service similar to how Tangled uses "knots" alongside the PDS.

7. **Multi-device key management: seed phrase (option C).** The keypair will be derived deterministically from a BIP-39-style mnemonic. Same seed on any device produces the same key. Best UX, but a leaked seed compromises everything. Key export/import as an escape hatch. For MVP: plaintext keypair at `~/.config/opake/accounts/<did>/identity.json`, seed derivation is future work.

8. **Public keys as PDS records.** Since atproto DID documents only contain signing keys (secp256k1/P-256), not encryption keys, Opake publishes X25519 encryption public keys as `app.opake.cloud.publicKey/self` singleton records on each user's PDS. This makes key discovery a simple unauthenticated `getRecord` call.

## File Structure

```
lexicons/
├── README.md                          # Architecture overview and flow diagrams
├── EXAMPLES.md                        # Concrete example records with annotations
├── app.opake.cloud.defs.json          # Shared type definitions
├── app.opake.cloud.document.json      # File/document record
├── app.opake.cloud.publicKey.json     # Encryption public key (singleton)
├── app.opake.cloud.keyring.json       # Group access keyring
└── app.opake.cloud.grant.json         # Ad-hoc share grant
```

## References

- AT Protocol specs: https://atproto.com/specs
- Lexicon spec: https://atproto.com/specs/lexicon
- PDS self-hosting: https://github.com/bluesky-social/pds
- Custom schemas guide: https://docs.bsky.app/docs/advanced-guides/custom-schemas
- Data model (blob format): https://atproto.com/specs/data-model
- atproto Rust crates: https://tangled.org/ngerakines.me/atproto-crates
- Tranquil PDS (Rust): mentioned in atproto self-hosting docs
- Lexicon community registry: https://github.com/lexicon-community/awesome-lexicons
