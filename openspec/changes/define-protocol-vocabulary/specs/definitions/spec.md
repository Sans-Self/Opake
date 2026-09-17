# definitions (delta)

## ADDED Requirements

### Requirement: workspace

A spec MUST use `workspace` to mean:

The shared tree of documents and directories a group of members holds in common, encrypted under a group key. It is identified for its whole lifetime by the genesis URI of its keyring chain.

> Note: the workspace is the domain object; the keyring is the record that carries it. A spec does not call the workspace a keyring.

#### Scenario: In a sentence

- **WHEN** a member resolves a workspace whose keyring has been superseded
- **THEN** its identity is the genesis URI, not the head URI


### Requirement: member

A spec MUST use `member` to mean:

An account listed in the current keyring head with a role of manager, editor or viewer. A member stays a member until their entry leaves that list.

#### Scenario: In a sentence

- **WHEN** a workspace's creator, demoted to editor, attempts a membership mutation
- **THEN** it is rejected exactly as any editor's would be


### Requirement: membership

A spec MUST use `membership` to mean:

The relationship between an account and a workspace, read from the current keyring head. It is present with a role, absent, or unknown while the workspace is not yet indexed.

- **Admitted:** membership state

#### Scenario: In a sentence

- **WHEN** the indexer resolves a member's role for an authority check
- **THEN** the role comes from the current head's member list


### Requirement: owner

A spec MUST use `owner` to mean:

The account that holds a record in its own repository and decides who else may read it. In sharing it is the account holding the document and writing the grant.

- **Admitted:** sharer
- **Deprecated:** workspace owner

#### Scenario: In a sentence

- **WHEN** the owner shares a cabinet document
- **THEN** one grant record is created on the owner's PDS with the content key wrapped to the recipient


### Requirement: counterparty

A spec MUST use `counterparty` to mean:

The other account in an operation that crosses account boundaries, whether it is resolving keys or having its keys resolved. It is an account, not the person or client acting locally.

- **Admitted:** recipient, consumer
- **Deprecated:** the other party

#### Scenario: In a sentence

- **WHEN** a counterparty resolves the keys of an account carrying no `#opake` verification method
- **THEN** resolution succeeds and reports the account as unverified


### Requirement: PDS operator

A spec MUST use `PDS operator` to mean:

The party running the PDS that serves an account's records. It can withhold or alter what it serves for its own user, and cannot reach past that user into another account's operation.

- **Admitted:** host

> Note: "hostile" is a reporting label a client may attach to a PDS operator, under the constraints account-verification places on it. It is not an operator state.

#### Scenario: In a sentence

- **WHEN** a member's PDS operator serves a record that does not verify and a manager removes someone else
- **THEN** the removal completes, and that member is excluded from the new wrap and reported


### Requirement: caller

A spec MUST use `caller` to mean:

The code that invoked the operation a requirement governs and receives its result. An operation may run with no caller present, and then nothing answers a decision on its behalf.

#### Scenario: In a sentence

- **WHEN** a caller passes no group keys to the PDS-only download layer
- **THEN** it returns an explicit missing-group-keys error and performs no second fetch of the keyring record


### Requirement: authority

A spec MUST use `authority` to mean:

The permission to write a record on a path, held through the author's role in the current keyring head and enforced by the indexer at write time.

> Note: the DID segment of an at-URI is also called its authority. A spec says `authority DID` for that.

#### Scenario: In a sentence

- **WHEN** an editor's unattended runner considers repairing a missing wrap
- **THEN** it does not author a keyring supersede, because repair requires manager authority


### Requirement: keyring

A spec MUST use `keyring` to mean:

The chained record kind that carries a workspace's member list and each member's wrap of the group key. Its head is the workspace's live membership state.

- **Admitted:** keyring chain, keyring head

#### Scenario: In a sentence

- **WHEN** a member added after genesis downloads a document uploaded before they joined
- **THEN** access is granted via their unwrapped group key


### Requirement: chain

A spec MUST use `chain` to mean:

The ordered set of records that carry one object's state over time. It begins at genesis, and every later record names its predecessor in `supersedes`.

- **Admitted:** supersede chain, keyring chain, directory chain, root chain

#### Scenario: In a sentence

- **WHEN** member B supersedes a directory whose canonical record lives on member A's PDS
- **THEN** B's record becomes the canonical for that path, and the chain now spans both PDSes


### Requirement: head

A spec MUST use `head` to mean:

The record in a chain that no other record supersedes. It is where live state is read; every predecessor is history.

- **Admitted:** chain head, keyring head, current head, live chain head, canonical

#### Scenario: In a sentence

- **WHEN** the indexer resolves a member's role for an authority check
- **THEN** the role comes from the current head's member list


### Requirement: genesis

A spec MUST use `genesis` to mean:

