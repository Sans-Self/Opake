## ADDED Requirements

### Requirement: Documents outside the reader's scopes are surfaced, not omitted

Cabinet listing and traversal by a holder of the account's default-scope key SHALL include documents whose scope the reader does not hold, labelled as belonging to a scope the reader cannot open. They SHALL NOT be silently omitted from a listing, and their presence SHALL NOT fail the listing operation.

Omitting them would show an account holder a cabinet that is missing their own data with no indication that anything is absent, which is indistinguishable from loss. Failing the listing would let one unopenable document deny access to every other. Surfacing them labelled is the only outcome that leaves the account holder able to tell what they have and why they cannot read it here.

#### Scenario: a listing shows a scoped document the reader cannot open

- **GIVEN** a cabinet containing default-scope documents and documents under scope `S`, and a reader holding only the default-scope key
- **WHEN** the reader lists the containing directory
- **THEN** every document appears, and those under scope `S` are marked as belonging to a scope the reader does not hold

#### Scenario: an unopenable document does not fail the listing

- **GIVEN** a directory containing one document under a scope the reader does not hold
- **WHEN** the reader lists that directory
- **THEN** the listing succeeds and the remaining documents are readable as usual

#### Scenario: an unopenable document is distinguishable from a damaged one

- **GIVEN** one document under an unheld scope and one whose ciphertext is corrupt
- **WHEN** the reader lists them
- **THEN** the two are reported differently, so a scope gap is never presented as data damage

### Requirement: Tree position carries no access meaning

A document's scope SHALL be determined by the record alone. Moving a document within the cabinet SHALL NOT add, remove, or alter any wrap, and SHALL NOT change which parties can read it. `spec:tree-cabinet § A cabinet move is one atomic applyWrites` is unchanged: a move remains a curatorial edit to directory records.

Nothing enforces a correspondence between where a document sits and who can open it. Presenting the tree as though it conveyed one would invite an account holder to reason about access from folder membership, and that reasoning would be wrong at exactly the moment it mattered.

Clients SHALL therefore indicate a document's scope from the record itself wherever access matters to the reader, and SHALL NOT represent a directory as though it bounded a scope.

#### Scenario: moving a document does not change who can read it

- **GIVEN** a document under scope `S`
- **WHEN** it is moved to a different directory in the cabinet
- **THEN** its wraps are unchanged, and exactly the same parties can open it as before

#### Scenario: a scoped document is marked wherever it sits

- **GIVEN** documents under scope `S` in several different directories
- **WHEN** the account holder lists those directories
- **THEN** each is marked as scoped, from the record's own scope rather than from its location

### Requirement: Directory names require the default-scope key

A directory record's listing entries are record-level fields, not encrypted metadata: the shape of the tree is legible to any party that can read the repository. Only the directory's name is sealed, under a per-directory content key wrapped to the account's default-scope key via the `Cabinet` context in `spec:document-crypto § Wraps are AEAD-bound to their record context`.

Cabinet directory keys SHALL continue to be wrapped to the default-scope key only. A holder of only a scope key SHALL therefore be able to traverse the tree and to append entries to it, and SHALL NOT be able to recover any directory's name.

A client in that position SHALL present directories as unnamed rather than as absent. Flattening a hierarchy whose shape it can see, because it cannot read the labels, discards structure the account holder created.

#### Scenario: a scope key does not open directory metadata

- **GIVEN** a reader holding only a scope `S` key
- **WHEN** it fetches a cabinet directory record
- **THEN** the directory's encrypted metadata does not decrypt, and no directory name is recovered

#### Scenario: a scope-only client still traverses and files

- **GIVEN** a reader holding only a scope `S` key
- **WHEN** it walks the cabinet from the root and adds a document to a directory
- **THEN** the parent-child edges resolve from the listing entries, and the new entry is appended without any directory key

#### Scenario: an unnamed directory is presented as unnamed

- **GIVEN** a client holding only a scope key, presenting documents organized into directories
- **WHEN** it renders the hierarchy
- **THEN** the unreadable directories are marked unnamed, rather than flattened away
