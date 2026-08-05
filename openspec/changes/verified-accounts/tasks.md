## 1. Lexicon, record model, and version

- [ ] 1.1 Add optional `signature` and `signatureAlgo` fields to `lexicons/at.opake.publicKey.json`, with a byte-length bound and a known-values list for the algorithm
- [ ] 1.2 Register the signature algorithm identifier in the vocabulary and bump the schema version, declaring the break rather than treating the addition as purely additive
- [ ] 1.3 Add both fields to the record model in `crates/opake-core/src/records/public_key.rs` as optional values, leaving parsing of existing records unchanged
- [ ] 1.4 Add a regression asserting an existing unsigned record still parses and yields identical key bytes

## 2. Signature construction

- [ ] 2.1 Add a context label for the published key record, built from the record's declared `opakeVersion`, alongside the existing transcript labels
- [ ] 2.2 Build the signed transcript from the closed, ordered field list: label, DID, `opakeVersion`, both encryption keys and their algorithms, `signingKey`, `signingAlgo`, `createdAt`
- [ ] 2.3 Implement signing with the identity's Ed25519 key, and verification that takes its algorithm from `signatureAlgo` and its scheme version from `opakeVersion`, never from the build
- [ ] 2.4 Add a golden vector pinning the transcript bytes so an accidental change to field order or labelling fails a test
- [ ] 2.5 Add a regression asserting a signature over one account's record does not verify when presented under another account's DID
- [ ] 2.6 Add a regression asserting a record whose byte encoding changed without a field value changing still verifies
- [ ] 2.7 Add a regression asserting a record carrying an unrecognised additional field still verifies, since the covered list is closed

## 3. DID document verification methods

- [ ] 3.1 Extend the DID document model to expose verification methods by fragment, for both `did:plc` and `did:web` resolution paths
- [ ] 3.2 Decode a verification method's multibase key value into an Ed25519 public key, rejecting other key types with a distinct error
- [ ] 3.3 Add a lookup returning the account's `#opake` key, absent, or malformed, as three distinguishable outcomes

## 4. Three-valued resolution

- [ ] 4.1 Extend identity resolution in `crates/opake-core/src/resolve.rs` to return the verification state alongside the key bundle
- [ ] 4.2 Classify version and vocabulary before verifying the signature, so a corrupt or future-version record refuses with its own reason and version skew is never reported as an attack
- [ ] 4.3 Return the error state when a verification method is present and the signature is absent, malformed, or does not verify, distinct from every existing resolution error
- [ ] 4.4 Return the error state when a verified account's record omits `signingKey`, or when the record's `signingKey` differs from the DID document's `#opake` key
- [ ] 4.5 Return the unverified state when no verification method is present, preserving the existing `RecipientNotReady` and wrong-algorithm outcomes unchanged
- [ ] 4.6 Read the DID's operation history at resolution time and report whether the `#opake` verification method has ever been replaced with a different key, treating a removal and re-addition of the same key as no replacement
- [ ] 4.7 Cache the history under the same expiry as the rest of resolution, keeping no record of previously observed verification methods
- [ ] 4.8 Add regressions for each outcome, including a stripped-signature case asserting refusal rather than downgrade, and a replaced-anchor case asserting a valid signature is still reported as a replacement

## 5. Publication

- [ ] 5.1 Sign the record in `publish_public_key`, writing the signed record on login, recovery, and share healing
- [ ] 5.2 Extend the OAuth scope with the identity-operation grant, and handle the re-consent every existing session will require
- [ ] 5.3 Add the operation that publishes the `#opake` verification method, ordered strictly after the signed record exists
- [ ] 5.4 Add the operation that removes the verification method, returning the account to unverified
- [ ] 5.5 Report a host's refusal to sign the identity operation to the owner rather than retrying silently
- [ ] 5.6 Add a regression asserting publication order, so a verified account never obliges a check it cannot yet satisfy

## 6. Single-recipient callers