The first record in a chain, the one carrying neither `supersedes` nor `lineage`. Its URI is the chain's permanent identity, and for a keyring chain that identity is the workspace.

- **Admitted:** genesis record, genesis URI, workspace id

#### Scenario: In a sentence

- **WHEN** any member resolves a workspace whose keyring has been superseded at least once
- **THEN** `Workspace.uri` equals the genesis URI, not the head URI


### Requirement: supersede

A spec MUST use `supersede` to mean:

To advance a chain by writing a record that names the current head in `supersedes`, and the record so written. The predecessor stays on its PDS as history.

- **Admitted:** superseding record, chain advance
- **Deprecated:** prior version

#### Scenario: In a sentence

- **WHEN** a manager re-roles another member to manager
- **THEN** the written supersede holds both members with the target's role changed and the author's untouched


### Requirement: lineage

A spec MUST use `lineage` to mean:

The field carrying the chain's genesis URI on every record after genesis. A record's lineage anchor is `lineage.unwrap_or(own URI)`, which resolves to the same value for every record in the chain.

- **Admitted:** lineage anchor, anchor

#### Scenario: In a sentence

- **WHEN** a component derives an object's identity from any record in a supersede chain
- **THEN** the result is `lineage.unwrap_or(own URI)` and equals the genesis URI


### Requirement: directory

A spec MUST use `directory` to mean:

The record kind that gives a tree its shape. It lists the documents and directories beneath it, keeps its own name in encrypted metadata, and holds no content of its own.

- **Admitted:** folder

> Note: a directory's listing is the contents of its `entries` field, not the record.

#### Scenario: In a sentence

- **WHEN** the PDS or any unauthorized reader inspects a directory record
- **THEN** it sees target and CID pairs and an encrypted metadata blob, and no entry name or type


### Requirement: cabinet

A spec MUST use `cabinet` to mean:

The personal tree an identity holds in its own repo, outside any workspace, rooted at a fixed directory record. One cabinet exists per identity, and its content keys are wrapped to the owner alone.

- **Admitted:** personal tree

#### Scenario: In a sentence

- **WHEN** a client builds the cabinet tree
- **THEN** the root is the owner's `self` directory record, constructed rather than discovered


### Requirement: document

A spec MUST use `document` to mean:

The record kind that holds one piece of stored content. The ciphertext lives in a PDS blob; the record carries the wrapped content key and the encrypted metadata a reader needs to open it.

- **Deprecated:** file

> Note: a DID document is W3C vocabulary and always carries its qualifier.

#### Scenario: In a sentence

- **WHEN** two documents are uploaded to the same workspace under the same group key
- **THEN** each carries its own content key, so no nonce is ever reused against a shared key


### Requirement: grant

A spec MUST use `grant` to mean:

An `at.opake.grant` record on the owner's PDS giving one recipient the content key of one cabinet document. Deleting it stops future discovery, not access already obtained.

- **Admitted:** share

> Note: the OAuth authorization for an identity operation is an `identity grant` and always carries its qualifier.

#### Scenario: In a sentence

- **WHEN** the owner revokes a grant
- **THEN** the grant record is deleted and the entry drops from the recipient's inbox


### Requirement: wrap

A spec MUST use `wrap` to mean:

A key encrypted under another key so one named holder can recover it. An asymmetric wrap targets a recipient's published encryption keys, a symmetric wrap targets the workspace's group key, and both fold their record context into the key derivation.

- **Admitted:** re-wrap, self-wrap

#### Scenario: In a sentence

- **WHEN** a content key wrapped under one document's URI is unwrapped while a different document URI is claimed as context
- **THEN** the derived wrapping key differs and the unwrap fails


### Requirement: content key

A spec MUST use `content key` to mean:

The AES-256 key that seals one document's blob and its encrypted metadata. Every document gets a fresh one from injected randomness, and no second document shares it.

- **Admitted:** per-document key

#### Scenario: In a sentence

- **WHEN** two documents are uploaded to the same workspace under the same group key
- **THEN** each carries its own content key, so no nonce is ever reused against a shared key


### Requirement: key material

A spec MUST use `key material` to mean:

The plaintext bytes of a secret key a process holds in memory, whether a content key, a group key, a private encryption or signing key, or a DPoP key. Handling rules attach to the bytes, not to the key's role.

#### Scenario: In a sentence

- **WHEN** a type that holds plaintext key material is dropped
- **THEN** it zeroizes


### Requirement: encrypted metadata

A spec MUST use `encrypted metadata` to mean:

The AES-256-GCM ciphertext carrying a record's descriptive fields, sealed under that record's content key and stored in the record beside its own nonce. Names, MIME types, sizes, tags, and descriptions travel only there.

- **Deprecated:** metadata blob

