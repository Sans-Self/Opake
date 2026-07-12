# document-crypto Specification

## Purpose

Define how a document's contents and metadata are encrypted, how the per-document content key is protected, and how a reader recovers that key long after the workspace it belongs to has rotated its group key.

Opake follows the git-crypt hybrid pattern: each document blob is sealed under its own fresh AES-256-GCM content key, and only that 32-byte key is ever wrapped — asymmetrically to a recipient's public keys (direct mode) or symmetrically under a workspace group key (keyring mode). The PDS stores ciphertext and dummy record-level fields; it never sees a filename, a MIME type, or a plaintext byte. This split — cheap-to-rewrap keys, expensive-to-rewrap blobs — is what lets a workspace rotate its group key on every membership change without re-encrypting stored content (CLAUDE.md decision #3), and it is also why removing a member does not retroactively lock them out of documents they could already read (decision #4).

Two properties drive most of the rules here. First, wraps are context-bound: a `WrappedKey` lifted from one record and replayed into another must fail to unwrap, so the HKDF derivation folds in a context tag and a scoping URI. Second, reads are rotation-aware: a document records which generation of the group key sealed it, and a reader who joined the workspace at rotation 3 must still derive the rotation-0 key to open an old file. Getting the scoping URI wrong (head vs genesis) is the failure mode the workspace-identity spec governs; this spec references that rule rather than restating it.

Terms:

- Content key: a random 256-bit AES key, one per document, sealing both the blob and its metadata.
- Group key: a workspace's shared symmetric key, one per rotation, wrapped per-member in the keyring record.
- Direct encryption: the content key wrapped asymmetrically to one or more DIDs' public-key bundles (`app.opake.document#directEncryption`).
- Keyring encryption: the content key wrapped symmetrically under a group key, referenced by `keyringRef` (`app.opake.document#keyringEncryption`).

## Requirements

### Requirement: Each document has its own random content key

Every document SHALL be encrypted under a content key generated fresh for that document from injected randomness, and that key SHALL seal both the blob and the encrypted metadata. The content key SHALL be AES-256 (32 bytes); the blob cipher SHALL be AES-256-GCM with a fresh 96-bit nonce per encryption.

Content keys SHALL NOT be derived, shared between documents, or reused across encryptions. The per-document key is what bounds the blast radius of an AES-GCM nonce collision to a single document — two documents under the same workspace group key still hold independent content keys, so no (key, nonce) pair is ever shared across documents (docs/CRYPTO.md, "Nonce collision blast radius").

Randomness SHALL be injected, never drawn from an ambient global. Crypto functions take `&mut (impl CryptoRng + RngCore)`; production callers pass `OsRng`, tests pass a deterministic RNG, and WASM callers get `crypto.getRandomValues()` through the same parameter.

#### Scenario: fresh key and nonce per upload

- **GIVEN** a plaintext blob and an injected RNG
- **WHEN** `prepare_upload` (crates/opake-core/src/documents/upload.rs) encrypts it
- **THEN** it calls `generate_content_key(rng)` then `encrypt_blob(&content_key, plaintext, rng)`, producing a random content key and a random 12-byte nonce, and the PDS blob is the raw AES-256-GCM ciphertext with no framing

#### Scenario: two documents never share a key

- **GIVEN** two documents uploaded to the same workspace under the same group key
- **WHEN** each is encrypted
- **THEN** each carries its own content key, so no nonce is ever reused against a shared key

### Requirement: A document is encrypted in exactly one of two modes

The `encryption` field SHALL be a discriminated union of `directEncryption` and `keyringEncryption` (lexicons/app.opake.document.json), and a document SHALL carry exactly one.

Direct mode SHALL carry an envelope holding the cipher `algo` (`aes-256-gcm`), the blob `nonce`, and a non-empty `keys` array of `WrappedKey`s — the content key wrapped once per authorized DID. It is used for cabinet documents and ad-hoc sharing.

Keyring mode SHALL carry a `keyringRef { keyring, wrappedContentKey, rotation }` plus the blob `algo` and `nonce`. `wrappedContentKey` is the content key wrapped under the group key (40 bytes: 32 + 8 AES-KW integrity). It is used for workspace documents, where any member who can unwrap the group key can open the document.

#### Scenario: cabinet document uses direct mode

