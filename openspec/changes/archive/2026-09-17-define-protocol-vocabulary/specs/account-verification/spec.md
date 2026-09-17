# account-verification (delta)

## MODIFIED Requirements

### Requirement: Resolution reads the anchor's history and reports a replacement

A signature verifies against whatever key the DID document currently names, so a PLC rotation-key
holder can replace the verification method with a key it controls, re-sign a
substituted bundle under it, and resolve as verified. The signature is sound; the anchor moved.

Where the DID method provides an operation audit history, resolving a verified account SHALL read it and
determine whether the `#opake` verification method has ever been replaced with a different key. A
replacement SHALL be reported alongside the verified state: verification succeeds, and the caller
is told the anchor changed. The signing key derives from the seed phrase, so re-anchoring after
migration republishes the same value — a removal and re-addition of the same key is not a
replacement, and today no legitimate cause for a genuine replacement exists.

For `did:plc`, the audit read SHALL include accepted operations on nullified branches: a
replacement is an ever-observed security notice, while the current DID document remains
authoritative for verification. If the audit transport is unavailable, resolution SHALL remain
verified and report that replacement history is unavailable rather than claiming no replacement.
The history SHALL be read at resolution time and cached under the same expiry as the rest of
resolution. No record of previously observed verification methods SHALL be kept: the history is
public and authoritative, and reading it covers replacements that predate the caller's first
contact with the account, which a remembered value cannot.

This does not reach an anchor that was never legitimate. A rotation-key holder that publishes a key
it controls before the account first anchors leaves a history with no replacement in it, and the
account resolves as cleanly verified. First contact is outside what any in-band mechanism can
establish.

Where the DID method publishes no operation history, no replacement is reported and the account
resolves as verified on its current document alone. `did:web` is such a method: its document is
served over HTTPS with no log behind it. Resolution SHALL NOT represent the absence of a history
as the absence of a replacement, and SHALL distinguish an account whose history shows no
replacement from one whose method offers no history to read.

When identity rotation exists, a legitimate change of signing key will need a statement signed by
the outgoing key rather than a bare substitution; until then the distinction does not arise.

#### Scenario: a replaced anchor is reported despite a valid signature

- **GIVEN** a `did:plc` account whose rotation-key holder replaced its `#opake` verification method
  with a key it controls and re-signed the published record under it
- **WHEN** a counterparty resolves the account
- **THEN** the signature verifies, and the caller is additionally told the verification method was
  replaced

#### Scenario: a method with no history cannot report a replacement

- **GIVEN** a `did:web` account carrying an `#opake` verification method and a record that verifies
- **WHEN** a counterparty resolves the account
- **THEN** it resolves as verified, and the caller is told the method publishes no history rather
  than told that no replacement occurred

#### Scenario: an anchor dropped and republished is not a replacement

- **GIVEN** an account whose verification method was removed and later republished carrying the
  same key
- **WHEN** a counterparty resolves the account
- **THEN** the history shows the same key restored, and nothing is reported

### Requirement: Recipients are resolved independently and a multi-recipient operation never fails wholesale

An operation that wraps a key to another account SHALL resolve each recipient's verification state
independently, and its disposition on the error state SHALL depend on whether the recipient is the
operation's subject or one of several beneficiaries.

Where an operation has a **single recipient** — admitting a member, creating a grant, completing a
pair — the error state SHALL refuse the operation before any wrap is computed.

Where an operation wraps to **every remaining member** — a group-key rotation — the error state
SHALL exclude that member from the wrap and SHALL NOT prevent the operation. An operation whose
purpose is to withdraw access MUST NOT be blockable by any account it is not withdrawing access
from; otherwise a single PDS operator serving its own user a record that resolves to the
error state would permanently prevent the removal of anyone else. The excluded member SHALL
be reported to the operator, and SHALL be eligible for repair once their record verifies
(`spec:background-work § Remaining work is derived from records, never stored`).

An unverified remaining member whose resolved encryption keys lack applicable key-bound approval
SHALL likewise be excluded from the new wrap, without delaying the withdrawal for confirmation.
This is a pending decision, not a fourth resolution state or a verification error. Both kinds of
exclusion SHALL retain membership and historical access; only the intended removal drops a member
(`spec:workspace-membership § Membership state is the keyring head's member list`).

A PDS operator can therefore deny its own user access to new material, which it could already do by serving
nothing at all. It cannot reach past its own user to block another account's operation.

#### Scenario: an unverifiable member does not block a removal

- **GIVEN** a workspace whose member Bob has a verification method and whose PDS serves a record
  that does not verify
- **WHEN** a manager removes a different member
- **THEN** the removal completes, the new group key is wrapped to every member whose keys verify,
  Bob is excluded and reported, and forward secrecy against the removed member holds

#### Scenario: a single-recipient operation refuses

- **GIVEN** a prospective grant recipient carrying an `#opake` verification method whose published
  record does not verify under it
- **WHEN** the owner shares a document to them
- **THEN** the operation is refused and no grant record is written

#### Scenario: declining to verify is never itself a refusal

- **GIVEN** a prospective grant recipient carrying no verification method
- **WHEN** the owner shares a document to them
- **THEN** the recipient resolves as unverified and the share proceeds on the owner's confirmation,
  because refusal attaches to a broken claim of verification and never to its absence
