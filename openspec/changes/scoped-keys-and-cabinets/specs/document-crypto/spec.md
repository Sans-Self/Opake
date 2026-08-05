## ADDED Requirements

### Requirement: A record names the scope its content key is wrapped to

A document record SHALL carry an optional scope field holding an opaque tag derived from the account's master seed and the scope identifier. An absent field SHALL mean the default scope, so every record written before scopes existed reads unchanged and no migration is required.

The field SHALL carry the derived tag, never the human-readable scope identifier. A readable identifier would put a meaningful plaintext string on a record, which `spec:document-crypto § All document metadata is encrypted` exists to prevent, and would tell the PDS and the indexer which client authored which document.

A scoped document's content key SHALL be wrapped to the account's default-scope key in addition to the scope key, so that reading it requires no derivation the account holder's client cannot perform. An established session holds derived keys and not the mnemonic, and a scope tag cannot be inverted to the identifier needed to derive its key, so a single wrap would leave an account holder unable to open their own document from a device that is already logged in.

The field routes; it never authorizes. It tells a reader whether a document is one it should be able to open, and which of its keys to use; whether that key opens the wrap is decided by the AES-KW integrity check as it already is.

#### Scenario: a scoped record selects the right key

- **GIVEN** a reader holding the default-scope key and a scope `S` key, and a document whose scope field carries the scope `S` tag
- **WHEN** the reader unwraps
- **THEN** it recovers the content key from the wrap addressed to the scope `S` key

#### Scenario: an unscoped record reads as the default scope

- **GIVEN** a document written before the scope field existed
- **WHEN** a reader holding the default-scope key opens it
- **THEN** it succeeds unchanged

#### Scenario: the account holder opens a scoped document

- **GIVEN** a document written under scope `S` by a scoped client
- **WHEN** the account holder opens it with the default-scope key
- **THEN** it decrypts, because the content key is wrapped to both keys

#### Scenario: a scope the reader does not hold is distinguishable

- **GIVEN** a document whose scope tag matches no scope the reader holds
- **WHEN** the reader attempts to read it
- **THEN** the outcome names the missing scope, and is distinct from the outcome for a corrupt or tampered record

### Requirement: The scope tag participates in the wrap transcript

The scope tag SHALL be folded into the HKDF `info` transcript alongside the existing context fields, through the same injective context-transcript encoder required by `spec:document-crypto § Wraps are AEAD-bound to their record context`. The default scope SHALL contribute exactly what an unscoped wrap contributes today, so existing transcripts are byte-identical and existing wraps continue to open.

Binding the tag means altering a record's scope field changes the transcript, so the alteration is caught by the integrity check rather than merely failing to find a working key by accident. Both wraps on a scoped document — the one to the account key and the one to the scope key — bind the same tag, so the record carries one consistent statement of its scope.

#### Scenario: existing wraps derive unchanged transcripts

- **GIVEN** a wrap created before the scope field existed
- **WHEN** its `info` transcript is recomputed under the current encoder with the default scope
- **THEN** the transcript bytes are identical to those originally used

#### Scenario: an altered scope field fails closed

- **GIVEN** a document whose scope field has been changed to name a scope the reader holds
- **WHEN** the reader unwraps with that scope's key
- **THEN** the derived wrapping key is wrong and the AES-KW integrity check rejects it

### Requirement: Changing a document's scope is an explicit re-key

A document's scope SHALL be fixed when the record is written. Moving the document, renaming it, or editing its metadata SHALL NOT change its scope or its wrap set.

Changing a document's scope SHALL be an explicit operation that rewrites the record's wraps and scope field together, so the two can never disagree. Re-keying a document so that another party can read it is a disclosure decision and SHALL be surfaced as one, not performed as a side effect of an organizational action.

Removing a scope's wrap SHALL NOT be presented as revoking that scope's access. Current limitation: a holder that has already read the document retains its content key, so withdrawal governs only future reads of future versions — the same limitation grants carry under `spec:sharing-grants § Revocation stops future discovery but not historical access`.

#### Scenario: an ordinary edit preserves scope

- **GIVEN** a document under scope `S`
- **WHEN** its content or metadata is updated
- **THEN** its scope field and wrap set are unchanged

#### Scenario: re-keying rewrites field and wraps together

- **GIVEN** a document under the default scope
- **WHEN** it is explicitly re-keyed into scope `S`
- **THEN** the record's scope field and its wrap set are written in one operation, and a reader holding the scope `S` key can open it

#### Scenario: withdrawal does not reach an existing holder

- **GIVEN** a document previously readable under scope `S`, whose scope wrap has been removed
- **WHEN** a party that already recovered the content key decrypts the blob it retained
- **THEN** it still succeeds, and the withdrawal is reported as governing future versions only
