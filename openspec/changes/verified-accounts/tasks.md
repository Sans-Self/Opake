## Scope and rollout

This checklist remains the account-verification implementation. The approved scenario
decisions have separate checklists in `rotation-grace-periods`, `rotation-write-safety`,
`bounded-key-history`, and `membership-mutation-outcomes`; do not count them as completed
or silently absorb them here. Follow [change-map.md](change-map.md) for dependencies and
sync order. Missing-wrap exclusions must not ship with the old destructive document sweep.
Bounded history is explicitly implementation-gated on its separate storage-design review.

## 1. Lexicon, record model, and version

- [ ] 1.1 Add optional `signature` and `signatureAlgo` fields to `lexicons/at.opake.publicKey.json`, with a byte-length bound and a known-values list for the algorithm
- [ ] 1.2 Register signature vocabulary and declare the pre-v1 version-1 redefinition covering the member/approval break; verify fixtures and schema validation use the same draft with no compatibility shim
- [ ] 1.3 Add both fields to the record model in `crates/opake-core/src/records/public_key.rs` as optional values, leaving parsing of existing records unchanged
- [ ] 1.4 Add a regression asserting an existing unsigned record still parses and yields identical key bytes

- [ ] 1.5 Change keyring member lexicon/model to required DID and role, optional current wrap and 32-byte approval commitment, including history snapshots; verify valid missing-wrap entries, duplicate-DID rejection, wrap-DID mismatch rejection, malformed approval rejection, and prior-draft rejection
- [ ] 1.6 Extend encrypted grant/pending metadata for key-bound approval, confirmed recipient DID, and explicit first-publication permission; verify metadata round trips and missing permission never defaults to consent
- [ ] 1.7 Regenerate development fixtures under the declared pre-v1 break and document the reset procedure; verify bootstrap, indexer ingest, and native/web clients agree on the new record shape

## 2. Signature construction

- [ ] 2.1 Add a context label for the published key record, built from the record's declared `opakeVersion`, alongside the existing transcript labels
- [ ] 2.2 Build the signed transcript from the closed, ordered field list: label, DID, `opakeVersion`, both encryption keys and their algorithms, `signingKey`, `signingAlgo`, `createdAt`
- [ ] 2.3 Implement signing with the identity's Ed25519 key, and verification that takes its algorithm from `signatureAlgo` and its scheme version from `opakeVersion`, never from the build
- [ ] 2.4 Add a golden vector pinning the transcript bytes so an accidental change to field order or labelling fails a test
- [ ] 2.5 Add a regression asserting a signature over one account's record does not verify when presented under another account's DID
- [ ] 2.6 Add a regression asserting a record whose byte encoding changed without a field value changing still verifies
- [ ] 2.7 Add a regression asserting a record carrying an unrecognised additional field still verifies, since the covered list is closed

- [ ] 2.8 Implement the versioned approval commitment using the spec's closed tuple; verify golden bytes, scope/DID separation, changes to either encryption key or algorithm, and equality across timestamp-only or encoding-only republication

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

## 5. Publication and removal

- [ ] 5.1 Sign the record in `publish_public_key`, writing the signed record on login, recovery, and share healing
- [ ] 5.2 Obtain separate authorization with fresh DPoP, PKCE, and state for each identity operation; verify callback issuer/state and token subject against the initiating account, with wrong-account, mismatched-callback, and separate-key regressions
- [ ] 5.3 Attempt revocation of access and every issued refresh/durable credential on handled submission, refusal, or abandonment, with independent bounded attempts and unconditional local disposal; verify network failure and timeout do not suppress the other attempt or retain credentials
- [ ] 5.4 Treat successful revocation responses as unverified and report cleanup failure separately from submission success, refusal, or unknown outcome; verify result shapes never claim server authority is known to have ended
- [ ] 5.5 Build the DID-document operation by reading the current verification methods and submitting the complete map, since supplying the field replaces it wholesale
- [ ] 5.6 Add the operation that publishes the `#opake` verification method, ordered strictly after the signed record exists
- [ ] 5.7 Add the operation that removes the verification method, returning the account to unverified
- [ ] 5.8 Report signer refusal rather than retrying silently, distinguish known confirmation-delivery failure from pending or unknown delivery, and never infer delivery from acceptance; verify each outcome with signer/confirmation fixtures
- [ ] 5.9 Add a regression asserting publication order, so a verified account never obliges a check it cannot yet satisfy
- [ ] 5.10 Keep pending and completed identity authorization out of every persisted type and application credential accessor, with zeroization and redacted diagnostics; verify type/API review and owned-secret cleanup tests against the WASM-boundary delta
- [ ] 5.11 Add regressions asserting session persistence mid-operation carries no identity credentials, pending authorization never uses persisted `PendingLogin`, and reload or tab closure restores only the standing session with nothing to resume
- [ ] 5.12 Add regressions asserting bounded revocation attempts on every handled exit including cancellation, while abrupt termination makes no revocation guarantee and starts no persisted cleanup job
- [ ] 5.13 Add a regression asserting the submitted operation preserves every verification method it did not intend to change
- [ ] 5.14 Keep one client metadata declaration of possible scopes but separate actual standing and identity authorization scopes; verify issuing and cleaning up an identity grant leaves the standing session functional and without identity authority
- [ ] 5.15 Implement the signer's owner-confirmation step for add and remove, retaining the grant through signing and submission; verify the local PDS bodyless confirmation request, valid and invalid confirmation, and sign-before-submit-before-cleanup ordering
- [ ] 5.16 Implement finite authorization/confirmation deadlines and cancellation during in-flight work, rejecting late callbacks and cleaning up late token responses; verify fake-clock timeout and controlled-response race tests without reviving the operation
- [ ] 5.17 Reconcile an uncertain submission against freshly read DID state before offering another mutation, with fresh authorization if one is needed; verify lost-response cases where the operation did apply, did not apply, conflicts with current state, or cannot yet be resolved

