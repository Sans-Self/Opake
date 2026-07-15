# record-validity (delta)

## ADDED Requirements

### Requirement: opakeVersion is a stable protocol contract

Every Opake record MUST carry a top-level `opakeVersion` integer field that sits outside any versioned payload. Its name, type, position and meaning MUST be stable across all schema versions, so that a reader of any vintage can extract `opakeVersion` from any well-formed Opake record. A record whose `opakeVersion` is absent or not an integer is corrupt; readers MUST NOT substitute a default. Because schema evolution is additive and vocabulary is version-pinned, comparing `opakeVersion` against the client's supported version is a complete understanding test: it covers both schema shape and cryptographic vocabulary.

#### Scenario: version field readable across versions

- **WHEN** a client with supported schema version N encounters a well-formed record with `opakeVersion` N+1
- **THEN** the client can extract the record's version and parse the record's payload under its own schema

#### Scenario: version outside the payload

- **WHEN** any Opake record type is serialized under any schema version
- **THEN** `opakeVersion` appears as a top-level field of the record, not nested inside version-dependent structure

#### Scenario: missing version is corrupt

- **WHEN** a record lacks `opakeVersion` or carries a non-integer value there
- **THEN** the record is classified corrupt; no default version is assumed

### Requirement: schema evolution is additive and vocabulary is version-pinned

Schema changes within a collection MUST be limited to field additions that are ignore-safe for reads: fields are never removed, optional fields never become required, new fields are optional, and a new field never changes the meaning of a read performed by a client that ignores it. Union variants, enumerated values and registry vocabularies are NOT ignore-safe: each schema version MUST pin the exact set of registry values it permits, vocabulary MUST be cumulative (version N permits every value pinned at or below N), and introducing a new value MUST bump `opakeVersion` together with the new vocabulary entry. A record that declares version N but uses a registry value outside version N's cumulative vocabulary is corrupt. The vocabulary-bearing fields are a closed, explicitly enumerated list of value identifiers: key-wrap algorithm identifiers (`wrappedKey.algo` and its keyring twin), content-encryption algorithm identifiers (encryption envelope `algo`), encryption public-key algorithm identifiers, pairing KEM/cipher algorithm identifiers, and keyring member roles; extending this list is itself a version bump. Structural discriminants (union `$type` tags such as `keyWrapping.$type` and `document.encryption.$type`) are NOT vocabulary: a new union variant is a structural change and takes the new-NSID path. A change that cannot be expressed as an ignore-safe field addition or a version-bumped vocabulary extension MUST take a new collection NSID. Wire representations of registry values MUST be open (a string, not a closed structural union), so that well-formed records of any version parse under any client.

#### Scenario: newer record parses under older schema

- **WHEN** a client supporting schema version N parses a well-formed record with `opakeVersion` N+1 that follows the evolution rules
- **THEN** the record parses successfully and every version-N field carries its version-N meaning

#### Scenario: vocabulary is cumulative

- **WHEN** a version-N+1 client writes a record using only version-N vocabulary
- **THEN** the record is valid declaring either version, and a version-N client fully understands it when it declares N

#### Scenario: new algorithm arrives with a version bump

- **WHEN** a new key-wrapping algorithm is introduced
- **THEN** it is added to the pinned vocabulary of a new schema version, and records using it declare that version

#### Scenario: undeclared vocabulary is corrupt

- **WHEN** a record declares `opakeVersion` N but uses a registry value that version N's vocabulary does not pin
- **THEN** the record is classified corrupt

### Requirement: version is peeked before structural judgment

Classification MUST extract `opakeVersion` from the raw record value before attempting a typed parse. Because schema evolution is additive, every record of every version MUST contain the required fields of every earlier version — so the client IS entitled to one structural judgment of future-version records: the required-field floor of its own newest known schema. A record whose envelope is well-formed, whose declared version exceeds the client's supported version, and whose payload contains the client's known-version required fields is classified needs-newer-client — no full typed parse is required, and the client MUST NOT judge future-version content beyond the known floor. A record that fails the known floor is corrupt regardless of the version it declares: a claimed future version is a claim to more vocabulary, never an exemption from the past's requirements.

#### Scenario: rule-abiding future version is locked, not corrupt

- **WHEN** a record's envelope parses, its `opakeVersion` peek yields a version above the client's, and the payload contains all required fields of the client's newest known schema
- **THEN** the record is classified needs-newer-client, not corrupt

#### Scenario: laundered garbage stays corrupt

- **WHEN** a record declaring a future `opakeVersion` is missing required fields of the client's known schema (such as the crypto envelope)
- **THEN** the record is classified corrupt, exactly as if it declared a known version

#### Scenario: known version earns full judgment

- **WHEN** a record declares a version the client supports and fails structural parsing or vocabulary validation
- **THEN** the record is classified corrupt

### Requirement: corrupt records are skipped per-record, never wholesale

