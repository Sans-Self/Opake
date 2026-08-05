## MODIFIED Requirements

### Requirement: Wraps are AEAD-bound to their record context

Every wrap SHALL fold a `WrapContext` into the HKDF `info` transcript so that a `WrappedKey` lifted from one record context and replayed into another produces a different wrapping key on unwrap and the AES-KW integrity check fails. The context contributes a tag and a scoping URI (crates/opake-crypto/src/lib.rs, `WrapContext` / `hkdf_info`).

The `info` transcript SHALL be injective: two distinct field tuples MUST never serialize to the same byte string. It SHALL be produced by the shared context-transcript encoder (crates/opake-crypto/src/lib.rs) — a fixed ASCII label followed by each field prefixed with its length as a 32-bit little-endian integer — never by joining fields with a delimiter character. Delimiter-joining is not injective when a field may contain the delimiter (`did:web` identifiers legally contain hyphens), and the transcript feeds key derivation, so an encoding collision is a cross-context key collision.

The encoder has more than one consumer, and its consumers SHALL be enumerated:

- key-derivation `info` transcripts for every wrap context below, and
- the signature over an account's published key record (`spec:account-verification § The signature covers a fixed, versioned transcript that names the account`).

Every consumer SHALL declare a fixed, version-pinned list of covered fields, identified by a distinct context label. A consumer SHALL NOT feed the encoder a variable projection — "every field present", "every field the writer knew" — because injectivity is a property of distinct field *tuples*, and two parties that disagree on which fields the tuple contains derive different bytes from identical values. Under key derivation that disagreement surfaces as an unwrap failure; under a signature it surfaces as a counterparty's host being reported hostile. Both are the same defect, and the fixed list is what prevents it.

A change to the encoder — its label handling, its length prefix, its field framing — SHALL be treated as a wire-format change with two distinct blast radii. Wraps are re-derivable by the holder of the key material, so a change there costs re-wrapping. Signatures are not: every signature already published by every verified account becomes invalid the moment the encoder changes, every verified account resolves to the error state, and no consumer can distinguish that from an attack until each account republishes. Any change to the encoder SHALL therefore state this consequence and SHALL take the version-bump path (`spec:record-validity § opakeVersion is a stable protocol contract`) rather than be treated as an implementation detail.

The contexts and their bindings:

- `Document { uri }` — a document's own content-key wrap binds the document's AT-URI. A grant that shares the same document to another recipient wraps the same content key under the *same* `Document { uri }` context, so the owner's envelope and every grant unwrap under one consistent tag (crates/opake-core/src/documents/{upload.rs,download_grant.rs}). Grants are otherwise the concern of `spec:sharing-grants § A grant is a standalone record, not inline document state`; this spec only fixes the context they bind.
- `Keyring { uri }` — a member's group-key wrap binds the workspace's lineage anchor (the genesis keyring URI), resolved through `Keyring::lineage_anchor`. Binding the head URI instead is a context mismatch that breaks the moment a workspace supersedes; the rule and its regressions belong to `spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`, and this spec defers to it.
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

#### Scenario: a signature consumer's field list is fixed, not projected

- **GIVEN** two clients that disagree on which optional fields a published key record carries
- **WHEN** each builds the signature transcript for that record
- **THEN** both cover the same field list, pinned by the declared scheme version, and both compute the same transcript bytes

#### Scenario: an encoder change is declared, not slipped in

- **WHEN** a change alters the shared context-transcript encoder's framing
- **THEN** it states that every published signature becomes invalid and every verified account resolves to the error state until republished, and it takes the version-bump path rather than shipping as an implementation detail
