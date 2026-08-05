## 1. Scoped derivation

- [ ] 1.1 Add the scope identifier type to opake-crypto with its character-set and length constraints, rejecting anything outside them at parse time
- [ ] 1.2 Append the scope identifier to the three existing HKDF info strings through the injective context-transcript encoder, with the default scope appending nothing so its info strings stay byte-identical
- [ ] 1.3 Add the scoped derivation entry point to `crates/opake-crypto/src/mnemonic/derive.rs`, sharing one master-seed computation with the unscoped path
- [ ] 1.4 Add golden vectors for the scoped outputs, and assert that an unscoped derivation still matches the existing v1 golden bytes exactly
- [ ] 1.5 Add a test that two scope identifiers whose naive concatenation would collide derive different key material
- [ ] 1.6 Derive the opaque scope tag from the master seed and the scope identifier, with a test that the same identifier yields the same tag and different identifiers do not

## 2. Record field and wrap binding

- [ ] 2.1 Add the optional scope field to `at.opake.document` in the lexicon, sized to the tag's fixed length
- [ ] 2.2 Fold the scope tag into the HKDF `info` transcript, with the default scope contributing exactly what an unscoped wrap contributes today
- [ ] 2.3 Assert byte-identity of the transcript for an existing unscoped record under the new encoder
- [ ] 2.4 Wrap a scoped document's content key to both the account's default-scope key and the scope key in one write
- [ ] 2.5 Select the key to use from the record's scope field, and add a distinct error for a scope the reader does not hold, separate from a decryption or integrity failure
- [ ] 2.6 Add a test that altering a record's scope field fails the integrity check rather than opening under the named scope's key

## 3. Identity and cabinet plumbing

- [ ] 3.1 Carry the scope on `Identity` and `Cabinet` in opake-core, with the absent case meaning the default scope
- [ ] 3.2 Write the caller's scope onto the record at document upload
- [ ] 3.3 Thread key selection through the owner download path and the PDS-only download path
- [ ] 3.4 Hold several scope keys at once in the client key material, so one reader can open documents across scopes it holds
- [ ] 3.5 Assert that a cabinet move leaves the record's scope field and wrap set untouched

## 4. Explicit re-key

- [ ] 4.1 Add a re-key operation that rewrites a document's scope field and wrap set in one write, so the two cannot disagree
- [ ] 4.2 Refuse a re-key that would leave the record's declared scope and its wraps inconsistent
- [ ] 4.3 Surface re-keying into a scope as a disclosure in CLI and web, distinct from moving or renaming
- [ ] 4.4 Add scope rotation: re-key every document under one scope identifier to a new one, reporting progress and partial completion

## 5. Provisioning

- [ ] 5.1 Persist the master seed on `Identity`, zeroized and redacted on the same terms as the private keys
- [ ] 5.2 Derive a scope key from the persisted seed without requiring the mnemonic, with a test that no phrase is needed
- [ ] 5.3 Add the scoped pairing request and response variants, carrying the scope identifier and tag
- [ ] 5.4 Verify a scoped response against the requested scope tag rather than `publicKey/self`, and reject a tag other than the one requested
- [ ] 5.5 Display the scope being granted at approval time in CLI and web
- [ ] 5.6 Assert that a scoped pair response contains no default-scope private key

## 6. Grants over scoped documents

- [ ] 6.1 Carry the document's scope tag on the grant record and bind it into the wrap transcript
- [ ] 6.2 Assert that a grant over an unscoped document is byte-unchanged
- [ ] 6.3 Assert that stripping the scope tag from a grant fails the integrity check

## 7. Indexer and live updates

- [ ] 7.1 Index the document scope field, following the new-field checklist
- [ ] 7.2 Take a scope argument on event stream subscription and filter events by tag
- [ ] 7.3 Assert that a scoped subscriber receives no events for documents outside its scope

## 8. Scope discovery and recovery

- [ ] 8.1 Enumerate in-use scope tags from the account's own document records
- [ ] 8.2 Match enumerated tags against identifiers the recovering party knows, and re-derive a key for each match
- [ ] 8.3 Report an enumerated tag with no matching identifier as an unrecovered scope, and fail to present the recovery as complete
- [ ] 8.4 Extend CLI and web recovery to run scope discovery after the default-scope check

## 9. Listing behaviour

- [ ] 9.1 Mark documents whose scope the reader does not hold in cabinet listings, without omitting them and without failing the listing
- [ ] 9.2 Distinguish that marking from a damaged or undecryptable record in both CLI and web output
- [ ] 9.3 Mark scoped documents for the account holder from the record's scope field, wherever access matters to the reader
- [ ] 9.4 Assert that directory records remain wrapped under the `Cabinet` context and do not open with a scope key

## 10. Verification

- [ ] 10.1 Add an end-to-end test that a scope key opens its own documents and fails on default-scope documents and on a workspace group key
- [ ] 10.2 Add a test that an account created before this change reads and writes unchanged, with no scope field anywhere in its records
- [ ] 10.3 Add a test that a scoped write which omits the scope wrap is refused at the point of write rather than producing a record its declared scope cannot open
- [ ] 10.4 Run `just spec-lint` and reconcile every citation added by this change
- [ ] 10.5 Update docs/CRYPTO.md with the scoped derivation path and the tag construction, and docs/ARCHITECTURE.md with what a scope key does and does not open, including that withdrawal is rotation and does not reach a prior holder