## 6. Single-recipient callers

- [ ] 6.1 Resolve verification state in workspace member addition, refusing the error state before any wrap is computed
- [ ] 6.2 Resolve verification state in grant creation, refusing the error state before any grant record is written
- [ ] 6.3 Thread explicit unverified confirmation through admission and grant creation, writing key-bound evidence with the wrap; verify another device uses identical-key approval without prompting and changed or missing approval never transfers
- [ ] 6.4 Verify received keys against the DID document in pairing completion, rejecting the error state
- [ ] 6.5 Add regressions asserting no record is written on the error state in each of the three paths

- [ ] 6.6 Add manager-authorized approval/repair for an already-admitted member without re-admission; verify same-rotation supersede preserves unrelated entries/history, decline writes nothing, and non-managers cannot alter another member's approval

## 7. Multi-recipient group-key rotation

- [ ] 7.1 Resolve each remaining member independently in the removal re-wrap loop in `crates/opake-core/src/opake.rs`
- [ ] 7.2 Retain DID, role, approval, and historical wraps when resolution errors or unverified approval is missing/mismatched; verify removal completes without that member's new wrap and never copies an old wrap as current
- [ ] 7.3 Report excluded members to the authoring manager as part of the operation's result
- [ ] 7.4 Re-wrap an unverified member without prompting only when resolved encryption keys match recorded approval; verify unchanged republication, either-key substitution, and fresh-resolution changes before wrapping
- [ ] 7.5 Extend member-wrap repair to verified or approved unverified recipients, gated by current manager authority and possession of the current key; verify stale work cannot re-add removed members, overwrite newer approval, or claim missing intermediate rotations were repaired
- [ ] 7.6 Add a regression asserting a removal completes with one member unverifiable, that forward secrecy holds, and that the excluded member is named in the result

- [ ] 7.7 Represent historical-only group keys and update all resolution/adoption paths without bypassing genesis derivation; verify historical reads with no current wrap, rejection without usable rotation 0, and explicit failure of operations requiring the missing current key
- [ ] 7.8 Update live projections for new-head adoption without a current wrap and same-rotation repair; verify no stale rotation remains active, no mistaken sidebar removal, and unlocking works without reload
- [ ] 7.9 Preserve current approval and wrap presence through self-removal, and rebuild both from the restored head on rollback; verify unauthorized approval changes fail and rollback never borrows approval from a deleted head

## 8. Unattended paths

- [ ] 8.1 Capture one first-publication permission bound to the resolved recipient DID in encrypted intent metadata; verify declined permission writes nothing and handle reassignment cannot redirect it
- [ ] 8.2 Atomically create the designated grant with actual-key approval and consume the unchanged pending intent under repository-revision CAS, never unconditional grant upsert; verify two devices resolving different bundles produce only one handoff
- [ ] 8.3 Report an error-state recipient to the owner rather than counting an ordinary retry failure, and carry the reason if the share reaches its TTL
- [ ] 8.4 Add regressions for the queue-time confirmation and the error-state report

- [ ] 8.5 Add conditional same-repository transaction plumbing and prove its PDS semantics before relying on first-use consumption; verify pending replacement/cancellation, grant collision, batch failure, and unrelated repository writes cause conflict without partial grant publication
- [ ] 8.6 Reconcile unknown queued-share completion against the bound intent and designated grant; verify lost-response retry, process restart, expiry/cancellation races, and later grant revocation cannot reuse the first-use permission

## 9. Self-check and repair