A record is corrupt when, under a version the client knows, it fails structural parsing, lacks a valid `opakeVersion`, or violates its declared version's cumulative vocabulary. Indexer response parsing (tree snapshots and deltas, workspace listings, inbox listings), SSE upsert events, and PDS collection listing MUST skip corrupt records individually with a warning log. A corrupt record MUST NOT cause the remainder of a response, or any other event, to fail. Each parse surface MUST report corrupt records as references carrying the envelope URI where one is extractable (count-only otherwise), and these MUST reach the client surface.

#### Scenario: malformed directory does not brick the snapshot

- **WHEN** a snapshot response contains a malformed `at.opake.directory` record (missing required fields) alongside well-formed records
- **THEN** the well-formed records all parse and render, the malformed record is skipped with a warning, and a corrupt reference carrying its URI is reported

#### Scenario: corrupt references reach the client

- **WHEN** any corrupt records were skipped while parsing a response or event
- **THEN** the parsed result exposes the corrupt references (or counts, when no URI was extractable) to the consuming client rather than only logging them

### Requirement: SSE delivery matches snapshot delivery

A corrupt or future-version record arriving through an SSE upsert event MUST produce the same client state as the same record arriving in a snapshot: corrupt records are skipped, reported and placeholder-rendered under the same rules; future-version records are kept, marked and mutation-gated under the same rules. An event carrying a corrupt record MUST NOT be dropped silently, and MUST NOT terminate or restart the event stream.

#### Scenario: corrupt upsert converges with snapshot handling

- **WHEN** the same corrupt directory record is delivered to one client via snapshot and to another via SSE upsert
- **THEN** both clients converge on the same rendered state: placeholder (when the URI is known), corrupt reference reported, stream uninterrupted

#### Scenario: corrupt keyring upsert signals distinctly

- **WHEN** an SSE keyring upsert carries a corrupt keyring record
- **THEN** the workspace listing state signals the workspace as unreadable, identically to a corrupt keyring met at bootstrap

### Requirement: future-version records are visible, locked, and actionable

A well-formed record whose `opakeVersion` exceeds the client's supported version MUST remain visible on read paths and MUST NOT be skipped from view. The client MUST mark it distinctly (needs-newer-client) and MUST NOT present it as readable content: its payload may be cryptographically inaccessible to this client, and the display MUST be client-assigned, never derived from partially-understood content. Mutations whose target chain or keyring contains a future-version link MUST be refused, and the refusal MUST carry an actionable message identifying that a newer client version is required — routine version skew among members blocks writing, never reading, and the block MUST be self-explanatory.

#### Scenario: future-version record stays visible but locked

- **WHEN** a sync response contains a well-formed record with `opakeVersion` greater than the client supports
- **THEN** the record remains visible with a client-assigned needs-newer-client presentation, and is not rendered as readable content

#### Scenario: future-version link blocks the write with an actionable message

- **WHEN** a client attempts a mutation on a chain or keyring containing a link with `opakeVersion` greater than it supports
- **THEN** the mutation is refused with an error identifying the link and stating that a newer client version is required, and no write reaches the PDS

### Requirement: cryptographic parameters derive from the record's declaration

When unwrapping or verifying a record, clients MUST derive cryptographic parameters (key-derivation transcripts, algorithm selection) from the record's declared `opakeVersion` and algorithm identifier — never from the client's own compile-time version. The declared values are bound into the key-derivation transcript for domain separation and self-consistency; this binding is not authentication of the declaration, and implementations MUST NOT treat the declared version as attested. A record whose declared version is supported and whose vocabulary is valid MUST be decryptable regardless of which client version wrote it.

#### Scenario: known-vocabulary record from a newer writer decrypts

- **WHEN** a client supporting version N+1 writes a record using version-N vocabulary and declares `opakeVersion` N
- **THEN** a client supporting version N decrypts it successfully

### Requirement: corrupt containers render as placeholders

When a corrupt record's envelope URI is known, and other records within the member's authorized snapshot parent to it, clients MUST render an opaque placeholder node at the position those references establish: the placeholder's display name MUST be client-assigned (never derived from attacker-controllable record content), and child records MUST remain attached and visible beneath it. Corrupt records whose envelope does not yield a URI MUST be counted only. Placeholders and corrupt references MUST only be created for elements the member's authorized snapshot already contains — degradation reporting MUST NOT become a channel for out-of-scope data. The client's degradation logic MUST NOT itself move a skipped record's position; the position follows the surviving references that establish it, and MUST NOT change except when those references change.

#### Scenario: children of a corrupt directory stay visible

- **WHEN** a directory record is corrupt but its envelope URI is known, and other members' records declare it as parent
- **THEN** the tree renders a placeholder node at the directory's position with the children intact beneath it

#### Scenario: placeholder name is client-assigned

- **WHEN** a placeholder node is rendered for a corrupt record
- **THEN** its display name comes from the client, not from any field of the corrupt record

#### Scenario: envelope-unparseable records are count-only

- **WHEN** an element of a response yields no parseable envelope (no URI)
- **THEN** the element is skipped, counted as corrupt, and no placeholder is created

#### Scenario: no placeholder for out-of-scope elements

- **WHEN** a corrupt element arrives that no record in the member's authorized snapshot references
- **THEN** it is counted, and no placeholder or URI disclosure is rendered into the tree

