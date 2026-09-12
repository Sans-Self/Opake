## ADDED Requirements

### Requirement: Workspace writes refresh rotation and do not reuse exposed content keys

Before preparing a workspace write, the client SHALL refresh the canonical workspace
head, check current write authority, and obtain the corresponding current group key.
A cached workspace key SHALL NOT substitute for that refresh. If current state or the
required key cannot be obtained, the operation SHALL fail explicitly rather than silently
encrypt using historical state.

If the client observes a superseding rotation before publication, it SHALL discard the
stale encryption plan and re-evaluate authority and keys before proceeding. It SHALL NOT
knowingly publish new encryption under a superseded rotation. Refreshing an eventually
consistent source is not proof that no concurrent removal exists; the already-encrypted,
in-flight boundary remains `spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`.

Changed content and metadata after removal SHALL NOT be encrypted under a content key
available to the removed member. The writer SHALL use fresh content-key material rather
than merely moving the old content key's wrap to the current group key. A sweep that
updated the wrap's rotation SHALL NOT be evidence that the content key was freshly minted.
Copying unchanged historical ciphertext is not new confidential content and requires no
whole-workspace re-encryption.

Where blob and metadata share one content key, a metadata edit requiring a new key SHALL
also re-encrypt that document's blob under the new key. If the edit cannot do so, it SHALL
fail before publishing changed metadata, not silently reuse the exposed key. This rule
introduces neither a second metadata-key format nor a rotation-time blob sweep.

#### Scenario: a stale client prepares an upload

- **GIVEN** a client caches group key 7 but a refresh returns canonical rotation 8
- **WHEN** it prepares an upload
- **THEN** it checks current authority and encrypts with a fresh content key protected by key 8, or fails if key 8 is unavailable

#### Scenario: the writer observes removal while an edit is prepared

- **GIVEN** an edit was prepared under rotation 7 but has not been published
- **WHEN** its writer observes rotation 8
- **THEN** it discards that encryption plan and re-evaluates current authority and keys before publication

#### Scenario: directory rename follows removal

- **GIVEN** Bob knows a directory's old content key and has been removed
- **WHEN** a remaining authorized writer encrypts a new directory name
- **THEN** the changed metadata uses a fresh content key protected by the current group key, and Bob's old content key cannot decrypt it

#### Scenario: swept wrapping does not conceal an exposed key

- **GIVEN** a document's old content key was exposed to Bob and its wrap was later swept to rotation 8
- **WHEN** a writer edits its plaintext or metadata after Bob's removal
- **THEN** the writer uses fresh key material rather than assuming the rotation-8 wrap made the old content key secret again

#### Scenario: file metadata cannot be safely rekeyed without its blob

- **GIVEN** a file uses one exposed key for its blob and metadata and the blob cannot be obtained
- **WHEN** a post-removal metadata edit requires a fresh key
- **THEN** the edit fails without publishing a new name under the exposed key or a record whose blob cannot be opened with its metadata key

## MODIFIED Requirements

### Requirement: Keyring reads select the group key by the document's rotation

A keyring-encrypted document SHALL be decrypted using the group key generation named by its `keyringRef.rotation`, not the workspace's current group key. A reader SHALL resolve the key via `GroupKeys::for_rotation(rotation)` (crates/opake-core/src/workspace.rs), which returns a usable current key when available and otherwise selects the matching historical key from the keyring's `keyHistory`; a missing current key does not prevent historical-key resolution.

If no key is available for the document's rotation, the read SHALL fail with an explicit error rather than attempting the wrong key — the reader may still be admitted but lack that rotation's wrap, or the rotation may be unknown.

Because a document keeps referencing the rotation it was sealed under, rotating the group key SHALL NOT require re-encrypting any blob or content key: new documents use the new group key, old documents stay readable through `keyHistory`, and the removed member cannot derive keys minted after their removal (rotation-on-removal mechanics are `spec:workspace-membership § Removal rotates the group key; leave does not`; docs/CRYPTO.md, "Key Rotation"; CLAUDE.md decisions #3 and #4). This is the git-crypt posture: fresh content keys protected only by an unexposed group-key generation exclude removed members, but historical access is not revoked and already-encrypted/in-flight old-key writes can remain readable after removal (`spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`).

Historical-only access SHALL be representable without a current group key. Missing-current-wrap members SHALL retain their role and reads supported by usable historical keys; operations requiring the current group key SHALL fail explicitly until it is available, rather than encrypting new content under a historical key. Content whose required key is absent SHALL be shown as unavailable for that rotation, not as proof of non-membership or corrupt ciphertext. Identity adoption still requires the rotation-0 derivation check (`spec:workspace-identity § Identity adoption verifies by derivation`).

New or changed workspace ciphertext SHALL follow `spec:document-crypto § Workspace writes refresh rotation and do not reuse exposed content keys`; readability through history is not permission to write under a historical key.

#### Scenario: post-rotation reader opens a pre-rotation document

- **GIVEN** a document sealed at rotation 0 and a reader who joined and derived the rotation-0 key from `keyHistory`
- **WHEN** they download it after the workspace has rotated to a later generation
- **THEN** `for_rotation(0)` returns the historical key and the content key unwraps

#### Scenario: missing rotation key fails loudly

- **GIVEN** a keyring-encrypted document whose rotation the reader has no key for
- **WHEN** they attempt to unwrap the content key
- **THEN** the operation errors ("no group key available for rotation N"), rather than trying another key or claiming that missing key material proves non-membership

#### Scenario: an admitted member reads history without the current key

- **GIVEN** the live head retains Carol's DID and role without a current wrap, and its history supplies her usable rotation-0 key
- **WHEN** Carol resolves the workspace and opens a rotation-0 document
- **THEN** identity verification and the historical read succeed without a current key, while opening current-rotation content or uploading with the missing current key fails explicitly
