# Proposal: crypto-context-binding

## Why

Two issues (#49, #50) concern how ciphertexts commit to their context, and both are wire-affecting — fixing them after v1 means a migration; fixing them now costs nothing.

1. The HKDF `info` string that domain-separates key wraps is built by hyphen-joining its fields. Hyphen-joining is ambiguous: two different `(uri, recipient_did)` pairs can produce identical `info` bytes, collapsing the separation. Not exploitable with today's field shapes, but `did:web` identifiers legally contain hyphens — the boundary is not provably unambiguous.
2. Content-blob and metadata encryption pass no associated data (AAD) to AES-256-GCM. A document's blob and its `encryptedMetadata` are sealed under the **same content key**, so a blob↔metadata ciphertext swap within one record decrypts cleanly and is caught only if the plaintext happens not to parse. Cross-record splices are caught only by per-document key uniqueness — an emergent property, not an asserted one.

Designing the AAD surfaced a missing protocol concept: a chain-stable object identity. Keyrings already have one (the genesis URI, carried as `workspace_id`); documents and directories do not, which is why their ciphertexts — copied verbatim across supersedes and cascades — had nothing stable to bind to. This change names that concept **lineage** and gives it to every chained record kind.

## What Changes

- **BREAKING (wire, pre-v1 — no install base, no shim):** the HKDF `info` transcript switches from a hyphen-joined format string to an unambiguous length-prefixed encoding. Every existing wrap becomes unreadable; dev environments reset.
- **BREAKING (wire, same terms):** `encrypt_blob` / `decrypt_blob` and `encrypt_metadata` / `decrypt_metadata` gain an AAD binding each ciphertext to `(lineage anchor, type)` — the stable identity of the object it belongs to, and the field type it seals (`document-blob`, `keyring-metadata`, …). Existing ciphertexts no longer authenticate.
- One shared transcript encoder produces both the HKDF `info` bytes and the AAD bytes.
- **BREAKING (lexicon):** `lineage` — the chain's genesis URI, absent on genesis, carried on every supersede — becomes a universal field on chained record kinds. The keyring's existing `workspaceId` field is renamed to `lineage` (same value, same semantics, uniform name); `document` and `directory` records gain it. The indexer enforces that lineage never flips across a supersede; the client chain walk mirrors the check.
- Directory creation switches from PDS-assigned rkeys to client-generated TIDs: a record that seals ciphertexts bound to its own URI must know that URI at encrypt time. (Documents and keyrings already do this; the switch also makes directory-create retries idempotent.)
- Unused raw crypto exports in `opake-wasm` (`encrypt_blob`, `decrypt_blob`, and siblings with no JS callers) are removed rather than extended.

## Capabilities

### New Capabilities

- `lineage`: the chain-stable object identity — definition and anchor rule, which record kinds carry it, the never-flips enforcement contract (indexer write-time, client mirror), and the client-chosen-rkey requirement for records that seal ciphertexts to their own URI.

### Modified Capabilities

- `document-crypto`: the "Wraps are AEAD-bound to their record context" requirement changes its transcript encoding from delimiter-joined to length-prefixed; a new requirement binds every content/metadata ciphertext to its lineage anchor and field type via AAD.
- `workspace-identity`: the "Genesis URI is the workspace identity" requirement renames the keyring's carried field to `lineage`; the "Group-key wraps are AEAD-bound to genesis" requirement extends the genesis binding to keyring metadata AAD.
- `tree-chains`: the workspace-root requirement drops its PDS-assigned-rkey mandate (root records are client-TID-rkeyed like all directories) and root-chain records carry `lineage`.
- `record-validity`: the three stability contracts (version stability, additive evolution, declared-parameter decryptability) gain an explicit pre-v1 window — `opakeVersion: 1` designates the current draft and may be redefined in place until v1 ships, which is the rule this change (and every prior pre-v1 wire break) operates under.

## Impact

- `crates/opake-crypto`: `hkdf_info` (lib.rs), `content.rs`, `metadata.rs`, `key_wrapping.rs`, test vectors.
- `crates/opake-core`: every `encrypt_*`/`decrypt_*` call site — documents (upload/download/update), keyrings (create/advance/rotate), directories (create/cascade/rename), sharing (pending shares, grants), pairing, cabinet; `records/` types gain `lineage`; directory creation TID switch; chain-walk lineage check.
- `crates/opake-wasm`: context parameters threaded through the exported operation surface; dead raw exports deleted.
- `lexicons/`: `at.opake.keyring` (`workspaceId` → `lineage`), `at.opake.document` and `at.opake.directory` (add `lineage`).
- `apps/indexer`: field rename in the consumer and authority checks; new never-flips validation on document and directory supersedes.
- Dev environments and e2e auth snapshots: existing encrypted records become unreadable; requires `dev-env-reset`.
- Issues resolved: #49, #50. Adjacent: lineage is the carried identity #19's re-homing needs; the new transcript test vectors are a natural seam for #52's known-answer tests (neither in scope).
