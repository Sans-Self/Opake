# lineage — delta for verify-foreign-lineage

## MODIFIED Requirements

### Requirement: Records that seal ciphertexts to their own URI choose their own rkey

Any record kind whose genesis seals a ciphertext bound to its own URI — documents, directories, keyrings — SHALL be created with a client-chosen rkey known before encryption: a client-generated TID, a fixed convention like the cabinet root's `self` rkey (`spec:tree-cabinet § The cabinet tree has a fixed root on the owner's PDS`), or a derived tag like the keyring genesis rkey, which is computed from the genesis group key and owner DID before the record exists (`spec:workspace-identity § Genesis URI is the workspace identity`). Letting the PDS assign the rkey makes the genesis AAD uncomputable at encrypt time and is therefore not permitted for these kinds.

PDS-assigned rkeys remain acceptable only where no ciphertext binds the record's own URI: the pending-share record binds the *target document's* anchor (`spec:document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`), so its own address may be assigned late.

Client-generated rkeys also make creation retries idempotent: a retried `createRecord` at the same rkey either succeeds or reports the record exists, instead of minting a duplicate.

#### Scenario: directory creation knows its URI before encrypting

- **GIVEN** a new directory being created
- **WHEN** its metadata is encrypted
- **THEN** the record's TID was generated client-side first, the AAD binds the resulting URI, and the subsequent `createRecord` uses that TID as the rkey

#### Scenario: keyring genesis knows its URI before encrypting

- **GIVEN** a new workspace being created
- **WHEN** the genesis keyring's metadata and member wraps are built
- **THEN** the rkey was derived from the genesis group key and owner DID first, every URI-bound value binds the resulting genesis URI, and the subsequent `createRecord` uses the derived tag as the rkey

### Requirement: Lineage never flips across a supersede

A superseding record's declared `lineage` SHALL equal its predecessor's lineage anchor. The indexer SHALL reject a supersede whose lineage does not match at write time, alongside its existing chain-authority checks. Clients SHALL mirror the check when walking chains (crates/opake-core/src/directories/chain.rs — `verify_and_walk_chain` serves both directory and keyring chains) and treat a flipped-lineage record as outside the chain, per the read-lenient posture of `spec:record-validity § corrupt records are skipped per-record, never wholesale`.

A record that declares `lineage` without `supersedes` claims chain membership without linking into a chain, and SHALL receive the same disposition as a flipped lineage: outside any chain, skipped per-record in listing surfaces with the existing degradation signals, and not resolvable as chained state. This mirrors the indexer's write-time rejection of the naked-supersede shape without introducing a new failure class.

#### Scenario: indexer rejects a flipped lineage

- **GIVEN** a chain whose anchor is genesis URI G
- **WHEN** a member writes a superseding record declaring `lineage` ≠ G
- **THEN** the indexer rejects the write, and the chain head does not advance

#### Scenario: client walk skips a flipped lineage

- **GIVEN** a snapshot containing a record whose `lineage` disagrees with its predecessor's anchor
- **WHEN** a client selects chain heads
- **THEN** the mismatched record is not treated as part of the chain, and the prior head remains canonical

#### Scenario: naked lineage is outside any chain

- **GIVEN** a record declaring `lineage` with no `supersedes`
- **WHEN** a client encounters it in a snapshot or on a resolution path
- **THEN** it is treated as outside any chain — skipped with the existing per-record degradation signals, never adopted as chained state

## ADDED Requirements

### Requirement: Supersede references carry a content pin

Every superseding record SHALL carry, alongside `supersedes`, the CID of the exact predecessor record it supersedes (`supersedesCid`), stamped by the writer from the chain-head pointer it holds. The pin names the immediate predecessor and SHALL NOT be copied through verbatim-copy paths — each record in a cascade or advance pins its own predecessor.

Readers SHALL compare a fetched predecessor's reported CID against the pin when present. A disagreement classifies the link as unverifiable; the consequence follows the owning chain's existing posture — degradation to the newest fully-verifiable head on directory chains (`spec:tree-chains § Consumers build the live tree from chain heads only`), non-acceptance of the proposed head on authority walks.

**What the pin does and does not guarantee (v1).** The reader compares the pin against the CID the serving host *reports* for the predecessor, not against a hash recomputed from the fetched bytes — clients do not compute atproto CIDs today. So the pin detects CID disagreement between honest, non-colluding hosts (an out-of-date cache, an accidental substitution, an indexer/PDS reporting mismatched heads); it does NOT detect a malicious host that serves tampered bytes while reporting the true CID, because that host controls both. True byte-level tamper-evidence requires recomputing the CID from the fetched bytes and is deferred to the work that needs it — replicated and archival chain serving, where records arrive from untrusted third parties (`spec:lineage` is the field's home; the byte-binding is tracked as replication-tier follow-up). Critically, this limitation does not touch #51: workspace identity adoption is guarded by key derivation (`spec:workspace-identity § Identity adoption verifies by derivation`), never by pins. The pin is defense-in-depth against honest-host inconsistency and a pre-v1 wire reservation so the field exists before the freeze; it is not, at v1, a trust boundary against a hostile host.

#### Scenario: disagreeing predecessor CID is rejected

- **GIVEN** a superseding record pinning its predecessor's CID
- **WHEN** a walk fetches a predecessor whose reported CID differs from the pin
- **THEN** the link is classified unverifiable

#### Scenario: a hostile host reporting a matching CID is not caught at v1

- **GIVEN** a host that serves tampered predecessor bytes while reporting the pinned CID
- **WHEN** a walk fetches it
- **THEN** the pin comparison passes — this is a known v1 limitation, closed only by recomputing the CID from bytes (deferred replication-tier work)

#### Scenario: cascade pins per level

- **GIVEN** a directory cascade superseding records at multiple levels
- **WHEN** each superseding record is built
- **THEN** each pins the CID of its own immediate predecessor, not a pin inherited from elsewhere in the cascade
