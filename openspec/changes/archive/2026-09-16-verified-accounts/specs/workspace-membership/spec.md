## MODIFIED Requirements

### Requirement: Membership state is the keyring head's member list

The member list of the current keyring chain head is the sole source of membership truth. Every membership decision — authority checks, indexer 403s, entry building for the sidebar — SHALL evaluate the head's `members[]`, keyed by an explicit `did`, carrying `role`. (Which record is "the head" and why the genesis record must never be consulted for membership is owned by `spec:workspace-identity § Membership authority is the live chain head`.)

Each member entry SHALL carry a required DID and role independently of its optional `wrappedKey`. DIDs SHALL be unique within each member list. A present wrap SHALL name that entry's DID and protect the group key for the containing rotation: the head's `rotation` for current members, or the enclosing `keyHistory` entry's rotation for historical members. A missing wrap SHALL mean no key is supplied to that member for that rotation, never non-membership or a role change. A prior-rotation wrap SHALL NOT be carried as a current wrap.

Exclusion from re-wrapping SHALL retain the member's DID, role, and existing key-bound approval in the head, and SHALL preserve historical wraps. Historical membership and approval entries SHALL NOT confer current authority or override the head's approval. Missing-current-wrap state SHALL be derivable from the head alone, not from a separate repair queue (`spec:background-work § Remaining work is derived from records, never stored`).

#### Scenario: indexer role lookup reads the head

- **GIVEN** a member added two supersedes ago
- **WHEN** the indexer resolves their role for an authority check
- **THEN** the role comes from the current head's member list (`RecordQueries.member_role/2` via `chain_heads`)

#### Scenario: missing wrap does not remove a member

- **GIVEN** a head retaining Carol's DID and editor role but no current-rotation wrap
- **WHEN** a client or indexer evaluates membership
- **THEN** Carol remains an editor, and her missing current key is reported separately from membership

#### Scenario: conflicting member identities are invalid

- **WHEN** a keyring contains duplicate member DIDs or a member's wrap names a different DID
- **THEN** the record is rejected as invalid rather than choosing whichever identity a consumer happens to inspect

### Requirement: Keyring supersede authority is manager-only, except pure self-removal

A keyring supersede SHALL be valid iff the author is currently a manager, OR the author is a non-manager member and the supersede is a pure self-removal. Under that exception, the new record SHALL equal the head after removing only the author's current member entry and changing the chain-edge transport fields (`supersedes`, its content pin, lineage, and record timestamps). The rotation counter, key history, encrypted metadata, every remaining member field, and any other record field SHALL be preserved. Equality of `$bytes` values is equality of decoded bytes, so a representation change alone does not reject a leave. Dropping anyone else, adding anyone, changing any remaining member's role, wrap, approval, or another record field, or keeping oneself in the list SHALL be rejected.

Renewing approval, repairing a missing group-key wrap, and all other keyring mutations SHALL be manager-authored supersedes, subject to the same authority checks as admission (`spec:account-verification § Key-bound approval is carried by the relationship's records`). A background runner has only its acting account's authority, never a separate repair privilege.

The rule SHALL be enforced in the indexer (`check_keyring_supersede/4` + `pure_self_removal?`, authority.ex) and re-checked client-side for a fast, clear error before the write. The two checks express the same rule; the indexer's is authoritative.

#### Scenario: editor leaves

- **GIVEN** a head with alice (manager), bob (editor), carol (viewer)
- **WHEN** bob authors a supersede whose members are exactly alice (manager) and carol (viewer), with their wrap presence and approvals unchanged
- **THEN** the supersede is accepted
- Tests: `editor leaving passes` and siblings, apps/indexer/test/opake_indexer/authority_db_test.exs

#### Scenario: self-removal that smuggles a change

- **WHEN** bob's supersede also drops carol, re-roles carol, or adds a new member
- **THEN** it is rejected with insufficient role
- Tests: `editor dropping someone else alongside themselves is rejected`, `editor re-roling a remaining member while leaving is rejected`, `editor adding a member while leaving is rejected` (authority_db_test.exs)

#### Scenario: leaving does not authorize replacement keys

- **GIVEN** bob is an editor and carol's current entry has a missing wrap and approval for encryption bundle A
- **WHEN** bob leaves while changing carol's approval to bundle B, adding a wrap for her, changing the rotation/history/metadata, or adding an unknown record field
- **THEN** the supersede is rejected as more than pure self-removal

### Requirement: Adding a member is a manager-authored supersede

