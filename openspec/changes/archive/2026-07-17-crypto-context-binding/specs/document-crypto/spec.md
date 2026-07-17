# document-crypto — delta for crypto-context-binding

## MODIFIED Requirements

### Requirement: Wraps are AEAD-bound to their record context

Every wrap SHALL fold a `WrapContext` into the HKDF `info` transcript so that a `WrappedKey` lifted from one record context and replayed into another produces a different wrapping key on unwrap and the AES-KW integrity check fails. The context contributes a tag and a scoping URI (crates/opake-crypto/src/lib.rs, `WrapContext` / `hkdf_info`).

The `info` transcript SHALL be injective: two distinct field tuples MUST never serialize to the same byte string. It SHALL be produced by the shared context-transcript encoder (crates/opake-crypto/src/lib.rs) — a fixed ASCII label followed by each field prefixed with its length as a 32-bit little-endian integer — never by joining fields with a delimiter character. Delimiter-joining is not injective when a field may contain the delimiter (`did:web` identifiers legally contain hyphens), and the transcript feeds key derivation, so an encoding collision is a cross-context key collision.

The contexts and their bindings:

- `Document { uri }` — a document's own content-key wrap binds the document's AT-URI. A grant that shares the same document to another recipient wraps the same content key under the *same* `Document { uri }` context, so the owner's envelope and every grant unwrap under one consistent tag (crates/opake-core/src/documents/{upload.rs,download_grant.rs}). Grants are otherwise the concern of `spec:sharing-grants § A grant is a standalone record, not inline document state`; this spec only fixes the context they bind.
- `Keyring { uri }` — a member's group-key wrap binds the workspace's lineage anchor (the genesis keyring URI), resolved through `Keyring::wrap_anchor`. Binding the head URI instead is a context mismatch that breaks the moment a workspace supersedes; the rule and its regressions belong to `spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`, and this spec defers to it.
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

#### Scenario: delimiter-straddling field pairs derive different keys

- **GIVEN** two wraps whose `(uri, recipient_did)` pairs concatenate to identical bytes under delimiter-joining (for example `uri = "…x-a", did = "b"` versus `uri = "…x", did = "a-b"`)
- **WHEN** each derives its wrapping key
- **THEN** the two `info` transcripts differ and the derived wrapping keys differ

## ADDED Requirements

### Requirement: Ciphertexts are AAD-bound to their lineage anchor and type

Every AES-256-GCM content or metadata encryption SHALL pass associated data (AAD) committing to two values: the **lineage anchor** of the record the ciphertext belongs to (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`) and the **type** of field it seals. Every decryption SHALL reconstruct the same AAD from the record it fetched and reject a ciphertext that fails authentication under it (crates/opake-crypto/src/{content.rs,metadata.rs}). The AAD bytes SHALL be produced by the same injective context-transcript encoder the HKDF `info` uses.

The type tag carries real weight where one key seals more than one field: a document's blob and its `encryptedMetadata` are sealed under the same content key, so without AAD a blob↔metadata ciphertext swap inside one record decrypts cleanly and is caught only if the plaintext fails to parse. With the AAD the swap fails GCM authentication. Cross-record splices additionally remain blocked by per-document key uniqueness (`spec:document-crypto § Each document has its own random content key`); the AAD turns that emergent property into an asserted one with failing tests.

Because the anchor is chain-constant, a ciphertext copied verbatim into a superseding record — keyring metadata on a membership advance (crates/opake-core/src/opake.rs), directory metadata through a substitute cascade (crates/opake-core/src/directories/cascade.rs) — still authenticates: the new record declares the same lineage, so the reader reconstructs the same AAD. The binding names the object, not the record.

The seal types and the record whose anchor they bind:

| Ciphertext | Anchor of | Type |
|---|---|---|
| Document blob | the document | `document-blob` |
| Document metadata | the document | `document-metadata` |
| Keyring metadata | the keyring (= workspace identity) | `keyring-metadata` |
| Directory metadata | the directory | `directory-metadata` |
| Pending-share metadata | the *target document* (crates/opake-core/src/sharing/pending.rs — the share record's own rkey is PDS-assigned and unknowable at encrypt time) | `grant-metadata` |
| Pairing identity blob | the sentinel `self:pair-response` (no scoping record exists, mirroring the `PairResponse` wrap context; crates/opake-core/src/pairing/respond.rs) | `pair-identity` |

Genesis records compute their anchor as their own URI, which requires the URI to be known at encrypt time — the client-chosen-rkey rule (`spec:lineage § Records that seal ciphertexts to their own URI choose their own rkey`).

#### Scenario: a blob ciphertext pasted into the metadata slot fails authentication

- **GIVEN** a document whose blob and `encryptedMetadata` are sealed under the same content key
- **WHEN** the blob ciphertext (with its nonce) is presented for decryption as `document-metadata`
- **THEN** GCM authentication fails under the mismatched type rather than the result depending on whether the bytes parse as JSON

#### Scenario: a ciphertext moved to another object fails authentication

- **GIVEN** a metadata ciphertext sealed under document A's anchor
- **WHEN** it is presented for decryption as document B's metadata, under B's content key or a maliciously re-wrapped copy of A's key
- **THEN** the AAD reconstructs to B's anchor, authentication fails, and the splice is rejected independent of key uniqueness

#### Scenario: keyring metadata survives a chain advance

- **GIVEN** a workspace whose keyring metadata was encrypted at genesis
- **WHEN** a membership change writes a new head record carrying the same `encrypted_metadata` bytes and declaring the genesis as its lineage
- **THEN** a member decrypting from the new head reconstructs the anchor-bound AAD and decryption succeeds

#### Scenario: directory metadata survives a substitute cascade

- **GIVEN** a workspace directory whose metadata ciphertext is copied verbatim into a superseding record by a cascade
- **WHEN** a member decrypts the metadata from the new record
- **THEN** the AAD reconstructed from the record's declared lineage matches the AAD it was sealed under and decryption succeeds
