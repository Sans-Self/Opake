## Context

See proposal.md. `crates/opake-core/src/manager/upload.rs` and `manager/editor.rs` use
cached workspace key material. `crates/opake-core/src/manager/rename.rs` encrypts changed
directory metadata with the existing content key. The document format uses one content
key for blob and metadata; key-wrap rotation does not record when that content key was minted.

## Goals / Non-Goals

**Goals:** truthful post-removal confidentiality and no knowingly stale encryption plan.

**Non-Goals:** an instantaneous global barrier, undoing disclosed ciphertext, re-encrypting
untouched documents during removal, or introducing a separate metadata-key format.

## Decisions

### Refresh before encryption, re-evaluate observed changes

Obtain current head/authority/key before preparing a write instead of using a cached
workspace handle as freshness evidence. If a newer rotation is observed before publish,
discard the stale plan and prepare again with fresh key material, or fail if authority
or the current key is unavailable. A stale or lagging read cannot establish global
freshness; acknowledge rather than conceal the accepted in-flight exposure window.

### Treat the content key, not its latest wrap, as the confidentiality boundary

A document swept from rotation 7 to 8 still has the same content key. Do not infer that
Bob never knew it from the wrap's current rotation. Use fresh content keys for changed
workspace plaintext rather than adding an exposure-history cache or relying on wrapper age.
Preserve existing lineage/AAD bindings and fresh-nonce requirements when re-encrypting.
Unchanged ciphertext copied by a structural cascade is not a new confidential edit.

For a directory rename this requires fresh metadata encryption and a new content-key wrap.
For a file metadata edit, the existing shared-key format also requires re-encrypting that
file's blob. If the client cannot obtain/re-encrypt it, fail the edit before publication.
This consequence is local to the edited file; changing the format to avoid that cost would
be a separate design, not an implicit part of account verification or key rotation.

### Test disclosure, not only indexer acceptance

Race an already-encrypted rotation-7 write with removal at rotation 8 and demonstrate that
Bob can still decrypt the old-key ciphertext. Separately test that an informed writer
uses fresh content-key material and Bob cannot decrypt the new edit. A replacement record
or indexer refusal cannot erase ciphertext the PDS already exposed.

## Risks / Trade-offs

- File metadata edits may require substantial blob work → expose that cost and fail clearly
  when unavailable; never claim a cheap re-wrap protects an exposed key.
- Repeated rotations can invalidate preparation → bounded client retry/cancellation and clear
  errors, not publishing a known-stale plan to force progress.
- Eventual visibility leaves an exposure window → security docs name the cryptographic boundary
  and do not promise a wall-clock cutoff.

## Migration Plan

No new wire representation is required. Update write paths and security documentation together.
The document-read replacement delta carries forward `verified-accounts` historical-only
semantics for the coordinated spec sync; the write-path work itself is independent.
No blob migration runs at rollout. Existing ciphertext stays historical until an actual edit.