#### Scenario: In a sentence

- **WHEN** an upload of `report.pdf` is sent to the PDS
- **THEN** the record has no `name`, `mimeType`, `size`, or `tags` fields, and `encryptedMetadata` carries the ciphertext plus its own nonce


### Requirement: rotation

A spec MUST use `rotation` to mean:

Minting a new group key for a workspace and writing the keyring supersede that carries it, wrapped to every eligible remaining member. The word also numbers the generations that event produces, so a document names the rotation its content key was wrapped under and earlier generations live in the keyring's key history.

- **Admitted:** rotation event, generation

#### Scenario: In a sentence

- **WHEN** a manager removes a member and a document is later uploaded under the new rotation
- **THEN** the removed member holds no wrap for that rotation and cannot decrypt it


### Requirement: forward secrecy

A spec MUST use `forward secrecy` to mean:

A removed member cannot read content encrypted under a group-key generation they hold no wrap for. It retracts nothing they could already read.

#### Scenario: In a sentence

- **WHEN** a manager removes a member and no daemon or open tab ever performs follow-up work
- **THEN** the removed member cannot unwrap content encrypted under the new rotation


### Requirement: PLC rotation key

A spec MUST use `PLC rotation key` to mean:

A key in a `did:plc` identity's rotation set. The directory accepts an operation on that identity signed by any holder of one, without authenticating the submitter, so an account holding its own can publish its verification method with no other party's participation.

#### Scenario: In a sentence

- **WHEN** the holder of an account's PLC rotation key refuses the identity operation that would publish its verification method
- **THEN** the refusal is reported to the owner rather than retried silently


### Requirement: identity

A spec MUST use `identity` to mean:

The account's own keypair bundle, derived deterministically from its 24-word mnemonic: an X25519 keypair, an Ed25519 signing keypair, and an ML-KEM-768 keypair. One identity belongs to one account and is carried between that account's devices by recovery or pairing.

- **Admitted:** identity keys

> Note: `workspace identity` is a chain's genesis URI and `identity operation` is authority over a DID document. Both always carry their qualifier.

#### Scenario: In a sentence

- **WHEN** two devices derive an identity from the same 24-word mnemonic
- **THEN** the X25519, Ed25519, and ML-KEM-768 keypairs are byte-identical


### Requirement: signing key

A spec MUST use `signing key` to mean:

The Ed25519 key of an account's identity, which signs the account's published key record and authenticates its requests to the indexer. It derives from the seed phrase, so any device holding the phrase reproduces it.

- **Admitted:** Ed25519 signing key

> Note: the `atproto signing key` in a DID document and the `PLC rotation key` are different keys and always carry their qualifier.

#### Scenario: In a sentence

- **WHEN** the DID document's `#opake` key and the record's signing key disagree
- **THEN** the record does not verify


### Requirement: published key record

A spec MUST use `published key record` to mean:

The singleton record an account writes to its own PDS carrying the public halves of its identity, and, once the account is verified, a signature over them and the algorithm that produced it. Counterparties resolve it to obtain an account's wrap targets.

- **Admitted:** published record
- **Deprecated:** publicKey self-record, published public-key record, public key record

#### Scenario: In a sentence

- **WHEN** the login flow completes
- **THEN** the published key record carries the identity's X25519 and ML-KEM-768 public keys


### Requirement: verification method

A spec MUST use `verification method` to mean:

The entry an account publishes in its DID document under the fragment `#opake`, carrying the same Ed25519 signing key the account publishes in its key record. It vouches for that record rather than containing it, and the DID method serves it, not the account's PDS.

- **Admitted:** `#opake` verification method
- **Deprecated:** anchored account, DID key

> Note: the verification method is not an anchor. `anchor` belongs to `lineage`.

#### Scenario: In a sentence

- **WHEN** an account has published no `#opake` verification method
- **THEN** it remains able to publish keys, share, join workspaces, and authenticate


### Requirement: verification state

A spec MUST use `verification state` to mean:

The three-valued result of resolving an account's published key record against its DID document. It is verified when the document carries an `#opake` verification method and the record's signature verifies against it, unverified when the document carries no such method, and the error state when the method is present but the signature is absent, malformed, or does not verify.

- **Admitted:** resolution result, verified, unverified, error state
- **Deprecated:** unverifiable account, unverifiable record

> Note: an unverifiable chain link is a different thing, owned by lineage and tree-chains.

#### Scenario: In a sentence

- **WHEN** a manager adds a member
- **THEN** the manager resolves the recipient's verification state before wrapping


### Requirement: key-bound approval

A spec MUST use `key-bound approval` to mean:

A manager's or owner's recorded decision to wrap a key to one unverified account's exact encryption keys. It lives in the relationship's own current record and covers no other keys.

- **Admitted:** approval