- [ ] 6.1 Resolve verification state in workspace member addition, refusing the error state before any wrap is computed
- [ ] 6.2 Resolve verification state in grant creation, refusing the error state before any grant record is written
- [ ] 6.3 Thread an explicit confirmation through both paths for the unverified state, recorded per relationship so later re-wraps do not re-ask
- [ ] 6.4 Verify received keys against the DID document in pairing completion, rejecting the error state
- [ ] 6.5 Add regressions asserting no record is written on the error state in each of the three paths

## 7. Multi-recipient rotation

- [ ] 7.1 Resolve each remaining member independently in the removal re-wrap loop in `crates/opake-core/src/opake.rs`
- [ ] 7.2 Exclude a member resolving to the error state from the re-wrap and complete the rotation, rather than aborting it
- [ ] 7.3 Report excluded members to the authoring manager as part of the operation's result
- [ ] 7.4 Re-wrap without prompting for a member already admitted as unverified
- [ ] 7.5 Extend the re-wrap sweep to pick up members excluded from a rotation once their record verifies
- [ ] 7.6 Add a regression asserting a removal completes with one member unverifiable, that forward secrecy holds, and that the excluded member is named in the result

## 8. Unattended paths

- [ ] 8.1 Capture the confirmation covering the eventual wrap when a pending share is queued, and write no pending record without it
- [ ] 8.2 Complete a queued share under the captured confirmation without prompting
- [ ] 8.3 Report an error-state recipient to the owner rather than counting an ordinary retry failure, and carry the reason if the share reaches its TTL
- [ ] 8.4 Add regressions for the queue-time confirmation and the error-state report

## 9. Self-check and repair

- [ ] 9.1 Check the account's own DID document for its `#opake` verification method at boot
- [ ] 9.2 Report absent and mismatched distinctly, offering republication only for the absent case
- [ ] 9.3 Add a regression asserting a verification method holding a foreign key is reported as substitution and not silently repaired

## 10. Indexer

- [ ] 10.1 Accept an authenticating account that publishes no verification method, unchanged from today
- [ ] 10.2 Refuse an authenticating account whose verification method is present and whose published record does not verify under it
- [ ] 10.3 Ensure the key cache cannot serve a stale decision across a change in verification state
- [ ] 10.4 Add controller tests for the accept and refuse paths

## 11. Clients

- [ ] 11.1 Surface verification state wherever a counterparty is named, with an accessible non-colour indicator
- [ ] 11.2 Present the unverified confirmation at the point access is granted, naming the consequence rather than the mechanism
- [ ] 11.3 Present the error state as a refusal with no override
- [ ] 11.4 Present excluded members after a rotation, naming what they cannot read until repaired
- [ ] 11.5 Add the verification setup and removal flows to the CLI
- [ ] 11.6 Add the self-check result to the web account view

## 12. Dev-env and end-to-end

- [ ] 12.1 Bootstrap at least one verified fixture actor: signed record plus an `#opake` verification method in the local DID directory
- [ ] 12.2 Add a harness affordance supplying the confirmation for scenarios that wrap to an unverified counterparty
- [ ] 12.3 Add e2e coverage for all three resolution outcomes and for a removal completing with an unverifiable member

## 13. Documentation

- [ ] 13.1 Document the verification method, the signed transcript, and the three states in `docs/CRYPTO.md`
- [ ] 13.2 Document the setup and repair flows in `docs/AUTH.md`, including that a migrated account becomes unverified
- [ ] 13.3 Correct the statement in `docs/ARCHITECTURE.md` that DID documents carry only signing keys
- [ ] 13.4 Record that a host holding the account's rotation keys can replace the verification method, and that a host can refuse to publish one for an account that holds no rotation key of its own
- [ ] 13.5 Document rotation-key custody as the deployment property that bounds both limitations: a key held off the infrastructure that serves the PDS and listed at higher authority than the host's, and what the recovery window does and does not cover
