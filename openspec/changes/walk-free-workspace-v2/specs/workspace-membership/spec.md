## ADDED Requirements

### Requirement: Membership writes are compare-and-swap on the superseded record

A membership write SHALL name the exact record it supersedes (`supersedesCid`), and that pin SHALL act as an optimistic concurrency condition on the superseded record: the write wins the head position only if the named parent is still the current head when the write is evaluated. This condition is evaluated **retrospectively by an independent observer**, not synchronously at write time. The write is authored unconditionally to the author's own PDS — which is untrusted and merely stores bytes (`spec:workspace § The trust surface is four-tiered, and time is trusted nowhere`) — so nothing gates it locally; whether it won is learned when an observer echoes the competing-supersede set (`§ A membership write is confirmed only by an independent observer`; `spec:indexer-consistency § The write echo carries the compare-and-swap verdict`). A write that lost — its parent was superseded before the write was observed — is surfaced and MAY be replayed against the new head. No load-bearing write-time gate exists: the author's own PDS cannot enforce the condition (it does not hold the global chain head, which lives across other members' PDSes), and the indexer SHALL NOT be that gate (`spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`). "Won" is always relative to the set of competing records an observer has seen: because omission is invisible and agreement only eventual (`spec:workspace § Stated limitations no construction removes`), a write judged winning against one observed set can still be displaced when a wider set is observed, until it is built upon (`§ A membership write is confirmed only by an independent observer`).

This "compare-and-swap" names the superseded-record head condition; it is deliberately NOT the PDS-level `swapCid`/`swapRecord` optimistic write of `spec:background-work § Concurrency is resolved per record by compare-and-swap`, which conditions on a single record's own CID at its own PDS. The membership guard is a cross-PDS, retrospectively-evaluated head condition; the two mechanisms share a name and nothing else.

This is discard-and-retry, not merge: a raced membership write loses and is retried, never folded. It is deliberately scoped to membership. Document-tree writes keep their additive-merge semantics (`spec:tree-chains`) and are not governed by this rule.

The retry is self-correcting for the case that motivates it: if the author lost the race because they were concurrently removed, their replay against the new head fails its own authority check, because the new head's roster no longer lists them.

#### Scenario: a raced add loses and is replayed

- **GIVEN** two managers who both author a supersede naming head H as parent
- **WHEN** an independent observer echoes both, showing the first has superseded H
- **THEN** the second write loses the head, its author is told via the echoed verdict, and the client may replay the add against the new head

#### Scenario: a removed author's replay fails authority

- **GIVEN** a manager removed on the winning branch, whose own concurrent write lost the compare-and-swap
- **WHEN** the client replays the losing write against the new head
- **THEN** the replay is rejected by the authority check, because the new head's roster no longer lists the author as a manager

### Requirement: A rejected membership write is surfaced, never silently discarded

A membership write that is refused — by the compare-and-swap guard, the fork-timing ceiling, or an authority check — SHALL be surfaced to the human who initiated it. The client SHALL NOT drop a rejected write silently and SHALL NOT present an optimistic view that implies the write succeeded. Holding the intent and telling the human it did not land is a protocol obligation, not a UI nicety.

#### Scenario: a discarded write reaches the user

- **WHEN** a membership write is refused for any reason
- **THEN** the initiating user is informed that it did not take effect, and no projection shows it as applied (`spec:indexer-consistency § Client projections contain only indexer-confirmed state`)

### Requirement: A membership write is confirmed only by an independent observer

A membership write SHALL be treated as pending until it is echoed back from a party that is not the author's own PDS. The author's own host returning success proves nothing, because that host is untrusted (`spec:workspace § The trust surface is four-tiered, and time is trusted nowhere`). Qualifying independent observers are the indexer, a second relay, another member's client, or the author's own client observing the record on the public firehose — so a member with no indexer can still confirm. Confirmation SHALL NOT be defined as "the indexer has seen it", because that would make the indexer necessary for writing.

The echo MAY carry the compare-and-swap verdict — the set of records the observer has seen superseding the same parent — so the client reaches its own conclusion rather than trusting the observer's (`spec:indexer-consistency § The write echo carries the compare-and-swap verdict`).

Confirmation is not terminal. An echo establishes that a write was *seen*, not that it won for good: a confirmed membership write MAY still be displaced — transition to `lost` — if a competing record superseding the same parent is observed later, until the write is built upon by a live descendant. The built-upon floor that makes a removal durable governs every membership write (`§ Removal is durable once built upon, not merely witnessed`). A client SHALL treat a confirmed-but-not-yet-built-upon membership write as displaceable, and SHALL surface a post-confirmation displacement exactly as it surfaces an initial loss (`§ A rejected membership write is surfaced, never silently discarded`). Detecting a displacement requires observing the competing record; where a client reaches no indexer that observation is best-effort, the same liveness reality under which an indexer is recommended (`spec:workspace-sequencing § The log is never necessary for truth`).

#### Scenario: a confirmed write is later displaced

- **GIVEN** a membership write confirmed by an independent observer and not yet built upon
- **WHEN** a competing write superseding the same parent is observed later and wins head selection
- **THEN** the confirmed write transitions to lost, the displacement is surfaced to its author, and no projection continues to show it as applied

#### Scenario: own-host success is not confirmation

- **WHEN** a member's PDS accepts a membership write and the member has no independent echo
- **THEN** the write is pending, and the client presents it as pending rather than applied

#### Scenario: firehose self-observation confirms without an indexer

- **GIVEN** a self-hosting member running no indexer
- **WHEN** the member observes their own membership record on the public firehose
- **THEN** the write is confirmed, because the firehose is an observer other than the author's own PDS

### Requirement: The fork-timing ceiling is measured against the frontier

A membership write SHALL be refused if the record it supersedes has fallen more than a bounded distance behind the live frontier, even when it would otherwise win. "Behind" is measured as the write's parent's position relative to the current head, established against a freshness beacon (`spec:workspace-sequencing § A signed tree head is the freshness beacon`) — never as the author's elapsed offline time, and never from a record timestamp.

The distinction is load-bearing: a member gone a month in a quiet workspace whose parent is still the head is not late and is accepted; a write rooted far behind a head that has moved on is discarded. This *bounds* the malicious fork-against-an-ancient-ancestor case to the freshness of the beacon-delivery channel: a signed tree head establishes a frontier position, not proof that the position is current (`spec:workspace-sequencing § A signed tree head is the freshness beacon`), so a verifier fed only stale beacons is bounded by how stale they are, not fully closed. Where no beacon is available but the verifier still holds a live frontier, the ceiling degrades to the structural defence — a member holding a live frontier refuses a fork rooted behind it — preserving correctness while losing the tightened bound (`spec:workspace-sequencing § The log is never necessary for truth`). Where the verifier holds *neither* a live frontier nor a beacon, the structural defence is absent, not merely degraded: the memoryless verifier has no anchor against which to measure lateness (`spec:workspace § Stated limitations no construction removes`).

#### Scenario: a month-old write into a quiet workspace is accepted

- **GIVEN** a workspace with no membership changes since member M's parent record
- **WHEN** M writes a supersede naming that still-current parent after a month offline
- **THEN** the write is accepted, because its parent is still the head — absence is not lateness

#### Scenario: a write rooted far behind a moved head is discarded

- **GIVEN** a workspace whose head has advanced well beyond record R
- **WHEN** a write supersedes R, rooted more than the ceiling behind the current head as measured against the beacon
- **THEN** the write is refused as late, independent of any timestamp it carries

### Requirement: Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably

When a single current record must be chosen among competing valid heads (a fork), selection SHALL proceed in this order:

1. **Frontier-scoped endorsement (primary).** Prefer the branch endorsed by the most distinct members who are managers **in the verifier's current frontier roster** — the newest membership state the verifier has already verified — and who authored a record on that branch. The electorate is the *current* roster, not the roster at the fork's base: an author counts only if they are a manager now. So a member removed before the fork — including a since-removed sockpuppet — contributes nothing even when the fork is rooted at an ancient record where they were still a manager, and an honest manager added recently *does* count. This is sound only because records are author-signed, which makes "a distinct member built on this" unforgeable (`spec:record-signatures § Every workspace record carries an author signature`). Quantity of self-asserted entries — roster size, number of removals — SHALL NOT count.

   Anchoring the electorate to the verifier's frontier is what makes endorsement Sybil-resistant, and it is the same anchor the fork-timing ceiling uses: a fork rooted far behind the frontier is refused by the ceiling before endorsement is consulted (`§ The fork-timing ceiling is measured against the frontier`). Together they mean a verifier holding a live frontier (or a fresh beacon) cannot have its endorsement electorate dragged backward by where an attacker roots a fork. A verifier with **neither** a frontier nor a beacon — a device that lost its cache and reaches no indexer — has no electorate anchor and is exposed to endorsement inversion, exactly as it is exposed to stale state. This is the accepted memoryless-victim limitation (`spec:workspace § Stated limitations no construction removes` — fork-danger and staleness-danger are one problem), not a separate defence, and is not defended here.

2. **Ungrindable tie-break at equal endorsement, anchored at the common ancestor.** The tie-break SHALL NOT be the record CID (a hash of author-chosen bytes, mineable by whoever computes more hashes). Competing heads may diverge at different depths — a stale-rooted head competing with an advanced one — so the fork SHALL be resolved at the **most-recent common ancestor (MRCA)** of the full competing head-set, and the records compared SHALL be the branch-root records that directly supersede the MRCA. Each fork-eligible record carries a VRF (verifiable random function) output over its own `supersedesCid`, computed with the author's roster-carried VRF key (`§ The roster carries each member's signing key`); because each branch-root supersedes the MRCA, its `supersedesCid` **is** the MRCA CID, so all contenders' VRF outputs share one input and are comparable, and the branch whose root has the lowest verified VRF output wins. A VRF output is unique per `(key, input)`: the author cannot search for a favourable one, the input is not author-controlled, and every observer verifies the proof and computes the same winner offline, with no sequencer and no external entropy. A record carrying no valid VRF proof SHALL rank after every record that carries one, so omitting the proof never improves an author's odds. Comparing each head's own `supersedesCid` directly (which differ across depths) is non-conforming.

3. **Degraded fallback.** Only where no competing branch-root carries a valid VRF proof — a transitional state, or a member without a VRF key — SHALL the winner be the lowest CID, a known-grindable fallback, explicitly not a trust boundary.

Because losing a tie costs only a retry (`§ Membership writes are compare-and-swap on the superseded record`) and a tie is broken the moment a real member endorses either branch, the tie-break governs only the instant before endorsement — and the VRF makes even that unbiasable. The residual way to influence it is to mint a low-VRF identity and add it, which is a witnessed membership change contributing zero endorsement in the current frontier roster: closed on the endorsement axis before the tie-break is consulted.

#### Scenario: a since-removed sockpuppet does not win via an ancient fork base

- **GIVEN** a hostile manager who roots a branch at an ancient record whose roster still listed sockpuppets the workspace later removed, and has those sockpuppets author on it; and an honest branch built on by managers added after that ancient record
- **WHEN** a verifier holding the current frontier runs head selection
- **THEN** the sockpuppets contribute nothing — they are not managers in the current frontier roster — and the honest recent managers count, so the honest branch wins; the attacker's choice of fork depth does not move the electorate

#### Scenario: cross-depth forks compare at the common ancestor

- **GIVEN** competing heads at different depths — one superseding record H, another superseding a descendant of H — whose most-recent common ancestor is M
- **WHEN** the tie-break runs
- **THEN** the records compared are the two that directly supersede M, whose `supersedesCid` is M's CID, so their VRF outputs share one input and are comparable

#### Scenario: grinding record bytes does not steal the tie

- **GIVEN** two equally-endorsed forks, one whose author pads and reorders content searching for a low CID
- **WHEN** the tie is broken
- **THEN** the winner is the lowest VRF output over the common-ancestor CID, independent of record content, so the grinding confers no advantage

#### Scenario: omitting the VRF proof does not help

- **GIVEN** a fork whose branch-root omits a VRF proof to dodge an unfavourable output
- **WHEN** it competes against a branch-root carrying a valid VRF proof at equal endorsement
- **THEN** the proof-carrying branch wins, because a record without a valid proof ranks after every record that has one

### Requirement: Removal is durable once built upon, not merely witnessed

A removal is a membership write, and its durability follows the same liveness reality as any write — not a cryptographic instant. Three bars must not be conflated: a removal is **observed** once an independent party echoes it (`§ A membership write is confirmed only by an independent observer`); it is **durable** only once a live descendant record supersedes it (`spec:keyring-tombstones § Rollback restores the newest live record and re-broadcasts it`), or while the remover's host stays honest. Observation is not durability, and the removing manager's client SHALL NOT present a removal as complete-and-durable on observation alone.

Because the removal record lives on the remover's own — untrusted — PDS and is signed by the remover (so no other party can republish it as the same record), that host can revert the removal by deleting it: the head delete rolls back to the pre-removal record, reinstating the member with the pre-rotation group key. This is the same liveness attack as a host accepting a write and then withholding it (the PDS→relay lie): detected, not prevented. The removing manager's client SHALL watch the resolved head for the removed member reappearing and re-issue the removal; a persistently hostile remover-host is escaped only by account migration, the standing human remedy for a hostile host. The revert is surfaced, never silent (`§ A rejected membership write is surfaced, never silently discarded`).

**Forward-secrecy gate.** Content that must exclude the removed member SHALL NOT be committed under the post-removal epoch until the removal is durable: encrypting under the new key before durability risks orphaning that content if the removal is reverted (the new key lives only in the not-yet-durable record), and encrypting under the old key leaks it to the removed member. Forward-secure writes therefore wait for durability. This bounds a revert's blast radius to "the member briefly reappears," not "the member reads new content."

**Indexer ceiling (availability help, not a truth dependency).** Where an indexer is present it retains the signed records it ingests and continues to serve an ingested removal record after the author's host deletes it, so the removal stays verifiable and the head does not roll back past it (`spec:keyring-tombstones § Rollback restores the newest live record and re-broadcasts it`). This closes the offline-remover and persistent-revert gaps in practice. It is an availability enhancement, not a correctness dependency: the served record is signed, so a client verifies it exactly as from any host and trusts the signature, not the indexer (`spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`); a client with no such indexer, or whose indexer lacks the record, falls back to the built-upon durability floor above. General member-to-member replication of records — which would make removal cryptographically durable without an indexer — is out of scope here ([#19](https://github.com/Opake-at/Opake/issues/19)).

Rotation mechanics are unchanged (`spec:workspace-key-rotation § The rotation event is synchronous and self-sufficient`); this requirement governs when the remover may rely on the removal, and what the workspace may safely encrypt in the meantime. It replaces the earlier "effective when witnessed" framing, which conflated observation with durability.

#### Scenario: a hostile remover-host reverts a witnessed-but-unsuperseded removal

- **GIVEN** manager Alice removes member M in head record B (which also rotates the key), B is observed by other parties but no live record yet supersedes it
- **WHEN** Alice's own hostile PDS deletes B and the head rolls back to the pre-removal record
- **THEN** the removal is reverted — a detected liveness attack, not a silent loss: Alice's client observes M reappear on the resolved head and re-issues, and durability is reached only once a live descendant supersedes the removal, the indexer holds the record available, or Alice migrates off the hostile host

#### Scenario: forward-secure writes wait for durability

- **GIVEN** a removal that is not yet durable
- **WHEN** a member would upload content that must exclude the removed member
- **THEN** it does not commit under the post-removal epoch until the removal is durable — neither orphaned under an unwitnessed new key nor leaked under the old key

#### Scenario: the indexer holds a removal available for an offline remover

- **GIVEN** a removal ingested by an indexer, whose author then goes offline, and whose host deletes the removal record
- **WHEN** a client resolves the workspace through that indexer
- **THEN** the indexer serves the signed removal record it retained, the client verifies its signature, the head does not roll back past it, and a client with no such indexer falls back to the built-upon durability floor

### Requirement: The roster carries each member's signing key

The workspace roster SHALL carry each member's public identity keys — their signing key and their VRF key (or commitments to them) — alongside their DID and role, so that adding a member is itself the act of attesting both. Signature verification reads the signing key from the roster the verifier already holds (`spec:record-signatures § Signature verification uses the roster-carried key, with no external lookup`); the VRF key is read from the same roster to break fork ties ungrindably (`§ Head selection is endorsement-weighted, frontier-scoped, and tie-broken ungrindably`). Key provenance is internal to the workspace and needs no external DID-document lookup (`spec:workspace-identity § The roster is the workspace key registry`); both keys are attested at add time and served from the roster, never re-fetched.

A member's signing and VRF keys SHALL be immutable once attested. They derive from the member's mnemonic on distinct derivation paths and do not rotate (identity rotation does not exist — `spec:workspace-key-rotation`; [#18](https://github.com/Opake-at/Opake/issues/18)). Every supersede SHALL carry each continuing member's keys forward byte-identical, and any supersede that alters, drops, or substitutes a continuing member's signing or VRF key SHALL be rejected by the authority check — indexer at ingest and client on verification alike. Only an add introduces a new `{did, signing key, VRF key}` binding. Without this, a supersede that is otherwise authorised — a pure self-removal, or a manager's role change — could rewrite a *remaining* member's signing key and thereby forge that member's future authorship, since verification roots in the roster-carried key; the immutability rule closes that escalation, and extends it to the VRF key so a member's tie-break identity cannot be swapped either.

#### Scenario: adding a member attests their signing key

- **WHEN** a manager adds a member
- **THEN** the superseding keyring record carries the new member's signing key in the roster, and thereafter that member's records verify against it with no external fetch

#### Scenario: a supersede that rewrites a continuing member's signing key is rejected

- **GIVEN** a head listing manager alice with signing key K
- **WHEN** any member authors a supersede whose roster keeps alice but carries a signing key other than K for her
- **THEN** the supersede is rejected — a continuing member's signing key is immutable across supersedes — even if the supersede is an otherwise-valid self-removal or role change

### Requirement: Re-adding a removed member rebinds its prior key

Key immutability (`§ The roster carries each member's signing key`) protects *continuing* members; a removal breaks the binding, so re-adding a previously-removed DID is a fresh attestation that rule alone does not constrain. Without a further constraint a hostile manager could re-admit a removed DID under a key the manager controls — authoring as that member and silently dropping that member's genuinely-signed records, which would then fail to verify against the substituted key. To close that, a re-add of a DID the workspace lineage records as a prior member SHALL bind the same signing and VRF keys that DID last held. An add that reintroduces a previously-known DID with different keys SHALL be rejected by the authority check wherever the prior binding is recoverable from the lineage the verifier holds, and the manager constructing the re-add — who holds that history — SHALL carry the prior keys forward.

Binding a *new* key to a returning DID is identity rotation, which does not exist (`spec:workspace-key-rotation`; [#18](https://github.com/Opake-at/Opake/issues/18)): a member who lost their mnemonic returns under a new DID, not a rebound old one. A verifier whose held lineage does not reach the prior binding cannot enforce this and falls back to trusting the attesting manager — the same social-trust floor every add already rests on (`spec:workspace § The trust surface is four-tiered, and time is trusted nowhere`).

#### Scenario: re-adding a removed member under an attacker key is rejected

- **GIVEN** a DID that was previously a member with signing key K, since removed, whose prior binding is present in the lineage the verifier holds
- **WHEN** a manager authors an add reintroducing that DID with a signing key other than K
- **THEN** the add is rejected, because a re-added DID rebinds the key it last held, not a new one

#### Scenario: a genuine re-add carries the prior keys forward

- **WHEN** a manager re-adds a previously-removed member
- **THEN** the add carries that member's prior signing and VRF keys, and the member's existing signed records continue to verify

## MODIFIED Requirements

### Requirement: Keyring supersede authority is manager-only, except pure self-removal

A keyring supersede SHALL be valid iff the author is currently a manager, OR the author is a non-manager member and the supersede is a pure self-removal: the new member list equals the head's list minus the author, compared on `{did, role, signing key, VRF key}` — every remaining member's role and identity keys carry forward byte-identical (`§ The roster carries each member's signing key`). Under the exception, dropping anyone else, adding anyone, changing any remaining member's role, altering any remaining member's signing or VRF key, or keeping oneself in the list SHALL be rejected. Wrapped-key bytes are not compared — they legitimately differ across supersedes; signing and VRF keys are not wrapped-key bytes and ARE compared.

The rule SHALL be enforced in the indexer (`check_keyring_supersede/4` + `pure_self_removal?`, authority.ex) and re-checked client-side. The two checks express the same rule, but the indexer's is enforcement at ingest, not a trusted authority: because records are author-signed (`spec:record-signatures § Every workspace record carries an author signature`), a client verifies the author's role against the roster and the author's signature itself, and where the client's own verification and the indexer's acceptance disagree, the client's verification governs the client's behaviour (`spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`). The indexer's ingest check keeps unauthorised writes out of the pipeline; it is not the source of truth the client defers to.

#### Scenario: editor leaves

- **GIVEN** a head with alice (manager), bob (editor), carol (viewer)
- **WHEN** bob authors a supersede whose members are exactly alice (manager) and carol (viewer)
- **THEN** the supersede is accepted
- Tests: `editor leaving passes` and siblings, apps/indexer/test/opake_indexer/authority_db_test.exs

#### Scenario: self-removal that smuggles a change

- **WHEN** bob's supersede also drops carol, re-roles carol, or adds a new member
- **THEN** it is rejected with insufficient role
- Tests: `editor dropping someone else alongside themselves is rejected`, `editor re-roling a remaining member while leaving is rejected`, `editor adding a member while leaving is rejected` (authority_db_test.exs)

#### Scenario: client verification governs on disagreement

- **GIVEN** a signed keyring supersede that the indexer accepted but whose author does not hold the required role in the roster the client verifies against
- **WHEN** the client evaluates the record
- **THEN** the client rejects it on its own verification rather than deferring to the indexer's acceptance

### Requirement: Adding a member is a manager-authored supersede

A manager adds a member by attesting the member's identity keys into the roster and wrapping the current group key to the recipient's published hybrid public keys, appending both to a superseding keyring record. Attesting the member's keys is part of the add, not a separate step: the superseding record's roster carries the new member's `{did, role, signing key, VRF key}` binding (`§ The roster carries each member's signing key`), copied once from the member's `publicKey/self` at add time (`spec:workspace-identity § The roster is the workspace key registry`). An add that wraps the group key but does not attest the member's signing and VRF keys is incomplete and SHALL be rejected — there is no keyless admission path. Adding a DID already in the member list SHALL be rejected; re-adding a previously-removed DID rebinds its prior keys (`§ Re-adding a removed member rebinds its prior key`). The wrap's AEAD anchor is the genesis URI (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`); the role is assigned at add time.

Direct manager add is the only admission channel: no invitation, request-to-join, or other self-service path exists.

#### Scenario: duplicate add rejected

- **GIVEN** bob already in the member list
- **WHEN** a manager adds bob again
- **THEN** the operation fails before any write (`add_workspace_member`, crates/opake-core/src/opake.rs)

#### Scenario: an add that omits the member's keys is rejected

- **WHEN** a manager authors an add whose roster wraps the group key to a new DID but carries no signing or VRF key for it
- **THEN** the supersede is rejected, because attesting the member's keys is part of the add and no keyless admission path exists