#### Scenario: In a sentence

- **WHEN** an authorized device resolves an approved member's same keys during rotation or repair
- **THEN** it uses the recorded approval without another prompt


### Requirement: explicit confirmation

A spec MUST use `explicit confirmation` to mean:

A decision a person gives at the moment an operation would wrap a key to an unverified account. No default, prior verified resolution, historical wrap, or pending record supplies it.

- **Admitted:** confirmation, fresh explicit decision

#### Scenario: In a sentence

- **WHEN** an operation would wrap a key to a counterparty whose keys resolve as unverified
- **THEN** it surfaces the unverified state and writes nothing until the caller confirms


### Requirement: transcript

A spec MUST use `transcript` to mean:

The byte string the shared context-transcript encoder produces: a fixed ASCII label followed by each covered field prefixed with its length as a 32-bit little-endian integer. The encoding is injective, and the same transcript construction feeds key-derivation `info`, ciphertext AAD, record signatures, and key-bound approval commitments.

- **Admitted:** signed transcript, signature transcript, `info` transcript, context transcript

#### Scenario: In a sentence

- **WHEN** two wraps in different record contexts each derive a wrapping key
- **THEN** the two transcripts differ and the derived wrapping keys differ


### Requirement: context label

A spec MUST use `context label` to mean:

The fixed ASCII label that opens a context transcript and identifies its consumer, such as `at.opake.publicKey/self:v<n>`. Each consumer has its own label, and the label carries the declared `opakeVersion` that pins the consumer's covered-field list.

#### Scenario: In a sentence

- **WHEN** a client builds the signed transcript for a published key record
- **THEN** the context label carries the record's declared `opakeVersion`, so the set of fields the signature covers is fixed by that declaration


### Requirement: indexer

A spec MUST use `indexer` to mean:

The service that consumes the atproto firehose into queryable rows and answers a client's snapshots, chain-head lookups and membership checks. It authorizes record access and sees only ciphertext.

#### Scenario: In a sentence

- **WHEN** the firehose delivers an `at.opake.*` record that fails structural validation
- **THEN** the indexer does not index it, and subsequent snapshots and streams do not contain it


### Requirement: background runner

A spec MUST use `background runner` to mean:

A process that executes a workspace's derivable maintenance work with no person present, acting under its own account's authority and never a separate repair privilege. The CLI daemon is a committed runner and the web client an opportunistic one.

- **Admitted:** runner, unattended runner

#### Scenario: In a sentence

- **WHEN** a user's CLI daemon and an open web tab run the same maintenance task at the same time
- **THEN** every item is fixed exactly once, and the final state equals a single-runner run


### Requirement: tombstone

A spec MUST use `tombstone` to mean:

The indexer row that marks a record deleted from its PDS. It survives for a bounded period and is then purged, so a chain's deleted links are not archived.

#### Scenario: In a sentence

- **WHEN** a keyring delete tombstone arrives for a workspace whose chain still has a live record
- **THEN** the workspace is not dropped, because the tombstone is record cleanup and not destruction


### Requirement: delete outcome

A spec MUST use `delete outcome` to mean:

The single classification the indexer resolves from chain state for every keyring record delete and carries in the delete event, telling a client what the delete did to the chain. Its values are `unchanged`, `rolled_back` and `torn_down`.

- **Admitted:** outcome

#### Scenario: In a sentence

- **WHEN** a member's client receives a keyring delete carrying outcome `torn_down`
- **THEN** the keeper entry keyed by the payload's `workspace_id` is removed


### Requirement: visibility gap

A spec MUST use `visibility gap` to mean:

The interval between a PDS accepting a write and the indexer being able to answer for it. No bound on that interval exists, and a workspace-scoped read inside it answers `workspace_not_indexed`.

- **Admitted:** acceptance-to-visibility window

#### Scenario: In a sentence

- **WHEN** a client writes a record to its PDS and immediately requests an indexer snapshot
- **THEN** a snapshot that does not yet contain the record is a conforming response, not an error


### Requirement: version skew

A spec MUST use `version skew` to mean:

The condition in which a client and the record or service it reads were built against different protocol versions. It is reported with its own reason, never as an attack or as corruption.

#### Scenario: In a sentence

- **WHEN** a client ahead of its indexer deserializes a bare delete payload
- **THEN** the outcome defaults to `unchanged` and no tracked state is dropped


## MODIFIED Requirements

### Requirement: group key

A spec MUST use `group key` to mean:

The symmetric key that wraps a workspace's content keys for the current
rotation. Every member holds a wrap of it; rotation replaces it.

- **Deprecated:** workspace key

> Note: a rotation key is a `PLC rotation key`, never the group key.

#### Scenario: In a sentence

- **WHEN** a member is removed
- **THEN** the group key rotates
