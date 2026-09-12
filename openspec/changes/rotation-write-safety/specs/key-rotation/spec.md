## ADDED Requirements

### Requirement: Removal confidentiality is a key-generation boundary, not a global clock

A removal-triggered rotation SHALL withhold its newly minted group key from the removed
member. Confidentiality of new content against that member SHALL require a fresh content
key protected exclusively by a group-key generation not available to that member. Wrapping
an already-exposed content key under a newer group key SHALL NOT be described as making
new ciphertext under the same content key confidential from its former holders.

There is no instantaneous global cutoff across independent PDSes. An operation already
encrypted under the old group key can commit after the removal and remain readable by
the removed member. This in-flight exposure is an accepted limitation, not a guarantee
that later indexing or retry can undo disclosure. Clients SHALL still refresh state and
refuse knowingly stale writes under `spec:document-crypto § Workspace writes refresh rotation and do not reuse exposed content keys`.

Historical ciphertext, keys, and plaintext already available to a member SHALL remain
outside revocation guarantees. Rotation itself SHALL NOT require re-encrypting existing
blobs; fresh-key work for an edited document belongs to that edit, not to removal.

#### Scenario: fresh post-removal encryption is protected

- **GIVEN** Bob was removed by the canonical rotation from 7 to 8 and has no key 8
- **WHEN** a writer uses a fresh content key wrapped exclusively under group key 8
- **THEN** Bob's retained key 7 and historical content keys do not decrypt the new ciphertext

#### Scenario: an old-key upload crosses the removal

- **GIVEN** an upload was encrypted under group key 7 before its writer observed removal at rotation 8
- **WHEN** its old-key record commits after the removal
- **THEN** Bob may decrypt it with key 7, and neither indexer rejection nor publishing a replacement under key 8 is claimed to reverse that exposure

#### Scenario: re-wrapping a known content key is not revocation

- **GIVEN** Bob recovered a document's content key before removal
- **WHEN** maintenance wraps that same content key under group key 8
- **THEN** Bob still knows the content key, so it cannot protect a newly encrypted name or edit from him