- [ ] 9.1 Check the account's own DID document for its `#opake` verification method at boot, reading the document itself rather than the host's recommended credentials, which cannot report it
- [ ] 9.2 Report absent and mismatched distinctly, offering republication only for the absent case
- [ ] 9.3 Add a regression asserting a verification method holding a foreign key is reported as substitution and not silently repaired

## 10. Indexer

- [ ] 10.1 Accept an authenticating account that publishes no verification method, unchanged from today
- [ ] 10.2 Refuse an authenticating account whose verification method is present and whose published record does not verify under it
- [ ] 10.3 Ensure the key cache cannot serve a stale decision across a change in verification state
- [ ] 10.4 Add controller tests for the accept and refuse paths

- [ ] 10.5 Update indexer membership lookup, list/subscription fan-out, and self-removal validation for explicit DID/role independent of wraps; verify admitted missing-wrap members are served, removed DIDs receive 403, and self-removal cannot mutate remaining approvals

## 11. Clients

- [ ] 11.1 Surface verification state wherever a counterparty is named, with an accessible non-colour indicator
- [ ] 11.2 Present the unverified confirmation at the point access is granted, naming the consequence rather than the mechanism
- [ ] 11.3 Present the error state as a refusal with no override
- [ ] 11.4 Present excluded members after a group-key rotation, naming what they cannot read until repaired
- [ ] 11.5 Add CLI setup/removal with owner-confirmation input and cancellation, naming a delivery channel only when the signer establishes it and promising no per-operation consent screen; verify accepted, refused, cancelled, and delivery-unknown flows
- [ ] 11.6 Add the self-check result to the web account view
- [ ] 11.7 Add web setup/removal using a live WASM operation and a same-origin callback channel, with no persisted pending identity state; verify opener-isolated authorization, blocked popup, explicit cancellation, finite timeout, and reload without relying on `popup.closed`

- [ ] 11.8 Distinguish historical-only access, pending unverified-key approval, and verification error in CLI/web, with an explicit manager repair action; verify current-key-dependent actions are disabled or fail clearly, while historical reads and unchanged-key rotations do not prompt

## 12. Dev-env and end-to-end

- [ ] 12.1 Bootstrap at least one verified fixture actor: signed record plus an `#opake` verification method in the local DID directory
- [ ] 12.2 Add a harness affordance supplying the confirmation for scenarios that wrap to an unverified counterparty
- [ ] 12.3 Add e2e coverage for all three resolution outcomes and for a removal completing with an unverifiable member
- [ ] 12.4 Promote the authorization spike's successful add/remove, persistence, revocation, and failure scenarios to production-path tests with namespace-scoped actors; provide an explicit owner-confirmation fixture without bypassing signer validation, and distinguish simulated token delivery from any tested mail delivery

- [ ] 12.5 Add cross-device end-to-end coverage for changed-key exclusion, preserved membership/history, explicit approval, same-rotation repair, rollback, and queued first-use concurrency; verify these through production APIs rather than marking hypothetical reviews as test runs

## 13. Documentation

- [ ] 13.1 Document the verification method, the signed transcript, and the three states in `docs/CRYPTO.md`
- [ ] 13.2 Document the setup and repair flows in `docs/AUTH.md`, including that a migrated account becomes unverified
- [ ] 13.3 Correct the statement in `docs/ARCHITECTURE.md` that DID documents carry only signing keys
- [ ] 13.4 Record that a host holding the account's rotation keys can replace the verification method, and that a host can refuse to publish one for an account that holds no rotation key of its own
- [ ] 13.5 Document rotation-key custody as the deployment property that bounds both limitations: a key held off the infrastructure that serves the PDS and listed at higher authority than the host's, and what the recovery window does and does not cover
- [ ] 13.6 Align `docs/CRYPTO.md`, `docs/AUTH.md`, and `docs/ARCHITECTURE.md` with the terminology capability, in particular the two signing keys a verified account's DID document carries and the three operations called rotation
- [ ] 13.7 Use verified and unverified in every interface string, keeping anchor out of user-facing copy
- [ ] 13.8 Sweep the specs and docs for "host" where a DID-document operation is in view, replacing it with the party the terminology capability names
- [ ] 13.9 Align `docs/AUTH.md` and `docs/ARCHITECTURE.md` with the transient injected-transport allowance, operation-only credential custody, and interruption/cleanup limits; verify the docs promise neither persisted identity continuation nor secrecy from browser-managed I/O or guaranteed revocation after termination

- [ ] 13.10 Document member versus wrap state, key-bound approval, same-repository first-use consumption, and pre-v1 reset policy; verify docs make no new walk-free authority, missed-generation recovery, or automatic grant-rewrap claim
