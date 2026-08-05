## Context

See proposal.md § Why for motivation.

Three properties of the current system shape the approach. Derivation already has a domain-separation layer: one PBKDF2 stage produces a master seed, and three HKDF info strings expand it into the three keypairs. Wraps already carry a recipient DID and fold a context into an injective HKDF transcript. And `spec:auth-identity § The derivation path is version-pinned and immutable` permits a new derivation scheme only as new version labels living alongside v1, never as an edit to v1.

The constraint that dominates the design is that all record-level metadata is dummy and everything meaningful is encrypted (`spec:document-crypto § All document metadata is encrypted`). Anything this change adds to a record is visible to the PDS and to the indexer, so it has to be built to leak as little as a routing hint can.

## Goals / Non-Goals

**Goals:**

- A scope key whose compromise is bounded by the documents wrapped to it.
- Byte-identical derivation and byte-identical wrap transcripts for everything that exists today.
- Deterministic key selection at unwrap, with a distinguishable "not my scope" outcome.
- Recovery that restores scoped access from the phrase alone.

**Non-Goals:**

- Scoped workspace membership. Group keys stay wrapped to the default-scope key.
- Scoped grants. Grants continue to target a DID's published public keys.
- A scoped client's write authority. A client holding a scope key still authenticates with an OAuth session whose finest granularity is the collection; narrowing that is a separate problem with no enforcer today.
- Identity rotation, though both live on the derivation path and a rotation design will inherit scopes.

## Decisions

### Scope enters the HKDF expansion, not the PBKDF2 stage

BIP-39 defines an optional passphrase that salts the PBKDF2 stage — the "25th word" — and it would work: a different passphrase yields a different master seed and therefore unrelated keys.

It is the wrong layer here. The PBKDF2 stage exists to turn a phrase into a seed, and its round count is fixed for BIP-39 interoperability rather than as a hardening parameter. Scoping is domain separation, which is what the HKDF `info` layer already does three times over with `opake-v1-x25519-identity`, `opake-v1-ed25519-signing`, and `opake-v1-mlkem768-keygen`. Putting scopes there keeps one master seed per mnemonic with cheap sibling derivation, reuses the encoder and the golden-vector discipline already in place, and avoids introducing a second separation mechanism at a different layer for one caller.

The scope identifier is encoded into the info strings through the injective context-transcript encoder rather than concatenated, for the same reason the wrap transcript requires it: identifiers may contain the separator, and a collision here is a key collision.

A parallel set of scoped info labels was considered and is not needed. One family suffices: the scope extends the three existing strings, and the default scope extends them by nothing at all, so its info bytes are exactly today's. Requiring scope identifiers to be non-empty is what keeps the bare and extended forms distinguishable, which is all injectivity needs. Two label families would have to be kept in step forever for no gain.

**Identifier form.** `opake/<consumer>/<instance>`. Scope identifiers are account-local: never resolved, never published, never compared across accounts, so they must not be drawn from a namespace whose authority a party must hold. A reverse-DNS form such as `at.opake.obsidian` would imply control of a domain, which a plugin distributed through a third-party registry cannot claim, and would falsely suggest the identifier means something outside this account. The instance segment must be reproducible on every device participating in the same store and must not come from anything the host may reassign — a value the client generates once and replicates alongside its non-secret settings is the shape that survives a rename or a re-registration.

**Tag length: 128 bits.** The tag is a keyed function of the identifier under the master seed, so guessing an identifier does not yield a tag and preimage resistance is not the binding concern. What matters is collision between the handful of scopes one account operates, where 128 bits is overwhelming, and keeping per-record overhead negligible — 16 bytes against the 1160-byte wrap envelope beside it.

### The scope identifier carries no secret

A user-supplied password folded into the derivation was considered and rejected on two grounds.

It breaks the recovery contract. Today the phrase is sufficient; adding a per-scope secret makes a scope's documents unrecoverable when that secret is lost even though the phrase survives.

And it defends nothing that matters. The threat scoping addresses is a client that holds key material in an environment the account holder does not control. The derived key is resident in that client either way — a secret in the derivation protects the act of deriving, which is not what an attacker in that position is attacking. Protecting the at-rest copy on the client is a separate wrap and remains the client's responsibility.

### The record names its scope, and position does not

Two homes were available for a document's scope: its place in the cabinet tree, or a field on the record.

Position is seductive because it makes the tree self-documenting — look at a folder, know who can read it. It fails on enforcement. Nothing server-side checks that a document inside a designated subtree is wrapped to that scope, and a scoped client holds collection-wide write authority regardless, so the correspondence is a convention maintained by well-behaved clients. An account holder reasoning about access from folder membership would be reasoning from something no party guarantees, and would be wrong exactly when it mattered. Position also made every boundary-crossing move a re-key, turning a drag of a folder into one record write per document inside it.

