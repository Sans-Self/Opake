## ADDED Requirements

### Requirement: A workspace admits at most 256 simultaneous members

The initial supported limit SHALL be 256 DIDs in canonical current membership, regardless
of role or current-wrap availability. Excluded members still admitted during grace SHALL
count toward that limit. Removed members retained in historical-key records SHALL NOT
count toward it. An admission exceeding the limit SHALL fail clearly before publishing
a new membership head, without changing current membership.

The limit SHALL NOT cap total lifetime admissions, historical recipients of a generation,
or the number of rotations. Writers and readers SHALL still obey declared individual
record-byte and structural bounds; a logical member count does not prove encoded size.

#### Scenario: the 257th simultaneous member is refused

- **GIVEN** a canonical head with 256 current members, including some without current wraps
- **WHEN** a manager attempts another admission
- **THEN** the operation reports the supported member limit without writing a new head

#### Scenario: removal frees a current slot, not a historical slot

- **GIVEN** a workspace with 256 members whose history also records earlier removed members
- **WHEN** a removal becomes canonical and a manager admits a replacement
- **THEN** the new admission fits the current-member limit while historical access remains stored independently

## MODIFIED Requirements

### Requirement: Removal rotates the group key; leave does not

Removing a member SHALL rotate: the authoring manager mints a new group key, re-wraps it for every eligible remaining member, bumps `rotation`, and preserves the prior rotation's member wraps in authenticated historical-key storage so existing documents stay readable (`spec:document-crypto § Keyring reads select the group key by the document's rotation`). Required new history material SHALL be durably published before or atomically with the new head. The removed member receives no wrap for the new group key; confidentiality remains bounded by the fresh-key and in-flight contract in `spec:key-rotation § Removal confidentiality is a key-generation boundary, not a global clock`.

The authoring manager SHALL resolve each remaining member's verification state independently before re-wrapping (`spec:account-verification § Key resolution is three-valued, and an anchored account may not serve an unsigned record`). A remaining member who resolves to the error state SHALL be excluded from the re-wrap and reported, and SHALL NOT abort the removal (`spec:account-verification § Recipients are resolved independently and a multi-recipient operation never fails wholesale`). A removal is a withdrawal of access, and an account the removal is not withdrawing access from SHALL NOT be able to prevent it — otherwise one host serving an unverifiable record for its own user permanently blocks the removal of anyone else in the workspace.

Re-wrapping an unverified member SHALL NOT ask for confirmation again when the resolved encryption keys match the head's key-bound approval. If those keys changed, or no applicable approval exists, the removal SHALL finish without that member's new wrap and report that a manager's fresh confirmation is needed. It SHALL NOT wait for that decision or silently fall back to formerly approved keys. The member remains admitted with their role and historical wraps. Resolution errors and missing approval SHALL be reported distinctly; the former offers no override, while the latter permits a subsequent explicit decision. The lifecycle of either missing wrap is `spec:key-rotation § The rotation event is synchronous and self-sufficient`.

Leave SHALL NOT rotate. The leaver authors the supersede, so any key minted in it is a key the leaver knows — rotation there costs a rotation number and buys nothing. A leave carries the remaining member entries, including absent wraps, approvals, and deadlines, the rotation counter, and the historical-key references unchanged, dropping only the author's current entry. Forward secrecy against a departed member arrives with the next manager-authored rotation; `remove_workspace_member` covers the uncooperative case.

#### Scenario: removal locks out future content

- **GIVEN** bob removed by a manager, rotation bumped from n to n+1
- **WHEN** a document is uploaded under rotation n+1
- **THEN** bob holds no wrap for rotation n+1 and cannot decrypt it, while remaining members read rotation-n documents via rotation-addressed historical-key lookup

#### Scenario: leave carries everything but the author

- **WHEN** bob (editor) leaves
- **THEN** the written record has the prior rotation, the prior historical-key references, and every other member's wrap and role unchanged
- Test: `leave_workspace_writes_self_removal_supersede` (crates/opake-core/src/opake_tests.rs)

#### Scenario: a remaining member's unverifiable record does not veto a removal

- **GIVEN** a workspace with alice (manager), bob (editor) and carol (editor), where carol's host serves a record that does not verify under her verification method
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

### Requirement: Membership state is the keyring head's member list

The member list of the current keyring chain head is the sole source of membership truth. Every membership decision — authority checks, indexer 403s, entry building for the sidebar — SHALL evaluate the head's `members[]`, keyed by an explicit `did`, carrying `role`. (Which record is "the head" and why the genesis record must never be consulted for membership is owned by `spec:workspace-identity § Membership authority is the live chain head`.)

Each member entry SHALL carry a required DID and role independently of its optional `wrappedKey`. DIDs SHALL be unique within current membership. A present wrap SHALL name that entry's DID and protect the group key for the containing rotation: the head's `rotation` for current members, or the authenticated historical-key record's declared rotation for historical recipient wraps. A missing wrap SHALL mean no key is supplied to that member for that rotation, never non-membership or a role change. A prior-rotation wrap SHALL NOT be carried as a current wrap.

Exclusion from re-wrapping SHALL retain the member's DID, role, and existing key-bound approval in the head, and SHALL preserve historical wraps. Historical recipient wraps, membership snapshots, and approval entries SHALL NOT confer current authority or override the head's approval. Missing-current-wrap state SHALL be derivable from the head alone, not from a separate repair queue (`spec:background-work § Remaining work is derived from records, never stored`).

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
