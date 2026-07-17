# lineage — new capability for crypto-context-binding

## ADDED Requirements

### Requirement: Lineage is the chain's genesis URI, carried on every supersede

Every chained record kind — keyring, document, directory — SHALL carry a `lineage` field on every record after genesis, set to the AT-URI of the chain's genesis record. The genesis record SHALL NOT carry the field: it identifies itself. Any holder of a chain record SHALL derive the object's stable identity through the anchor rule:

```
lineage_anchor(record) = record.lineage.unwrap_or(record's own URI)
```

The anchor is constant across the whole chain — every record, genesis or descendant, resolves to the same value. This generalizes the mechanism the keyring already uses (`Keyring::wrap_anchor`, crates/opake-core/src/records/keyring.rs; identity semantics in `spec:workspace-identity § Genesis URI is the workspace identity`): for a keyring, the lineage *is* the workspace identity. Documents and directories carry lineage with the same shape and the same rule.

Record kinds that never supersede — cabinet directories and cabinet documents (`spec:tree-cabinet § Cabinet curatorial writes mutate directory records in place`) — SHALL NOT carry the field: every such record is permanently its own genesis, the anchor rule resolves to its own URI, and a declared `lineage` on one would be dead weight the never-flips machinery never validates.

Lineage is a carried declaration, not an attested fact — the same trust class as every self-declared chain field. What makes it load-bearing is the never-flips enforcement below plus, where cryptography consumes it, the property that a lying writer only breaks its own record's decryptability (`spec:document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`).

#### Scenario: anchor derived from an arbitrary chain record

- **GIVEN** any record in a supersede chain — genesis, superseded intermediate, or head
- **WHEN** a component derives the object's identity from it
- **THEN** the result is `lineage.unwrap_or(own URI)` and equals the genesis URI

#### Scenario: genesis identifies itself

- **GIVEN** a freshly created record with no `supersedes` and no `lineage`
- **WHEN** its anchor is derived
- **THEN** the anchor is the record's own URI, and every later record in the chain declares that URI as its lineage

### Requirement: Lineage never flips across a supersede

A superseding record's declared `lineage` SHALL equal its predecessor's lineage anchor. The indexer SHALL reject a supersede whose lineage does not match at write time, alongside its existing chain-authority checks. Clients SHALL mirror the check when walking chains (crates/opake-core/src/directories/chain.rs, crates/opake-core/src/chain.rs) and treat a flipped-lineage record as outside the chain, per the read-lenient posture of `spec:record-validity § corrupt records are skipped per-record, never wholesale`.

#### Scenario: indexer rejects a flipped lineage

- **GIVEN** a chain whose anchor is genesis URI G
- **WHEN** a member writes a superseding record declaring `lineage` ≠ G
- **THEN** the indexer rejects the write, and the chain head does not advance

#### Scenario: client walk skips a flipped lineage

- **GIVEN** a snapshot containing a record whose `lineage` disagrees with its predecessor's anchor
- **WHEN** a client selects chain heads
- **THEN** the mismatched record is not treated as part of the chain, and the prior head remains canonical

### Requirement: Records that seal ciphertexts to their own URI choose their own rkey

Any record kind whose genesis seals a ciphertext bound to its own URI — documents, directories, keyrings — SHALL be created with a client-chosen rkey known before encryption: a client-generated TID, or a fixed convention like the cabinet root's `self` rkey (`spec:tree-cabinet § The cabinet tree has a fixed root on the owner's PDS`). Letting the PDS assign the rkey makes the genesis AAD uncomputable at encrypt time and is therefore not permitted for these kinds.

PDS-assigned rkeys remain acceptable only where no ciphertext binds the record's own URI: the pending-share record binds the *target document's* anchor (`spec:document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`), so its own address may be assigned late.

Client-generated rkeys also make creation retries idempotent: a retried `createRecord` at the same rkey either succeeds or reports the record exists, instead of minting a duplicate.

#### Scenario: directory creation knows its URI before encrypting

- **GIVEN** a new directory being created
- **WHEN** its metadata is encrypted
- **THEN** the record's TID was generated client-side first, the AAD binds the resulting URI, and the subsequent `createRecord` uses that TID as the rkey
