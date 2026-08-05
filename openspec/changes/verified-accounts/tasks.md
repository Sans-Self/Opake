## 1. Lexicon and record model

- [ ] 1.1 Add the optional `signature` field to `lexicons/at.opake.publicKey.json` with a byte-length bound, documenting that a client which ignores it reads identical key material
- [ ] 1.2 Add the field to the record model in `crates/opake-core/src/records/public_key.rs` as an optional value, leaving parsing of existing records unchanged
- [ ] 1.3 Add a regression asserting an existing unsigned record still parses and yields identical key bytes

## 2. Signature construction

- [ ] 2.1 Add a context label for the published key record, carrying the record type and the verification scheme version, alongside the existing wrap-context labels
- [ ] 2.2 Build the signed transcript from the context label, the account DID, and the record's field values excluding `signature`, using the shared context-transcript encoder
- [ ] 2.3 Implement signing with the identity's Ed25519 key, and verification against a supplied Ed25519 public key
- [ ] 2.4 Add a golden vector pinning the transcript bytes so an accidental change to field order or labelling fails a test
- [ ] 2.5 Add a regression asserting a signature over one account's record does not verify when presented under another account's DID
- [ ] 2.6 Add a regression asserting a record whose byte encoding changed without a field value changing still verifies

## 3. DID document verification methods

- [ ] 3.1 Extend the DID document model to expose verification methods by fragment, for both `did:plc` and `did:web` resolution paths
- [ ] 3.2 Decode a verification method's multibase key value into an Ed25519 public key, rejecting other key types with a distinct error
- [ ] 3.3 Add a lookup returning the account's `#opake` key, absent, or malformed, as three distinguishable outcomes

## 4. Three-valued resolution

- [ ] 4.1 Extend identity resolution in `crates/opake-core/src/resolve.rs` to return the verification state alongside the key bundle
- [ ] 4.2 Return the error state when a verification method is present and the record's signature is absent or does not verify, distinct from every existing resolution error
- [ ] 4.3 Return the unverified state when no verification method is present, preserving the existing `RecipientNotReady` and wrong-algorithm outcomes unchanged
- [ ] 4.4 Add regressions for each of the three outcomes, including a stripped-signature case asserting refusal rather than downgrade

## 5. Publication

- [ ] 5.1 Sign the record in `publish_public_key`, writing the signed record on login, recovery, and share healing
- [ ] 5.2 Add the operation that publishes the `#opake` verification method, ordered strictly after the signed record exists
- [ ] 5.3 Add the operation that removes the verification method, returning the account to unverified
- [ ] 5.4 Add a regression asserting publication order, so a verified account never obliges a check it cannot yet satisfy

## 6. Callers that wrap to another account

- [ ] 6.1 Resolve verification state in workspace member addition, refusing the error state before any wrap is computed
- [ ] 6.2 Resolve verification state in grant creation, refusing the error state before any grant record is written
- [ ] 6.3 Thread an explicit confirmation through both paths for the unverified state, so no wrap is written without it
- [ ] 6.4 Verify received keys against the DID document in pairing completion, rejecting the error state
- [ ] 6.5 Add regressions asserting no record is written on the error state in each of the three paths

## 7. Self-check and repair

- [ ] 7.1 Check the account's own DID document for its `#opake` verification method at boot
- [ ] 7.2 Report absent and mismatched distinctly, offering republication only for the absent case
- [ ] 7.3 Add a regression asserting a verification method holding a foreign key is reported as substitution and not silently repaired

## 8. Indexer

- [ ] 8.1 Accept an authenticating account that publishes no verification method, unchanged from today
- [ ] 8.2 Refuse an authenticating account whose verification method is present and whose published record does not verify under it
- [ ] 8.3 Ensure the key cache cannot serve a stale decision across a change in verification state
- [ ] 8.4 Add controller tests for the accept and refuse paths

## 9. Clients

- [ ] 9.1 Surface verification state wherever a counterparty is named, with an accessible non-colour indicator
- [ ] 9.2 Present the unverified confirmation at the point of wrapping, naming the consequence rather than the mechanism
- [ ] 9.3 Present the error state as a refusal with no override
- [ ] 9.4 Add the verification setup and removal flows to the CLI
- [ ] 9.5 Add the self-check result to the web account view

## 10. Documentation

- [ ] 10.1 Document the verification method, the signed transcript, and the three states in `docs/CRYPTO.md`
- [ ] 10.2 Document the setup and repair flows in `docs/AUTH.md`, including that a migrated account becomes unverified
- [ ] 10.3 Correct the statement in `docs/ARCHITECTURE.md` that DID documents carry only signing keys
- [ ] 10.4 Record the limitation that an account whose host holds its rotation keys can replace the verification method, and that the log is what makes it visible