### Requirement: corrupt workspaces are skipped with a distinct signal

A corrupt keyring record in a workspace listing MUST remove only that workspace from the assembled list. The listing result MUST carry a distinct signal for skipped workspaces so clients can indicate that a workspace exists but is unreadable.

#### Scenario: one corrupt keyring does not brick the workspace list

- **WHEN** a workspace listing response contains a corrupt keyring alongside readable ones
- **THEN** the readable workspaces all appear, and the result signals the skipped workspace distinctly from a workspace that does not exist

### Requirement: writes refuse state they do not fully understand

A mutation whose target chain or keyring contains a corrupt or future-version link MUST be refused before any write is issued. An authority walk that encounters a corrupt chain link MUST NOT accept the chain head; the client MUST fall back to the last verifiable state. Maintenance operations MUST NOT delete or revoke records they cannot fully understand: a grant that is corrupt or future-version is never revoked by healing logic.

#### Scenario: chain advance against a corrupt link is refused

- **WHEN** a client attempts to advance a chain whose walk crosses a corrupt record
- **THEN** the mutation is refused with an error identifying the corrupt link, and no write reaches the PDS

#### Scenario: unverifiable head is not accepted

- **WHEN** chain mirror-validation encounters a corrupt link between the last verified state and a proposed head
- **THEN** the proposed head is rejected and the client retains the last verifiable state

#### Scenario: healing never revokes what it cannot read

- **WHEN** grant healing encounters a grant that is corrupt or declares a version newer than the client supports
- **THEN** the grant is left untouched and reported, and no revocation is issued

### Requirement: PDS collection listing policy is per-caller

The per-record degradation policy (skip corrupt with references, keep future-version with marking) applies to collection listings that feed user-facing state: grants, keyrings, directories, documents, and encryption public keys. Pairing collections retain their existing skip-quietly semantics and are explicitly outside this policy; a caller of the shared listing machinery MUST choose its policy explicitly.

#### Scenario: pairing cleanup keeps its semantics

- **WHEN** pairing cleanup lists pairing records and encounters an unparseable one
- **THEN** it skips the record under its existing semantics, without corrupt-reference reporting

#### Scenario: unreadable public key blocks sharing with a reason

- **WHEN** identity resolution fetches a recipient's encryption public-key record that is corrupt or declares a version newer than the sharer's client
- **THEN** the share is refused with a reason distinguishing corrupt-key and newer-client-required from a recipient who has no key at all (which keeps its existing not-ready semantics), and no grant is written

### Requirement: PDS lexicon validation is the first, untrusted layer

Opake lexicons MUST declare every field of the cryptographic envelope required, and the lexicons MUST be published for resolution so that conforming PDS implementations reject malformed `at.opake.*` writes at record creation. This layer prevents honest-author accidents only: it MUST NOT be trusted as sufficient, since a record author controls their own PDS. Client-side degradation MUST NOT depend on it.

#### Scenario: conforming PDS rejects a malformed write

- **WHEN** a client attempts to create an `at.opake.directory` record missing required envelope fields on a PDS that resolves and validates Opake lexicons
- **THEN** the PDS rejects the write

#### Scenario: client protection holds without PDS validation

- **WHEN** a malformed record enters the network through a PDS that did not validate it
- **THEN** every client-side degradation requirement in this specification applies unchanged

### Requirement: indexer validates structure for all versions and vocabulary for known versions

The indexer MUST structurally validate every incoming `at.opake.*` record against the newest lexicons it ships — additive evolution guarantees well-formed records of any version parse structurally. For records declaring a version within the indexer's known range, the indexer MUST additionally enforce that version's pinned vocabulary. Malformed and vocabulary-violating records MUST be refused: rejection, not deletion — the record remains on the author's PDS and MUST NOT enter snapshots or streams. Records declaring a future version pass structural validation only; the vocabulary check does not apply to versions the indexer does not know. A claimed future version MUST NOT bypass structural validation. Client-side read lenience MUST NOT depend on this gate.

#### Scenario: malformed record is refused at ingest

- **WHEN** the firehose delivers an `at.opake.*` record that fails structural validation
- **THEN** the indexer does not index it, and subsequent snapshots and streams do not contain it

#### Scenario: vocabulary violation is refused at ingest

- **WHEN** the firehose delivers a record declaring a known version but using a registry value outside that version's pinned vocabulary
- **THEN** the indexer refuses it

#### Scenario: claimed future version is no structural bypass

- **WHEN** the firehose delivers a structurally malformed record claiming `opakeVersion` above the indexer's known range
- **THEN** the indexer refuses it like any other malformed record

#### Scenario: well-formed future-version record is indexed

- **WHEN** the firehose delivers a record with `opakeVersion` above the indexer's known range that passes structural validation
- **THEN** the indexer indexes and relays it verbatim

#### Scenario: client lenience holds without the gate

- **WHEN** a client consumes a snapshot from an indexer that has not deployed ingest validation and the snapshot contains poison records
- **THEN** the client's per-record degradation applies unchanged
