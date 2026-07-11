# workspace-membership Specification

## Purpose

Define who is in a workspace, what each role may do to the member list, and which membership mutations rotate the group key.

Membership is a list of `{did, role}` pairs on the keyring chain head, and the authority rule over that list is what the indexer enforces on every keyring supersede: managers may reshape it, and any member may remove exactly themselves. The self-removal exception is stated to the equality check, because "an editor may remove themselves" and "an editor may author a keyring supersede" differ by exactly that check — a looser reading is a privilege escalation. There is no owner: the workspace creator is a manager like any other, and every capability in this spec attaches to a role, never to a DID's history.

Identity, AEAD anchoring, and the head-vs-genesis rules are owned by the workspace-identity spec and referenced here, not restated.

## Requirements

### Requirement: Three roles, no owner

Workspace roles are `manager`, `editor`, and `viewer` — the lexicon-canonical lowercase strings. There SHALL be no owner role: the DID that authored the genesis keyring is a historical fact recorded in the genesis at-uri, not an authorization level. After genesis, the creator is a manager, capability-identical to any other manager (FEDERATION.md).

A role string outside the known set SHALL be treated as insufficient authority and logged, never silently accepted — an unknown role is lexicon drift or tampered data (indexer `@known_roles`, apps/indexer/lib/opake_indexer/authority.ex).

#### Scenario: creator holds no special authority

- **GIVEN** a workspace whose creator demoted themselves to editor via a manager's role change
- **WHEN** the creator attempts a membership mutation
- **THEN** it is rejected exactly as any editor's would be

### Requirement: Membership state is the keyring head's member list

The member list of the current keyring chain head is the sole source of membership truth. Every membership decision — authority checks, indexer 403s, entry building for the sidebar — SHALL evaluate the head's `members[]`, keyed by `wrappedKey.did`, carrying `role`. (Which record is "the head" and why the genesis record must never be consulted for membership is owned by `spec:workspace-identity § Membership authority is the live chain head`.)

#### Scenario: indexer role lookup reads the head

- **GIVEN** a member added two supersedes ago
- **WHEN** the indexer resolves their role for an authority check
- **THEN** the role comes from the current head's member list (`RecordQueries.member_role/2` via `chain_heads`)

### Requirement: Keyring supersede authority is manager-only, except pure self-removal

A keyring supersede SHALL be valid iff the author is currently a manager, OR the author is a non-manager member and the supersede is a pure self-removal: the new member list equals the head's list minus the author, compared on `{did, role}` pairs. Under the exception, dropping anyone else, adding anyone, changing any remaining member's role, or keeping oneself in the list SHALL be rejected. Wrapped-key bytes are not compared — they legitimately differ across supersedes.

The rule SHALL be enforced in the indexer (`check_keyring_supersede/4` + `pure_self_removal?`, authority.ex) and re-checked client-side for a fast, clear error before the write. The two checks express the same rule; the indexer's is authoritative.

#### Scenario: editor leaves

- **GIVEN** a head with alice (manager), bob (editor), carol (viewer)
- **WHEN** bob authors a supersede whose members are exactly alice (manager) and carol (viewer)
- **THEN** the supersede is accepted
- Tests: `editor leaving passes` and siblings, apps/indexer/test/opake_indexer/authority_db_test.exs

#### Scenario: self-removal that smuggles a change

- **WHEN** bob's supersede also drops carol, re-roles carol, or adds a new member
- **THEN** it is rejected with insufficient role
- Tests: `editor dropping someone else alongside themselves is rejected`, `editor re-roling a remaining member while leaving is rejected`, `editor adding a member while leaving is rejected` (authority_db_test.exs)

### Requirement: Adding a member is a manager-authored supersede