- **GIVEN** a personal (cabinet) upload with no workspace
- **WHEN** the record is built
- **THEN** `encryption` is `directEncryption`, `envelope.keys` holds one `WrappedKey` for the owner, and `workspaceId` is absent (crates/opake-core/src/records/document.rs)

#### Scenario: workspace document uses keyring mode

- **GIVEN** a workspace upload with a resolved group key
- **WHEN** `prepare_upload_keyring` builds the record
- **THEN** `encryption` is `keyringEncryption`, `keyringRef.wrappedContentKey` is the AES-KW wrap of the content key under the group key, `keyringRef.rotation` is the group key's current generation, and the record carries `workspaceId` set to the genesis keyring URI

### Requirement: Asymmetric wraps use the hybrid post-quantum construction

Every asymmetric content-key or group-key wrap SHALL use `x25519-mlkem768-hkdf-a256kw-v2` (`crypto::HYBRID_WRAP_ALGO`) — a hybrid of X25519 ECDH and ML-KEM-768 encapsulation, combined through HKDF-SHA256 and applied as an AES-256-KW wrap around the key. Unwrap SHALL reject any `WrappedKey` whose `algo` is not this identifier; there is no v1 compatibility shim.

This is deliberately not JWE's `ECDH-ES+A256KW`: the hybrid combiner keeps the wrap confidential against a harvest-now-decrypt-later adversary unless *both* X25519 and ML-KEM-768 are broken. Hybrid post-quantum key establishment is the deployment posture recommended by BSI TR-02102 (Germany) and ANSSI (France); the ML-KEM-768 byte sizes follow NIST FIPS-203 §6. The contract lives in crates/opake-crypto (`key_wrapping.rs`); this spec describes the guarantees, not the byte layout.

The HKDF salt SHALL commit to the recipient's published X25519 key and the ML-KEM ciphertext, so an attacker who substitutes the post-quantum half breaks the AES-KW integrity check rather than producing a usable wrap (splice resistance, Bindel et al., PQCrypto 2019). On unwrap the recipient SHALL derive its own X25519 public key from its private key rather than trusting a value carried in the envelope.

Symmetric content-key-under-group-key wraps SHALL use AES-256-KW (RFC 3394) with no nonce and no post-quantum upgrade — the group key never leaves a small, controlled set of member devices, and AES-KW's determinism removes any nonce-reuse surface (docs/CRYPTO.md).

#### Scenario: wrong algorithm is refused

- **GIVEN** a `WrappedKey` whose `algo` is not `x25519-mlkem768-hkdf-a256kw-v2`
- **WHEN** `unwrap_key` is called on it
- **THEN** it errors before attempting any decryption

#### Scenario: tampered post-quantum half fails closed

- **GIVEN** a hybrid wrap whose ML-KEM ciphertext has been altered
- **WHEN** the recipient unwraps
- **THEN** the salt no longer matches, the derived wrapping key is wrong, and the AES-KW integrity check rejects it rather than yielding a key

### Requirement: Wraps are AEAD-bound to their record context

Every wrap SHALL fold a `WrapContext` into the HKDF `info` string so that a `WrappedKey` lifted from one record context and replayed into another produces a different wrapping key on unwrap and the AES-KW integrity check fails. The context contributes a tag and a scoping URI (crates/opake-crypto/src/lib.rs, `WrapContext` / `hkdf_info`).

The contexts and their bindings:

- `Document { uri }` — a document's own content-key wrap binds the document's AT-URI. A grant that shares the same document to another recipient wraps the same content key under the *same* `Document { uri }` context, so the owner's envelope and every grant unwrap under one consistent tag (crates/opake-core/src/documents/{upload.rs,download_grant.rs}). Grants are otherwise the concern of `spec:sharing-grants § A grant is a standalone record, not inline document state`; this spec only fixes the context they bind.
- `Keyring { uri }` — a member's group-key wrap binds the workspace's *genesis* keyring URI, resolved through `Keyring::wrap_anchor`. Binding the head URI instead is a context mismatch that breaks the moment a workspace supersedes; the rule and its regressions belong to `spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`, and this spec defers to it.
- `PairResponse` — the device-pairing Identity-bundle wrap, single-recipient and single-context, needs no URI (crates/opake-core/src/pairing/).
- `Cabinet` — the personal self-wrap of a content key under the user's own published keys, one cabinet per identity, needs no URI (crates/opake-core/src/directories/mod.rs).

