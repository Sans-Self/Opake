## ADDED Requirements

### Requirement: Keyring heads carry a rolling archive of their chain

A superseding keyring record SHALL carry a `chainArchive` field: an ordered array of blob references whose concatenated segments contain the raw canonical-CBOR blocks of every predecessor keyring record in the chain, genesis through the immediate predecessor, in genesis-first chain order. Each segment SHALL be a CARv1 file; segment boundaries split the block sequence below the blob size cap without semantic significance. Genesis keyrings carry no archive — there is no history to carry.

The field is optional on the wire: readers SHALL accept keyring records without it (pre-upgrade chains) and MUST NOT treat its absence as a defect.

#### Scenario: supersede carries the full predecessor history

- **GIVEN** a keyring chain of N records with head H
- **WHEN** a member supersedes H
- **THEN** the new record's `chainArchive` segments concatenate to the blocks of records 1 through N in chain order, including H

#### Scenario: genesis has no archive

- **WHEN** a workspace's genesis keyring is written
- **THEN** it carries no `chainArchive` field

#### Scenario: oversized history segments across blobs

- **GIVEN** a chain whose total block bytes exceed a single blob's size cap
- **WHEN** a supersede builds the archive
- **THEN** the blocks are split across multiple segments in the `chainArchive` array, each below the cap, and readers verify the concatenation as one sequence

### Requirement: Archive maintenance is incremental and verified before extension

A supersede author SHALL build the new archive by extending the prior head's archive with the prior head's own bytes. Before extending, the author SHALL verify the prior archive against its own verified chain walk: every block's CID recomputed from bytes and matched to the corresponding `supersedesCid` pin. If the prior archive is absent, malformed, or disagrees with the verified chain, the author SHALL rebuild the archive from its verified walk instead of extending. A defective archive SHALL never be extended or propagated.

#### Scenario: ordinary supersede appends one block

- **GIVEN** a head whose archive verifies against the author's walked chain
- **WHEN** the author supersedes
- **THEN** the new archive is the prior archive's block sequence plus the prior head's bytes, and no historical host is contacted for archive material

#### Scenario: defective prior archive is rebuilt, not extended

- **GIVEN** a head whose archive fails verification against the author's walked chain
- **WHEN** the author supersedes
- **THEN** the author builds the archive from the records of its own verified walk and the defective archive contributes nothing

#### Scenario: first supersede after upgrade archives the live chain

- **GIVEN** a pre-upgrade chain whose head carries no archive
- **WHEN** a member supersedes it
- **THEN** the author builds the first archive from its verified live walk of the full chain

### Requirement: Archive verification is offline and anchored at the live head

A reader verifying a chain from an archive SHALL fetch only the head record and its archive segments, then verify offline: recompute each block's CID from its bytes, match each block against its successor's `supersedes` + `supersedesCid` pair (the successor's pair is the sole binding between a block's bytes and its URI), confirm the terminal block is the genesis whose URI equals the workspace identity, and run the keyring authority walk over the parsed records unchanged. The head itself SHALL be verified per the content-pin requirement's head rule (`spec:lineage § Supersede references carry a content pin`). No historical host is consulted.

An archive SHALL only widen availability, never trust: a chain a live walk would reject MUST also be rejected when read from an archive.

#### Scenario: cold-start verification with all historical hosts dead

- **GIVEN** a workspace whose every historical author PDS is unreachable and whose head author's PDS is alive
- **WHEN** a new member verifies the chain from the head's archive
- **THEN** verification reaches genesis and the authority walk passes without contacting any historical host

#### Scenario: tampered archive block fails closed

- **GIVEN** an archive segment containing a block whose bytes do not hash to the successor's pinned CID
- **WHEN** a reader verifies the archive
- **THEN** the walk from the head stops at the last verifiable block and the proposed head is not accepted by an authority walk

#### Scenario: archive cannot launder an invalid chain

- **GIVEN** a chain containing a supersede whose author was not a manager of its predecessor
- **WHEN** the chain is verified from an archive
- **THEN** the authority walk rejects it exactly as a live walk would

### Requirement: Archive defects degrade to the live walk

On any archive defect — absent field, unfetchable segment, malformed CAR, block/pin mismatch — the reader SHALL fall back to the live walk and the owning chain's existing dispositions. Archive verification failure is never a terminal error state of its own.

#### Scenario: missing archive falls back to live walk

- **GIVEN** a keyring head without a `chainArchive` field
- **WHEN** a reader verifies the chain
- **THEN** it performs the live walk exactly as before the feature existed

#### Scenario: defective archive falls back to live walk

- **GIVEN** a head whose archive fails verification
- **WHEN** the live walk subsequently verifies the chain to genesis
- **THEN** the chain is accepted — the defective archive does not poison an otherwise verifiable chain