A field says the thing directly. It is written once, in the same operation as the wraps, so the two cannot drift; it survives moves without any rewrite; and it makes discovery a plain listing rather than a second mechanism.

The cost is that the tree no longer tells an account holder who can read what, so clients carry that obligation instead — marking scoped documents from the record's own field wherever access matters to the reader. That is the honest version: the marking reads the actual wrap set rather than inferring from a folder.

### A document's scope is fixed at write

Scope transitions were designed for before it was clear any were needed. Enumerating them: a document created by a scoped client keeps that scope for life, which is the overwhelming majority; adding an existing document to a scope is real but rare and is a deliberate disclosure; and removing one from a scope cannot achieve what it appears to, because the holder already has the content key.

So scope is assigned at write and does not change when a document moves, is renamed, or is edited. Changing it is an explicit re-key that rewrites field and wraps together, presented as the disclosure decision it is rather than buried in an organizational gesture. Withdrawal is rotation under a new identifier, priced honestly as bulk work, and never described as revocation — the same limitation grants already carry.

### The scope tag is opaque, not the scope name

The recipient DID no longer identifies which keypair a wrap belongs to, because every scope of one account shares one DID. Something legible before decryption has to say which scope a record belongs to.

Inferring it by attempting each held key was considered and rejected — not for cost, since a scoped record carries at most two wraps, but because a failure to open would be indistinguishable from a corrupt record. `spec:tree-cabinet § Documents outside the reader's scopes are surfaced, not omitted` turns on exactly that distinction: a reader must be able to say "this belongs to a scope I do not hold" rather than "this did not decrypt".

Carrying the human-readable scope identifier was considered and rejected for a different reason: it would put a meaningful plaintext string on the record, which is what the all-metadata-encrypted posture exists to prevent, and it would tell the PDS and the indexer which client authored which document.

The field therefore carries an opaque tag derived from the master seed and the scope identifier. A holder of the seed can compute the tag for any scope it knows; a scope-only client is given its tag alongside its key. The PDS sees a value it cannot invert to a name and cannot correlate across accounts.

The tag states; it never authorizes. It tells a reader which of its keys to try and whether it should expect success, and the AES-KW integrity check still decides the outcome — so a forged or altered tag produces a decryption failure rather than a disclosure.

### A scoped document is wrapped twice

Wrapping only to the scope key looks sufficient, since every scope key derives from the account mnemonic and the account holder therefore has one. It is not, for two reasons that compound.

A logged-in client does not hold the phrase. Per `spec:auth-identity § The mnemonic zeroizes and never leaks through debug output`, only the derived keys persist, so an established session cannot derive a key it was not created with. Reading a scoped document would mean re-entering the seed phrase, which is not an acceptable routine operation.

And a tag cannot be inverted to the identifier that produced it — that opacity is what keeps the scope name off the record. Discovery therefore yields tags, while derivation needs identifiers, so an owner cannot get from what they can enumerate to a key. Closing that gap would require durably recording the identifiers somewhere, which reintroduces the registry rejected below, now in a position where drift means unreadable documents rather than an untidy list.

The second wrap removes the whole chain: the account holder never derives a scope key, never needs the identifier, and reads their cabinet with the key they already hold. The tag stays purely informational — it tells a scoped client which documents are its business and tells the owner's interface which to mark.

The cost is one extra hybrid envelope per scoped record, 1160 bytes by the sizing recorded in `lexicons/at.opake.defs.json`. That is real at scale and is accepted deliberately: the alternative trades it for a registry whose failure mode is an account holder unable to read their own documents.

### In-use scopes are enumerated from the account's own document records, not a registry

A registry record (`at.opake.scopes/self`, say) would make enumeration one fetch, but it is state that can drift from reality: a scope present in the registry with no documents, or documents whose scope never made it into the registry, both present as inconsistency the user cannot act on.

