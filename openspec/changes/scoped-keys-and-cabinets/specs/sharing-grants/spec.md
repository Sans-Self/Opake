## ADDED Requirements

### Requirement: A grant over a scoped document carries that document's scope

A grant SHALL carry the scope tag of the document it shares, and its wrap SHALL bind that tag into the same transcript position as the document's own wraps, per `spec:document-crypto § The scope tag participates in the wrap transcript`. An absent tag SHALL mean the default scope, so every grant written before scopes existed reads unchanged.

The recipient reconstructs the transcript from the grant record in order to unwrap. Without the tag on the grant, a recipient cannot reproduce the transcript for a scoped document and the unwrap fails — the grant would be unopenable for reasons the recipient could not diagnose.

Sharing SHALL remain independent of scope in every other respect: a grant continues to wrap the document's content key to the recipient's published public keys, and the recipient needs no scope key of the sharer's. Scope governs which of the *owner's* keys open a document; it does not restrict who the owner may share that document with.

#### Scenario: a scoped document shares successfully

- **GIVEN** a document under scope `S` and a recipient with a published public-key record
- **WHEN** the owner grants access
- **THEN** the grant carries the scope `S` tag, and the recipient unwraps the content key using only their own private keys

#### Scenario: a grant over an unscoped document is unchanged

- **GIVEN** a document with no scope
- **WHEN** it is shared
- **THEN** the grant carries no scope tag and unwraps exactly as grants did before scopes existed

#### Scenario: a stripped scope tag fails closed

- **GIVEN** a grant over a scoped document whose scope tag has been removed
- **WHEN** the recipient attempts to unwrap
- **THEN** the reconstructed transcript differs, the AES-KW integrity check rejects it, and no content key is recovered
