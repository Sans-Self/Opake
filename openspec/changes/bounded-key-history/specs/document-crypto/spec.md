## MODIFIED Requirements

### Requirement: Keyring reads select the group key by the document's rotation

A keyring-encrypted document SHALL be decrypted using the group key generation named by its `keyringRef.rotation`, not the workspace's current group key. A reader SHALL select a usable current key when its generation matches, or resolve that rotation's historical key through the accepted head's authenticated lookup. The reader SHALL NOT need to fetch every intervening rotation or membership-authority record merely to locate decryption material. A missing current key does not prevent historical-key resolution.

If no key is available for the document's rotation, the read SHALL fail with an explicit error rather than attempting the wrong key — the reader may still be admitted but lack that rotation's wrap, or the rotation may be unknown.

Because a document keeps referencing the rotation it was sealed under, rotating the group key SHALL NOT require re-encrypting any blob or content key: new documents use the new group key, old documents stay readable through the authenticated historical-key store, and the removed member cannot derive keys minted after their removal (rotation-on-removal mechanics are `spec:workspace-membership § Removal rotates the group key; leave does not`; docs/CRYPTO.md, "Key Rotation"; CLAUDE.md decisions #3 and #4). This is the git-crypt posture: fresh content keys protected only by an unexposed group-key generation exclude removed members, but historical access is not revoked and already-encrypted/in-flight old-key writes can remain readable after removal (`spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`).

Historical-only access SHALL be representable without a current group key. Missing-current-wrap members SHALL retain their role and reads supported by usable historical keys; operations requiring the current group key SHALL fail explicitly until it is available, rather than encrypting new content under a historical key. Content whose required key is absent SHALL be shown as unavailable for that rotation, not as proof of non-membership or corrupt ciphertext. Identity adoption still requires the rotation-0 derivation check (`spec:workspace-identity § Identity adoption verifies by derivation`).

New or changed workspace ciphertext SHALL follow `spec:document-crypto § Workspace writes refresh rotation and do not reuse exposed content keys`; readability through history is not permission to write under a historical key.

#### Scenario: post-rotation reader opens a pre-rotation document

- **GIVEN** a document sealed at rotation 0 and a reader who joined and derived the rotation-0 key from authenticated historical-key storage
- **WHEN** they download it after the workspace has rotated to a later generation
- **THEN** rotation-selected lookup returns the historical key and the content key unwraps

#### Scenario: missing rotation key fails loudly

- **GIVEN** a keyring-encrypted document whose rotation the reader has no key for
- **WHEN** they attempt to unwrap the content key
- **THEN** the operation errors ("no group key available for rotation N"), rather than trying another key or claiming that missing key material proves non-membership

#### Scenario: an admitted member reads history without the current key

- **GIVEN** the live head retains Carol's DID and role without a current wrap, and its history supplies her usable rotation-0 key
- **WHEN** Carol resolves the workspace and opens a rotation-0 document
- **THEN** identity verification and the historical read succeed without a current key, while opening current-rotation content or uploading with the missing current key fails explicitly
