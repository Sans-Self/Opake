## ADDED Requirements

### Requirement: Missing current wraps have a visible finite grace period

A member retained without the current rotation's wrap SHALL have a finite repair deadline
carried by authorized membership state. The deadline and the consequence of non-repair SHALL
be visible to managers and to the affected member when they observe that state. Admission
SHALL disclose that unresolved current-key access can lead to removal after a grace period.

The first canonical exclusion in a continuous missing-wrap period SHALL establish the
deadline. Further rotations, retries, device changes, or a change in exclusion reason SHALL
NOT restart it. Successful repair before expiry SHALL end that period. The deadline limits
pending repair, not the lifetime of otherwise applicable key-bound approval; elapsed time
SHALL NOT approve replacement keys or invalidate approval of unchanged keys.

Only a manager-authorized membership mutation SHALL establish or change the deadline.
Pure self-removal SHALL preserve every remaining member's deadline. Rollback SHALL derive
deadline state from the restored head, not a deleted or losing record or a local timer.

#### Scenario: another rotation does not renew grace

- **GIVEN** Carol lacks the current wrap and has deadline D in the canonical head
- **WHEN** another removal rotates the key while Carol remains excluded
- **THEN** her DID, role, approval, historical wraps, and deadline D are retained, without a fresh grace period

#### Scenario: another device observes the same deadline

- **WHEN** a member or manager opens the workspace on another device during grace
- **THEN** the device derives the same deadline from authorized records and reports historical-only access and the removal consequence, without needing the first device's timer

#### Scenario: timely repair ends grace

- **GIVEN** Carol's current wrap is missing and her grace period has not expired
- **WHEN** an authorized manager supplies that wrap under applicable verification or approval
- **THEN** the missing-wrap period ends without removing or re-admitting Carol

### Requirement: Expiry makes removal due but does not change membership

After the repair deadline, unresolved membership SHALL become due for ordinary
manager-authorized removal. A clock crossing the deadline SHALL NOT itself remove the DID,
change a role, withdraw record access, or authorize sweeping away historical readability.
The removal SHALL obey the current authority, rotation, and canonical-outcome rules.

If no authorized manager can execute it, or its write fails or loses a fork, the removal
SHALL remain overdue rather than be reported complete. Existing no-orphan constraints
SHALL NOT be bypassed by the expiry path. There is no guaranteed execution time after expiry.

Once removal becomes canonical, later recovery of verification or receipt of approval SHALL
NOT silently re-add the person. Return SHALL use ordinary fresh admission against current
membership and current recipient keys, with any required confirmation. A stale repair work
item SHALL NOT restore removed membership or reuse a previous admission's approval as consent.

#### Scenario: deadline passes with no manager online

- **WHEN** Carol's repair deadline passes while no authorized manager runs the removal
- **THEN** Carol remains admitted and historical-only, removal is overdue, and document hygiene remains deferred where it would take away her historical access

#### Scenario: removal must become canonical before cleanup proceeds

- **GIVEN** Carol is overdue for removal and still present in the canonical head
- **WHEN** a manager submits her removal but a competing membership mutation wins
- **THEN** Carol is not treated as removed and cleanup does not disregard her merely because the deadline or submission occurred

#### Scenario: return after expiry removal is a new admission

- **GIVEN** Carol's deadline-triggered removal is canonical
- **WHEN** her account becomes verifiable again
- **THEN** no background repair re-adds her; a manager must admit her anew using her current keys and any required confirmation

## MODIFIED Requirements

### Requirement: Keyring supersede authority is manager-only, except pure self-removal

A keyring supersede SHALL be valid iff the author is currently a manager, OR the author is a non-manager member and the supersede is a pure self-removal: the new member list equals the head's list minus the author, compared on `{did, role}` pairs. Under the exception, dropping anyone else, adding anyone, changing any remaining member's role, or keeping oneself in the list SHALL be rejected. Wrapped-key bytes are not compared — they legitimately differ across supersedes.

The self-removal exception SHALL additionally preserve each remaining member's wrap presence, key-bound approval, and missing-wrap deadline exactly. A non-manager SHALL NOT use a leave to add, replace, or erase approval, to change whether another member has a current wrap, or to set, clear, or extend another member's missing-wrap deadline. Capturing renewed approval and repairing a missing group-key wrap SHALL be manager-authored supersedes, subject to the same authority checks as admission (`spec:account-verification § Key-bound approval is carried by the relationship's records`). A background runner has only its acting account's authority, never a separate repair privilege.

The rule SHALL be enforced in the indexer (`check_keyring_supersede/4` + `pure_self_removal?`, authority.ex) and re-checked client-side for a fast, clear error before the write. The two checks express the same rule; the indexer's is authoritative.

#### Scenario: editor leaves

- **GIVEN** a head with alice (manager), bob (editor), carol (viewer)
- **WHEN** bob authors a supersede whose members are exactly alice (manager) and carol (viewer), with their wrap presence, approvals, and missing-wrap deadlines unchanged
- **THEN** the supersede is accepted
- Tests: `editor leaving passes` and siblings, apps/indexer/test/opake_indexer/authority_db_test.exs

#### Scenario: self-removal that smuggles a change

- **WHEN** bob's supersede also drops carol, re-roles carol, or adds a new member
- **THEN** it is rejected with insufficient role
- Tests: `editor dropping someone else alongside themselves is rejected`, `editor re-roling a remaining member while leaving is rejected`, `editor adding a member while leaving is rejected` (authority_db_test.exs)

#### Scenario: leaving does not authorize replacement keys

- **GIVEN** bob is an editor and carol's current entry has a missing wrap and approval for encryption bundle A
- **WHEN** bob leaves while changing carol's approval to bundle B or adding a wrap for her
- **THEN** the supersede is rejected as more than pure self-removal

#### Scenario: leaving cannot renew another member's grace

- **GIVEN** Bob is an editor and Carol has a missing-wrap deadline D
- **WHEN** Bob's leave clears or changes Carol's deadline
- **THEN** the supersede is rejected as more than pure self-removal