Enumerating tags from the account's own document records cannot drift, because the records are the thing being described. It costs a listing pass, which recovery already performs, and it means an unused scope is simply invisible — the behaviour `spec:scoped-identity § The scopes in use are discoverable from the account's records` specifies.

This yields tags, not names. A holder of the seed can recover a name by computing tags for the identifiers it knows and matching; a name for which nothing is known cannot be recovered from the tag, which is the point of the tag being opaque. Recovery therefore reports an unmatched tag as an unrecovered scope rather than silently completing.

### The seed persists so pairing can mint a scope key

A scope key has to reach the client somehow, and the client must never see the mnemonic — handing over the phrase would give it every scope and the account itself, defeating the point.

That leaves an already-trusted device deriving the key and transferring it, which is what pairing is for. But deriving a scope key needs the master seed, and today only the derived keys persist, so a logged-in device could not mint one without the account holder re-entering the phrase at every provisioning.

Persisting the seed removes the re-entry and adds nothing a compromised device did not already have: the default-scope key opens every document in the cabinet, scoped ones included, and the seed grants no read access beyond that. The phrase is not recoverable from it, so a stolen device still cannot yield the words.

The scoped pair response cannot be checked against `publicKey/self` the way a full one is, because scope public keys are deliberately never published — publishing them would disclose which scopes an account runs and undo the tag's opacity. Verification is against the requested tag instead, which detects a substituted or misdirected scope but does not prove the responder held the seed. That is a weaker check, stated as such in the spec, and it puts more weight on the ephemeral fingerprint confirmation than the full flow does.

### Live updates are filtered by scope

A scoped subscriber that received the whole cabinet's events would learn about documents it cannot read — the tag on every record, timing, and volume — which is exactly the leakage the tag's opacity is meant to limit. Filtering server-side keeps a scoped client's view of the account as narrow as its key.

The indexer already sees the tag on records it consumes, so filtering by it introduces no new disclosure to the indexer. It does mean the event stream takes a scope argument and that the document-field checklist applies.

### Directory records stay in the default scope

What a directory protects is narrower than it first appears. Its listing entries — the AT-URI and pinned CID of each child — are record-level fields, so the shape of the tree is legible to anyone who can read the repository. Only the directory's name lives in encrypted metadata, under a per-directory content key wrapped to the account's key.

So the question is not whether a scope-only client can see the tree; it already can, and it can append to it, which is what lets it write new documents into a directory without holding any directory key. The question is only whether it learns folder names.

Extending scoping to directories is structurally cheap — each directory already has its own content key, and the direct wrapping envelope already holds a vector of wraps, so adding a second recipient to a chosen directory needs no new machinery. It is deferred here for maintenance reasons rather than structural ones: every create and every move would have to decide which scopes receive a wrap, a document moved into a directory not wrapped to its scope silently loses its name for that client, and withdrawing a scope would mean re-keying directories and re-encrypting their metadata. Those are invariants worth designing deliberately rather than inheriting from a first pass.

The consequence for a scope-only client is a hierarchy with unnamed folders, not a flat bag of documents. A client that wants meaningful names in the interim can carry its own path convention inside its documents' encrypted metadata, which is sealed under the content key it already holds; defining such a convention is the consuming client's concern, not this change's.

## Risks / Trade-offs

**The scope tag reveals grouping.** The PDS and the indexer learn which documents share a scope, even without learning the scope's name. → Unavoidable for any scheme that routes without trial decryption. The leak is bounded: it is intra-account grouping, on records the PDS already knows belong to one account, and the tag is not correlatable across accounts because it derives from the account's master seed.

**A scope key is only as bounded as what gets wrapped to it.** Nothing prevents an account holder from wrapping the whole cabinet to a scope, which would make the scope key equivalent to the default one. → The capability provides the mechanism; the bounding is a property of use. Client-facing surfaces should make the scope a document is written under explicit rather than incidental.

**Derivation changes are unforgiving.** An error in the scoped info-string encoding orphans every document written under it, and the failure looks like corruption. → Golden vectors for scoped outputs alongside the existing v1 vector, and the byte-identity of the default scope asserted as a test rather than assumed.

**A scope-only client cannot tell a scope gap from a damaged record without the tag.** If a tag is absent from a wrap that is in fact scoped, the reader treats it as default-scope and fails. → Absent means default by definition, and the default scope is the only unlabelled one; a scoped writer that omits the tag is writing an unreadable record, which is a writer-side invariant worth asserting at the point of write.

**Two clients could adopt the same scope identifier.** Colliding on a name means sharing a key, silently. → The identifier form requires consumer namespacing and admits a per-instance component; the risk is documented rather than mechanically prevented, since nothing on the account can see another client's chosen name.

## Migration Plan

No data migration. Every existing wrap is unlabelled and reads as the default scope, and the default scope's derivation and transcript are byte-identical to today's, so an account that never uses a scope is indistinguishable from one on the current code.

Rollback is likewise clean while no scoped documents exist. Once documents have been written under a scope, reverting leaves them unreadable by the reverted client — they are not damaged, but nothing in the older code can route to their key. The point of no return is the first scoped write, not the deployment.
