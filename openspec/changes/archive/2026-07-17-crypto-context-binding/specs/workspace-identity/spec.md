# workspace-identity — delta for crypto-context-binding

## MODIFIED Requirements

### Requirement: Genesis URI is the workspace identity

The genesis keyring URI SHALL be the sole stable identifier of a workspace. Every keyring record after genesis SHALL carry `lineage` set to the genesis URI — the workspace is the keyring chain's object, and its identity field is the universal chain-identity field (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`), not a keyring-specific one. Any component holding a keyring record SHALL derive the workspace identity as `lineage.unwrap_or(record_uri)` — a record without `lineage` is genesis and identifies itself.

The resolved `Workspace.uri` SHALL be the genesis URI, regardless of which chain record the resolution started from.

Records that *reference* a workspace from outside the keyring chain (documents, directories) continue to do so via their `workspaceId` field; that field's value is the keyring chain's lineage. `lineage` always answers "which object am I"; `workspaceId` always answers "which workspace do I belong to".

#### Scenario: identity survives a supersede

- **GIVEN** a workspace whose keyring has been superseded at least once
- **WHEN** any member resolves the workspace from the current head
- **THEN** `Workspace.uri` equals the genesis URI, not the head URI

#### Scenario: identity derived from an arbitrary chain record

- **GIVEN** any keyring record in the chain (genesis, superseded intermediate, or head)
- **WHEN** a component derives the workspace identity from it
- **THEN** the result is `lineage.unwrap_or(record_uri)` and equals the genesis URI

### Requirement: Group-key wraps are AEAD-bound to genesis

Every member group-key wrap SHALL bind its AEAD context to the genesis URI, via `Keyring::wrap_anchor(self_uri)` (crates/opake-core/src/records/keyring.rs) — the keyring's lineage anchor. Every unwrap SHALL reconstruct the context the same way. Wrapping or unwrapping against a head URI is a context mismatch and SHALL NOT occur.

The same genesis binding SHALL extend one layer down to the keyring's `encryptedMetadata`: its AES-256-GCM AAD names the lineage anchor with the `keyring-metadata` type (`spec:document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`), never a head URI. Chain advances copy the metadata ciphertext verbatim into new head records (crates/opake-core/src/opake.rs), so a head-URI binding would break every advance; the anchor is the identity the ciphertext travels under for its whole life.

#### Scenario: decrypt after supersede

- **GIVEN** a workspace superseded after a member was added
- **WHEN** that member unwraps their group key from the current head
- **THEN** the unwrap succeeds using the genesis URI as AEAD context
- Regression: `bug__superseded_keyring_decrypts_name_via_genesis_anchor` (shipped fix `2c9b32d`)

#### Scenario: keyring metadata decrypts from any head in the chain

- **GIVEN** a workspace whose keyring metadata ciphertext has been carried verbatim across one or more supersedes
- **WHEN** a member decrypts the workspace name from the current head record
- **THEN** the AAD reconstructed from the head's lineage anchor matches the AAD it was sealed under and decryption succeeds
