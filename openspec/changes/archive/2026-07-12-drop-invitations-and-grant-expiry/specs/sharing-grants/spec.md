# sharing-grants delta

## MODIFIED Requirements

### Requirement: A share to a not-yet-ready recipient is queued, not dropped

When resolution returns `RecipientNotReady`, the client SHALL warn the user that the recipient exists but has not set up Opake — the recipient cannot receive the share until they publish an encryption key — before offering to queue. The owner MAY then enqueue an `at.opake.pendingShare` record on their own PDS instead of failing. The pending record SHALL carry the target document, the recipient as the user entered it, and the grant metadata encrypted under the document's content key, so the queue holds no plaintext and the grant can be reconstructed later. Pending shares SHALL expire after `DEFAULT_PENDING_SHARE_TTL_SECONDS` (7 days).

Pending shares are the owner's own outgoing queue and are not indexed: retry SHALL be driven by the daemon listing the owner's `pendingShare` records, re-resolving each recipient, and — once a recipient publishes a key — fetching the content key, creating the grant with the original metadata, and deleting the pending record. A recipient still without a key SHALL leave the record queued; a document that fails permanently (deleted, corrupt, undecryptable) SHALL be skipped for its siblings in the same pass.

#### Scenario: sharing to a not-ready recipient warns before queuing

- **GIVEN** a valid DID whose PDS has no `publicKey/self` record
- **WHEN** the owner shares a document to it
- **THEN** the client surfaces a warning that the recipient has not set up Opake, and queues the share only as an explicit follow-up, never silently

#### Scenario: a queued share completes once the recipient sets up Opake

- **GIVEN** a pending share for a recipient who has since published a `publicKey/self`
- **WHEN** the daemon runs a retry pass within the TTL
- **THEN** it creates the grant with the original permissions and note and deletes the pending record
- Provenance: `retry_pending_shares` (crates/opake-core/src/sharing/pending.rs)

#### Scenario: an expired pending share is discarded

- **GIVEN** a pending share older than the TTL whose recipient still has no key
- **WHEN** the daemon runs a retry pass
- **THEN** the pending record is deleted and no grant is created

### Requirement: Revocation stops future discovery but not historical access

Revoking a share SHALL delete the grant record (design decision 4). Deletion SHALL be guarded to the grant collection so no other record type can be removed through the revoke path. Deleting the grant stops indexer-mediated discovery — the entry disappears from the recipient's inbox via `grant:delete` — but SHALL NOT be presented as revoking access already obtained: the recipient may have cached the unwrapped content key or the plaintext, and the blob is not re-encrypted. A grant carries no expiry; it is open-ended until revoked. Time-boxed sharing, if ever wanted, is a designed feature with a writer and an enforcer, not a record field.

#### Scenario: revoke deletes the grant and clears the inbox entry

- **GIVEN** a grant the owner wants to revoke
- **WHEN** the owner revokes it
- **THEN** the grant record is deleted and the indexer emits `grant:delete` to both personal topics, dropping the entry from the recipient's inbox
- Regression: `revoke_grant` happy path; collection guard `rejects_document_uri` (crates/opake-core/src/sharing/revoke.rs)

#### Scenario: revocation does not reach a cached key

- **GIVEN** a recipient who already downloaded and cached a shared document's content key
- **WHEN** the owner revokes the grant
- **THEN** discovery stops but the recipient's cached key still decrypts the unchanged blob — true revocation requires re-encrypting under a new content key, which this operation does not do

## REMOVED Requirements

### Requirement: Invitation targets hold the stable resource id

**Reason**: The invitation machinery is removed wholesale. No working product loop ever existed — share-type invitations had no mint or redemption path, workspace invitations had no redemption route and no owner-side acceptance discovery, and `acceptInvitation` had zero callers. The lexicons (`at.opake.invitation`, `at.opake.invitationAcceptance`), core API, WASM exports, and SDK surface are all deleted.

**Migration**: Workspace membership additions happen via direct manager add (workspace-membership spec). Sharing to a recipient who has not set up Opake surfaces a warning and offers the pending-share queue. The head-URI ban this requirement leaned on is owned by `spec:workspace-identity § Head URI use is limited to head-record operations and resolution input` and survives with its scenario re-anchored to stored record fields. Re-introducing invitations is a future change that must design the full loop: creation, redemption, and owner-side acceptance discovery.