A manager adds a member by wrapping the current group key to the recipient's published hybrid public keys and appending a member entry with their DID, role, and wrap to a superseding keyring record. Adding a DID already in the member list SHALL be rejected, even if that member has no current wrap; repair is not admission. The wrap's AEAD anchor is the genesis URI (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`); the role is assigned at add time.

Before wrapping, the authoring manager SHALL resolve the recipient's verification state (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). The add SHALL be refused outright when resolution yields the error state, and SHALL require the manager's explicit confirmation when the recipient is unverified (`spec:account-verification § Wrapping a key to an unverified account requires explicit confirmation`). That confirmation's key-bound evidence SHALL be written with the member entry. A group key admits the recipient to every document the workspace holds, so the consequence of wrapping it to a substituted key is borne by every member rather than by the manager alone.

Direct manager add is the only admission channel: no invitation, request-to-join, or other self-service path exists.

#### Scenario: duplicate add rejected

- **GIVEN** bob already in the member list
- **WHEN** a manager adds bob again
- **THEN** the operation fails before any write (`add_workspace_member`, crates/opake-core/src/opake.rs)

#### Scenario: an add to an account serving an unverifiable record is refused

- **GIVEN** a prospective member whose DID document carries a verification method and whose published record does not verify under it
- **WHEN** a manager adds them
- **THEN** the add is refused before any wrap is computed, and no keyring supersede is written

#### Scenario: an add to an unverified account waits for the manager

- **GIVEN** a prospective member with no verification method
- **WHEN** a manager adds them
- **THEN** the manager is told the recipient's keys are not vouched for, and the group key is wrapped only after explicit confirmation

### Requirement: Removal rotates the group key; leave does not

Removing a member SHALL rotate: the authoring manager mints a new group key, re-wraps it for every eligible remaining member, bumps `rotation`, and pushes the prior rotation's members into `keyHistory` so existing documents stay readable (`spec:document-crypto § Keyring reads select the group key by the document's rotation`). The removed member never sees the new key — that is the forward-secrecy contract, bounded by the no-historical-revocation posture (removed members keep whatever they already had).

The authoring manager SHALL resolve each remaining member's verification state independently before re-wrapping (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). A remaining member who resolves to the error state SHALL be excluded from the re-wrap and reported, and SHALL NOT abort the removal (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). A removal is a withdrawal of access, and an account the removal is not withdrawing access from SHALL NOT be able to prevent it — otherwise one PDS operator serving an unverifiable record for its own user permanently blocks the removal of anyone else in the workspace.

Re-wrapping an unverified member SHALL NOT ask for confirmation again when the resolved encryption keys match the head's key-bound approval. If those keys changed, or no applicable approval exists, the removal SHALL finish without that member's new wrap and report that a manager's fresh confirmation is needed. It SHALL NOT wait for that decision or silently fall back to formerly approved keys. The member remains admitted with their role and historical wraps. Resolution errors and missing approval SHALL be reported distinctly; the former offers no override, while the latter permits a subsequent explicit decision. The lifecycle of either missing wrap is `spec:key-rotation § The rotation event is synchronous and self-sufficient`.

Leave SHALL NOT rotate. The leaver authors the supersede, so any key minted in it is a key the leaver knows — rotation there costs a rotation number and buys nothing. A leave carries the remaining member entries, including absent wraps and approvals, the rotation counter, and the key history verbatim, dropping only the author's current entry. Forward secrecy against a departed member arrives with the next manager-authored rotation; `remove_workspace_member` covers the uncooperative case.

#### Scenario: removal locks out future content

- **GIVEN** bob removed by a manager, rotation bumped from n to n+1
- **WHEN** a document is uploaded under rotation n+1
- **THEN** bob holds no wrap for rotation n+1 and cannot decrypt it, while remaining members read rotation-n documents via `keyHistory`

#### Scenario: leave carries everything but the author

- **WHEN** bob (editor) leaves
- **THEN** the written record has the prior rotation, the prior key history, and every other member's wrap and role unchanged
- Test: `leave_workspace_writes_self_removal_supersede` (crates/opake-core/src/opake_tests.rs)

#### Scenario: a remaining member's unverifiable record does not veto a removal

- **GIVEN** a workspace with alice (manager), bob (editor) and carol (editor), where carol's PDS serves a record that does not verify under her verification method
- **WHEN** alice removes bob
- **THEN** the supersede is written with a new group key wrapped to alice, carol is excluded from the re-wrap and reported to alice, and bob holds no wrap for the new rotation

#### Scenario: changed unverified keys do not veto a removal

- **GIVEN** alice admitted carol as an unverified editor under encryption bundle A, and carol now resolves as unverified under bundle B
- **WHEN** alice removes bob
- **THEN** the removal completes without another confirmation prompt, bob is absent from the head, and carol remains an editor with approval for A but no new wrap; alice is told that wrapping to B needs a separate confirmation

#### Scenario: a declined repair leaves withdrawal intact

- **GIVEN** a removal completed while withholding carol's new wrap pending approval of her changed keys
- **WHEN** a manager declines that approval
- **THEN** carol remains admitted without the new wrap, and neither the removal nor the rotation is rolled back