A manager adds a member by wrapping the current group key to the recipient's published hybrid public keys and appending the wrap to a superseding keyring record. Adding a DID already in the member list SHALL be rejected. The wrap's AEAD anchor is the genesis URI (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`); the role is assigned at add time.

#### Scenario: duplicate add rejected

- **GIVEN** bob already in the member list
- **WHEN** a manager adds bob again
- **THEN** the operation fails before any write (`add_workspace_member`, crates/opake-core/src/opake.rs)

### Requirement: Removal rotates the group key; leave does not

Removing a member SHALL rotate: the authoring manager mints a new group key, re-wraps it for every remaining member, bumps `rotation`, and pushes the prior rotation's members into `keyHistory` so existing documents stay readable (`spec:document-crypto § Keyring reads select the group key by the document's rotation`). The removed member never sees the new key — that is the forward-secrecy contract, bounded by the no-historical-revocation posture (removed members keep whatever they already had).

Leave SHALL NOT rotate. The leaver authors the supersede, so any key minted in it is a key the leaver knows — rotation there costs a rotation number and buys nothing. A leave carries the remaining members' wraps, the rotation counter, and the key history verbatim, dropping only the author's entry. Forward secrecy against a departed member arrives with the next manager-authored rotation; `remove_workspace_member` covers the uncooperative case.

#### Scenario: removal locks out future content

- **GIVEN** bob removed by a manager, rotation bumped from n to n+1
- **WHEN** a document is uploaded under rotation n+1
- **THEN** bob holds no wrap for rotation n+1 and cannot decrypt it, while remaining members read rotation-n documents via `keyHistory`

#### Scenario: leave carries everything but the author

- **WHEN** bob (editor) leaves
- **THEN** the written record has the prior rotation, the prior key history, and every other member's wrap and role unchanged
- Test: `leave_workspace_writes_self_removal_supersede` (crates/opake-core/src/opake_tests.rs)

### Requirement: Leave guards — no orphaned workspaces

The last member SHALL NOT leave: an empty member list is workspace destruction, which is deliberately unsupported (workspace-identity spec, open questions). The only manager SHALL NOT leave while other members remain: a manager-less workspace can never mutate membership again. The manager must promote someone first.

#### Scenario: last member blocked

- **GIVEN** alice as the sole member
- **WHEN** alice attempts to leave
- **THEN** the operation fails with a destruction-is-unsupported error
- Test: `leave_workspace_rejects_last_member`

#### Scenario: only manager blocked

- **GIVEN** alice (manager) and bob (editor)
- **WHEN** alice attempts to leave
- **THEN** the operation fails, directing her to promote another member first
- Test: `leave_workspace_rejects_only_manager`

### Requirement: Role changes are manager-authored supersedes

A manager changes a member's role by writing a supersede carrying the prior member list with only the targeted member's role changed. Roles of untargeted members SHALL carry forward unchanged across every membership mutation.

#### Scenario: promote an editor

- **GIVEN** alice (manager) and bob (editor)
- **WHEN** alice re-roles bob to manager
- **THEN** the written supersede holds both members with bob as manager and alice untouched
- Test: `update_member_role_writes_supersede_with_updated_role` (opake_tests.rs)

## Open questions

- Rotation after leave: forward secrecy against a leaver requires a manager to rotate afterwards. Should a manager's daemon rotate automatically on observing a leave? Related to auto key rotation (#90) and the unreliable bulk re-encryption noted in open work — a rotation whose re-encryption doesn't complete degrades to the historical-key fallback.
- Atomic hand-over: a lone manager who wants out of a populated workspace must promote-then-leave as two supersedes, with a fork-race window between them. Is a combined "transfer management and leave" operation worth its own primitive?
- Viewer enforcement surface: viewers are excluded from directory and keyring authorship (this spec + directory-chains), but read access is capability-based (holding a wrap), not checked per-operation. Confirm there is no surface where a viewer's write would be accepted.

## Non-requirements

- Genesis identity, AEAD wrap anchoring, head-vs-genesis resolution — workspace-identity spec.
- What editors may do to directory contents (wiki-semantics additivity) — directory-chains spec.
- The wrap algorithm, key hierarchy, and rotation-aware decryption mechanics — document-crypto spec.
- Person-to-person sharing and invitations — sharing-grants spec (`spec:sharing-grants § Invitation targets hold the stable resource id`).
- Workspace destruction — deliberately unspecified; see workspace-identity open questions.