#### Scenario: a grant lifted into a document envelope will not open it

- **GIVEN** a content key wrapped under `Document { uri: A }`
- **WHEN** a reader tries to unwrap it while claiming a different document URI as context
- **THEN** the derived wrapping key differs and the unwrap fails

#### Scenario: document round-trips under its own URI

- **GIVEN** a document uploaded with its content key wrapped under `Document { uri }`
- **WHEN** the owner downloads it
- **THEN** `unwrap_document_key` reconstructs `WrapContext::Document { uri }` from the same document URI and recovers the content key (crates/opake-core/src/documents/download.rs; regression `roundtrip_with_download` in upload.rs)

### Requirement: All document metadata is encrypted

A document's name, MIME type, size, tags, and description SHALL travel only inside `encryptedMetadata`, sealed with the document's content key using AES-256-GCM over the JSON-serialized `DocumentMetadata` (CLAUDE.md decision #5, issue #187). There SHALL be no plaintext metadata mode.

Record-level fields the PDS can see SHALL be dummies: the blob's own MIME type is `application/octet-stream`, and the record SHALL NOT carry a plaintext `name`, `mimeType`, `size`, or `tags`. A reader recovers real metadata only by unwrapping the content key and calling `decrypt_metadata` — the same content key that opens the blob, so metadata and content share one access decision.

#### Scenario: PDS never sees a filename

- **GIVEN** an upload of `report.pdf`
- **WHEN** the record is sent to the PDS
- **THEN** the record has no `name`, `mimeType`, `size`, or `tags` fields, and `encryptedMetadata` carries the ciphertext plus its own nonce (regression `happy_path`, upload.rs — asserts these fields are absent)

#### Scenario: metadata decrypts with the content key

- **GIVEN** a downloaded document
- **WHEN** the reader unwraps the content key
- **THEN** the same key decrypts `encryptedMetadata` back to the original name, MIME type, size, and description (regression `encrypted_metadata_decrypts_to_original`, upload.rs)

### Requirement: Keyring reads select the group key by the document's rotation

A keyring-encrypted document SHALL be decrypted using the group key generation named by its `keyringRef.rotation`, not the workspace's current group key. A reader SHALL resolve the key via `GroupKeys::for_rotation(rotation)` (crates/opake-core/src/workspace.rs), which returns the current key when the rotation matches and otherwise looks it up among the historical keys the reader derived from the keyring's `keyHistory`.

If no key is available for the document's rotation, the read SHALL fail with an explicit error rather than attempting the wrong key — the reader was not a member at that rotation, or the rotation is unknown.

Because a document keeps referencing the rotation it was sealed under, rotating the group key SHALL NOT require re-encrypting any blob or content key: new documents use the new group key, old documents stay readable through `keyHistory`, and the removed member cannot derive keys minted after their removal (rotation-on-removal mechanics are `spec:workspace-membership § Removal rotates the group key; leave does not`; docs/CRYPTO.md, "Key Rotation"; CLAUDE.md decisions #3 and #4). This is the git-crypt posture: forward secrecy holds for content created after removal, but a member who could already read a document may have cached its plaintext, so historical access is not revoked.

#### Scenario: post-rotation reader opens a pre-rotation document

- **GIVEN** a document sealed at rotation 0 and a reader who joined and derived the rotation-0 key from `keyHistory`
- **WHEN** they download it after the workspace has rotated to a later generation
- **THEN** `for_rotation(0)` returns the historical key and the content key unwraps

#### Scenario: missing rotation key fails loudly

- **GIVEN** a keyring-encrypted document whose rotation the reader has no key for
- **WHEN** they attempt to unwrap the content key
- **THEN** the operation errors ("no group key available for rotation N" / "not a member of this workspace at that rotation"), rather than trying another key

### Requirement: The PDS-only download layer will not resolve group keys itself

A layer that has only PDS access and no indexer SHALL NOT resolve a keyring-encrypted document's group keys on its own. Given a keyring-encrypted document and no supplied group keys, it SHALL refuse with an explicit error directing the caller to resolve the workspace and pass `ws.group_keys()`.

It SHALL NOT fall back to fetching the record at `keyringRef.keyring` and gating on that record's member list: `keyringRef.keyring` is the genesis URI, and the genesis record's members, wrapped keys, and `keyHistory` are frozen at creation — a member added by a later supersede is absent, and everyone else would be handed rotation-0 keys. Reaching the live chain head needs the indexer, which this layer does not have. Membership authority living at the chain head is `spec:workspace-identity § Membership authority is the live chain head`; this requirement is the document-side consequence.

#### Scenario: keyring document without keys errors instead of gating on genesis

- **GIVEN** a keyring-encrypted document and a caller who passes no group keys
- **WHEN** `fetch_content_key` (crates/opake-core/src/documents/download.rs) is asked for the content key
- **THEN** it returns an explicit missing-group-keys error and performs no second fetch of the keyring record
- Regression: `bug__keyring_doc_without_keys_errors_instead_of_stale_genesis_gate` (download.rs) — asserts exactly one getRecord and a "group keys" error

#### Scenario: cross-PDS member download uses already-resolved keys

- **GIVEN** a member who resolved the workspace at its head and holds `GroupKeys`
- **WHEN** they download a document hosted on another member's PDS via `download_keyring_document` (crates/opake-core/src/documents/download_keyring.rs)
- **THEN** the primitive fetches the record from the document authority's public endpoint and unwraps with the supplied key for the document's rotation, without re-fetching the keyring or re-checking membership

### Requirement: Key-carrying types zeroize on drop

Types that hold plaintext key material SHALL zeroize on drop. `ContentKey` and the workspace key SHALL be zeroized (via the `RedactedDebug` derive or an explicit `ZeroizeOnDrop`), their `Debug` output SHALL print byte length rather than contents, and `ContentKey` SHALL NOT be `Copy` — duplication that escapes zeroization must require an explicit `Clone` (crates/opake-crypto/src/lib.rs; docs/CRYPTO.md, "Memory Safety"). Borrowed key views (`PrivateKeyBundle`, `PublicKeyBundle`) SHALL redact their `Debug` output and SHALL NOT own the bytes they borrow.

#### Scenario: a content key does not linger after use

- **GIVEN** a `ContentKey` that goes out of scope
- **WHEN** it is dropped
- **THEN** its bytes are overwritten, and any debug print of it while live shows `[32 bytes]`, never the key

## Open questions

- AES-256-GCM-SIV is planned as a `SCHEMA_VERSION` v2 cipher swap (docs/CRYPTO.md, "Why not derived nonces or AES-GCM-SIV"). The migration shape — version-gated decrypt, SIV-only encrypt, a proactive re-encryption command — is documented but not built. The nonce-reuse argument for v1 rests entirely on per-document keys; whether v2 relaxes that is unspecified.
- Bulk re-encryption after a group-key rotation is described in project notes as a daemon job that re-seals documents under the new key, with the `keyHistory` fallback as the safety net. It is not firing reliably today, and the interaction between "blobs never re-encrypt on rotation" (this spec) and an opt-in bulk re-encrypt is not pinned down here.
- Post-quantum authenticity is out of scope: record signatures still use atproto's classical Ed25519 scheme (docs/CRYPTO.md, "Security Properties"). The hybrid construction covers confidentiality only.
- `keyringRef.rotation` selects a group key generation, but nothing in the document record proves the rotation number was not tampered with before the reader resolved keys; the integrity check is the AES-KW unwrap failing under the wrong key. Whether that is a sufficient binding, or whether rotation should be folded into the wrap context, is not settled here.

## Non-requirements

Owned by other specs and not legislated here:

- Which URI (genesis vs head) a wrap or an indexer call must use, and the identity invariant behind `Keyring::wrap_anchor` — the workspace-identity spec.
- Membership authority, the keyring supersede chain, rotation-on-removal mechanics, and who may author a rotation — the workspace-membership spec.
- Grant records, share flows, and recipient management — the sharing-grants spec. This spec only fixes that grant wraps exist and bind the document's `WrapContext::Document { uri }`.
- Directory key wrapping (`directKeyWrapping` / `keyringKeyWrapping`) and directory metadata encryption — the tree-chains spec. Directories reuse the same primitives (content key, hybrid wrap, `keyringRef`) but carry no blob.
- Identity derivation from the BIP-39 mnemonic and the published `publicKey/self` record — outside this spec; documented in docs/CRYPTO.md and docs/ARCHITECTURE.md.
