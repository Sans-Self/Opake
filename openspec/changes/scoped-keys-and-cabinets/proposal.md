## Why

An Opake identity is a single keypair. It unwraps every cabinet document and every workspace group key the account belongs to, and there is no way to hand a client less than all of it. Any client that must hold key material therefore holds the whole account — including the group keys that open collaborators' workspace documents, an exposure the account holder cannot consent to on their behalf.

That forecloses a class of client Opake otherwise supports well: one that runs in an environment the account holder does not fully control. Scoped keys make the exposure proportional. A client receives a keypair that opens one named subset of the cabinet and nothing else, derived from the same mnemonic, so the recovery model is unchanged and the user manages no additional secret.

## What Changes

- Key derivation gains a **scope** dimension. `derive_keys_from_mnemonic` maps a mnemonic plus a scope identifier to a keypair set. The unscoped derivation remains reachable unchanged as the default scope, so every existing identity derives byte-identically.
- A document **names the scope it is wrapped to**, as a record field carrying an opaque derived tag. Absent means the default scope, so every existing record reads unchanged.
- A scoped document is wrapped to **both** the account's default-scope key and the scope key, so a scoped client reads its own documents and the account holder retains access to everything in their cabinet.
- A document's scope is **fixed at write and does not change when the document moves**. Position in the cabinet tree carries no access meaning.
- Changing a document's scope is an **explicit re-key**, in the same family as sharing — never a side effect of organizing files.
- Clients **recognize documents outside every scope they hold** and report them as such, rather than failing to decrypt with no explanation.
- A scope key reaches a client through a **scoped pairing**, never by that client holding the mnemonic. The approving device derives the scope key and sends only that.
- An identity **persists its master seed** so a trusted device can mint a scope key without the account holder re-entering the phrase. The seed is key material, zeroized and redacted like the private keys beside it, and the phrase remains unrecoverable from it.
- Recovery re-derives scope keys, which requires the set of scopes in use to be **discoverable from the account's own records**.
- Live updates are **filtered by scope**, so a scoped client is not told about documents it cannot read.
- Scope identifiers are **not secret and carry no user-supplied secret component**. A scope key is derivable from the mnemonic alone, preserving the property that the seed phrase is sufficient for full recovery.

## Capabilities

### New Capabilities

- `scoped-identity`: deriving a keypair set for a named scope from the account mnemonic; scope identifier form and constraints; what a scope key does and does not open; how the scopes in use are discovered at recovery; what withdrawing a scope does and does not achieve.

### Modified Capabilities

- `auth-identity`: derivation takes a scope alongside the mnemonic. The v1 unscoped path is preserved verbatim as the default scope, and scoped derivation is expressed as new version-labelled info strings living alongside it, per the existing immutability requirement.
- `document-crypto`: a record names the scope its content key is wrapped to, and that name is bound into the wrap transcript. The wrap construction, the AEAD context binding, and the hybrid algorithm are unchanged.
- `tree-cabinet`: cabinet listing defines behaviour for documents whose scope the reader does not hold — they are surfaced and labelled, never silently omitted — and tree position is stated to carry no access meaning.
- `auth-pairing`: a scoped pairing variant delivers one scope key instead of the account identity, verified against the requested scope tag rather than the published key record. This is how a scope key reaches a client without that client ever seeing the mnemonic.
- `sharing-grants`: a grant over a scoped document carries that document's scope tag, so the recipient can reconstruct the wrap transcript. Who may be shared with is unchanged.

## Impact

**Lexicons** — `at.opake.document` gains an optional scope field. Additive; existing records remain valid and read as the default scope.

**crates/opake-crypto** — `mnemonic/derive.rs` gains a scoped derivation entry point and the scope-tag derivation. New golden vectors pin the scoped outputs alongside the existing v1 vector. `key_wrapping.rs` binds the scope tag into the HKDF transcript.

**crates/opake-core** — `Identity` and `Cabinet` carry a scope; `cabinet.rs`, `documents/upload.rs`, and the download paths read and write the record's scope. Recovery gains scope discovery. An explicit re-key path is added alongside sharing.

**Indexer** — `at.opake.document` gains a field the indexer consumes, so the new-field checklist applies. Event streams take a scope argument and filter on the tag, so a scoped subscriber receives only its own scope's events rather than the whole cabinet's.

**Clients** — CLI and web surface documents outside the holder's scopes, and mark scoped documents so an account holder can see which of their documents a scoped client can read. That marking reads the record's scope rather than inferring from location. The pairing flows gain a scoped variant that names the scope being granted at approval time.

**Not in scope** — workspace group keys remain wrapped to the account's default-scope key; scoped membership is not introduced here. Grants continue to target a DID's published public keys and are not extended to scopes. Cabinet directories remain wrapped to the default-scope key, so a scoped client reads document content but no directory names. Identity rotation is untouched, though scoped derivation and rotation both live on the derivation path and a rotation design will have to account for scopes.
