## ADDED Requirements

### Requirement: Recipient resolution has finite independent budgets

A removal's recipient-resolution phase SHALL use finite per-recipient and overall budgets
with bounded concurrency. A nonresponsive remaining recipient SHALL NOT indefinitely delay
another member's removal. When a budget expires, the client SHALL stop waiting for the
affected work and continue with recipients eligible under the verification and approval rules.
Late responses SHALL NOT mutate the completed operation's recipient decisions.

The result SHALL distinguish verification failure, unreachable/timed-out resolution,
pending approval, and recipients not attempted before the overall budget expired. Lack of
an attempt SHALL NOT be evidence that a recipient's host is unreachable or hostile. Each
retained member missing the new wrap SHALL enter or retain the membership grace period.

These budgets bound recipient resolution, not the availability of the authority chain or
the ability to commit on the author's PDS. Confirmed failure before a membership commit
SHALL report an author error with no membership or rotation change and no notification to
the removal target. An uncertain commit result SHALL be reconciled, not labelled no-commit.

#### Scenario: one recipient never responds

- **GIVEN** a manager removes Bob while Carol's recipient lookup never returns
- **WHEN** Carol's lookup budget expires and the author's commit succeeds
- **THEN** the operation does not wait indefinitely, Bob's removal can take effect, and Carol is retained without the new wrap, with a deadline and a timeout report

#### Scenario: the overall budget expires before every lookup starts

- **WHEN** bounded concurrency leaves recipients unattempted at the overall deadline
- **THEN** those recipients are reported as unattempted within budget, not as failed verification or proven unreachable hosts

#### Scenario: author write definitely does not commit

- **WHEN** the removal fails with confirmed no commit on the author's PDS
- **THEN** the author receives an error, the old canonical membership and rotation remain in force, and no removal event is produced for Bob or other members

## MODIFIED Requirements

### Requirement: The re-wrap sweep is hygiene under the background-work contract

Migrating existing documents' content-key wraps from historical group keys to the current
one SHALL be a background task conforming to `spec:background-work` in full: work set
derived per item by comparing a wrap's rotation to the keyring head at write time, each
re-wrap a single CAS-conditioned record write, duplicate runners harmless, completion
never required for correctness. The sweep reduces use of historical wraps; it has no
revocation effect — a re-wrap cannot revoke anything a former member could already unwrap.

Before replacing a document's sole content-key wrap, the runner SHALL check that every
currently admitted member has a usable wrap for the target group-key rotation. If any
admitted member lacks that wrap, the runner SHALL defer the document re-wrap and preserve
its existing historical access path. Deadline expiry, attempted removal, and a removal on
a losing branch SHALL NOT count as absence from canonical membership.

The runner SHALL re-evaluate the current head and eligibility for each item, and discard
stale work when a newly observed rotation, repair, removal, or rollback changes the
decision. Document CAS protects the document, not a cross-PDS membership snapshot; it
SHALL NOT be described as an atomic check of a foreign keyring head. An implementation
SHALL demonstrate the access-preservation rule under concurrent membership changes before
enabling destructive single-wrap replacement; otherwise that item remains deferred.

#### Scenario: interrupted sweep needs no recovery

- **WHEN** a sweep is interrupted with half a workspace's eligible wraps migrated
- **THEN** documents in both halves retain their supported read paths, and any later runner re-derives the remaining eligible work

#### Scenario: sweep write races a second rotation

- **WHEN** a sweep observes the keyring advance from rotation n+1 to n+2 before publishing its next item
- **THEN** it discards the n+1 plan and re-evaluates n+2 and its member wraps before writing, deferring if the new target would strand an admitted reader

#### Scenario: pending member retains an old document

- **GIVEN** Carol remains admitted, holds group key 7, and lacks the current key 8
- **WHEN** maintenance encounters a document whose sole content-key wrap uses key 7
- **THEN** it leaves that wrap unchanged, including after Carol's deadline if her removal is not yet canonical

#### Scenario: canonical removal releases the sweep gate

- **GIVEN** Carol's removal is canonical and every remaining member has a usable wrap for the current rotation
- **WHEN** maintenance re-evaluates the old document
- **THEN** Carol's former membership no longer blocks the re-wrap, without any claim that her cached historical access is revoked
