## Context

Opake began as a storage problem: encrypt files so the host is irrelevant. Solved. Adding "shared" changed the species of problem — a workspace is an agreement about who is a member under which key, and agreement on this substrate has no arbiter. Everyone writes only to their own PDS, no global order exists, and both of the standard ways to manufacture order are off the table: a trusted sequencer contradicts end-to-end encryption's premise, and proof-of-work is absurd for the population (small activist orgs on whatever hosting they can get).

The shipped design tracks membership as a chain of `at.opake.keyring` supersedes, with the indexer enforcing authority at ingest and clients re-walking the chain as defence-in-depth. Prior-art survey and adversarial design work established the settled results this change relies on: workspace identity is the hash of a founding artifact; the freshness limit is a theorem, not a bug (with no trusted time and hostile hosts, a joiner can verify a state's authenticity but never its recency); per-recipient wrap sharding is real but its completeness cost is always understated; "who and in what causal order, never when" is the ceiling on auditability; and concurrent removal is the hard kernel. A history-free frontier/fold architecture was then taken to destruction against those constraints, and every fatal finding landed on the same rock: the fold has no fixpoint on mutual removal; the floor is either self-bricking or reversible; ratification cuts secretly need history; head selection is a gameable ownership lever; and a sealed frontier's anti-rollback guarantee lives on a host the threat model calls hostile.

This change specifies the synthesis of that work. Its through-line: the two moves that did the most work were *subtractions*. Deleting the requirement to preserve raced writes retired the mutual-removal paradox, ratification cuts and both attacks on them, and the permanent floor with its absurd "removed member can never rejoin" side effect. Decomposing "offline" into raced / stale / unwitnessed removed the duration-threshold confusion. What remains to *add* is small and mutually reinforcing: signatures, roster-as-registry, and an optional auditable log.

## Goals / Non-Goals

**Goals:**

- Define a workspace as a first-class concept with an explicit trust surface and an explicit list of limitations no construction of ours removes, so every other spec references it instead of re-deriving it.
- Make workspace records self-authenticating (member-key signatures) so authorship is a property of the bytes, not of where they sit.
- Make key provenance internal to the workspace (roster-as-registry), so signature verification needs no external DID-document fetch and works for `did:web` members with no audit log.
- Replace order-free merge with discard-and-retry compare-and-swap for membership writes, and make write-confirmation depend on an independent observer, with rejection always surfaced.
- Bound the fork window against a freshness beacon, and specify the auditable sequencer that provides one — as a layer that is never necessary for correctness.

**Non-Goals:**

- Redesigning the proven encryption layer (group-key wrap, content-key hierarchy, rotation-aware reads). Untouched.
- Identity rotation ([#18](https://github.com/Opake-at/Opake/issues/18)). Signing raises its stakes; it stays out of scope, flagged.
- Discard-and-retry for *document-tree* writes. Explicitly scoped to membership; document trees keep their additive-merge story.
- Membership privacy, historical-access revocation, malicious-member containment, real-time collaborative editing, protocol-level quota/billing. All remain non-goals.
- The workspace-as-its-own-account model (a workspace DID + repo). Declined for this population: it forces a funded, renewed, hosted account and publishes a rotation cadence anyone can watch. Recorded as a deployment-context fork, not adopted.

## Decisions

### D1 — A workspace is a definitional concept with a stated trust surface

The `workspace` spec is normative but conceptual: it fixes the vocabulary the rest of the change leans on. The trust surface has four tiers plus a special case:

- **Fully trusted:** the member's mnemonic and derived keys (compromise is currently unrecoverable — no rotation); the primitives; the member's own client.
- **Socially trusted:** other members within their roles. A member leaks what they can read; a manager admits the wrong person; an inviter can fabricate an entire workspace for a joiner. All accepted non-goals. The cold joiner's trust bottoms out at one human — it always did; the chain walk merely disguised it.
- **Semi-trusted:** the indexer/sequencer. May help, must never be *necessary* for truth.
- **Untrusted:** every PDS, including your own. Confidentiality is fine (ciphertext only); integrity was failing (unsigned records = authorship by shelf) and D2 fixes it; availability (hosts die) is a liveness concern.
- **Trusted nowhere:** time. Every timestamp is a self-serving claim.

The spec also enumerates the limitations no design removes, so they are cited rather than rediscovered: "newest" is unknowable from bytes (freshness is forever a liveness property); silence is invisible (omission is undetectable in principle); agreement is only ever eventual; history is mortal (anything that must survive lives in current records or members' heads); removal is the knife's edge; small groups get no arithmetic (no-owner + enforceable roles + no-coordination is jointly unsatisfiable at 2–3 people — exactly where our users live); the social graph is public.

*Alternative considered:* fold these into `workspace-identity` / `workspace-membership`. Rejected — the trust surface and the limitation list are referenced by *both* and by the two new capabilities; a shared conceptual home stops four specs from each carrying a partial, drifting copy.

### D2 — Records sign themselves

Every workspace record carries an Ed25519 signature from its author's member key, over the same canonical dag-cbor bytes the CID pins (which is why [#64](https://github.com/Opake-at/Opake/issues/64), byte-recomputed CIDs, is a hard prerequisite — you sign what the CID commits to, or the signature and the pin can disagree).

**Buys:** closes the host-forgery hole (a hostile host can no longer author records as its user); makes records *mirrorable* — a signed record verifies from any host, which dissolves the invite zero-churn window, the hard half of record custody ([#19](https://github.com/Opake-at/Opake/issues/19)), and much dead-host anxiety; makes endorsement-based fork resolution actually sound (unsigned, "another member built on this" is forgeable by that member's host — so signatures are a *prerequisite* for the endorsement repair, not an enhancement); makes equivocation provable (two signed records superseding the same parent are self-incriminating); gives the blind enforcer a sound check instead of a location presumption; lets invite tokens be signed.

**Doesn't touch:** freshness; forks themselves (a signature authenticates a disagreement, it does not resolve one); the first-fold durability window; removal semantics; small-n; membership privacy.

**Costs:** ~100 bytes per record; canonical-byte discipline (the [#64](https://github.com/Opake-at/Opake/issues/64) work); and it raises the stakes on key provenance ([#57](https://github.com/Opake-at/Opake/issues/57), answered by D3) and rotation ([#18](https://github.com/Opake-at/Opake/issues/18), out of scope).

The signed *cleartext governance envelope* — `{workspace tag, author DID, role/tier, epoch/lineage anchor, signature}` — is what lets the keyless enforcer reject a bad write soundly. This is in direct, known tension with full metadata confidentiality: a party that can *reject* a write must *read* enough to judge it. The synthesis established this is a contradiction in the capability list, not a design failure; we pay it deliberately, as the shipped design already does with cleartext member DIDs.

*Alternative considered:* keep location-authentication and lean harder on indexer enforcement. Rejected — it makes the client-side re-walk decoration (it only matches the indexer when there are no forks, which is the exact case it exists for), and it cannot survive the record leaving its author's host.

### D3 — The roster is the key registry

The workspace roster carries each member's signing key (or a commitment to it). Adding a member *is* attesting their key. Signature verification then reads the key from the snapshot it is already holding — no external lookup, nothing to be offline.

The forcing reason is `did:web`. It has **no audit log**: a `did:web` document is a JSON file at a URL with no proof of what it said yesterday. Everything that made PLC attractive for key provenance — ordered signed history, rotation keys, a recovery window — is a PLC feature. And `did:web` is over-represented among exactly our target population, because the same sovereignty instinct that picks a self-hosted identity picks Opake. So we cannot make key provenance depend on an identity-layer audit log that our core users don't have.

**Costs, accepted:** trust-on-first-use at add time (the same human trust the invite already rests on); and key rotation means updating every workspace you are in, rather than one directory entry. The roster is the sole source of a member's key — external DID documents aren't consulted (decided in Open Questions, with rationale).

*Alternative considered:* the split from the synthesis — inline the small constantly-needed Ed25519 verification key, pin-and-fetch the large occasionally-needed ML-KEM key. We adopt exactly this: the signing key inlines in the roster (D3), the KEM key stays wrapped/pinned as today. Signature verification never touches a member's PDS; wrapping *to* someone on rotation still fetches their KEM key, an irreducible O(members) cost priced honestly.

### D4 — Discard-and-retry replaces merge, scoped to membership

The capability revision that reshaped everything: a member's unavailability blocks no reads or writes *by other members*, but their own writes may be discarded, with recovery. This turns fork resolution from a **data-preservation** problem (which forces CRDTs, union folds, permanent floors, ratification cuts — all discarded here) into a **liveness** problem: optimistic concurrency control, i.e. a `git push` rejected as non-fast-forward, pull and replay.

A membership write names the exact record it supersedes. `supersedesCid` has *always* been a compare-and-swap condition — we built it for tamper-evidence and never used it as a concurrency guard. If the named parent has been superseded by the time the write is evaluated, the write is not accepted; the author's client is informed and may replay its intent against the current state.

**Retires, by deleting a requirement rather than adding machinery:** the mutual-removal fixpoint paradox (pick one branch deterministically; the loser's retry then fails its own authority check because they are now removed); concurrent adds being destroyed (they are retried, not lost); ratification cuts *and both attacks on them* (cut wars, and the lever where a manager races the emptiest cut to erase a departing member's work); the permanent floor, its unbounded growth, and its side effect that a removed member could never rejoin under the same identity. With one accepted line, the newest accepted record's roster *is* the roster.

**Does not touch:** the first-fold durability window (a removal on exactly one host that dies before anyone else sees it); freshness; deliberate malicious forks (still need an ungameable tie-break, but losing now costs a retry, so the rule can be dumber).

**Costs:** the client must hold the *intent*, replay it, and — non-negotiable — tell the human when a write did not land. Silent discard would be worse than any bug discussed. This promotes [#11](https://github.com/Opake-at/Opake/issues/11) ("in-flight writes die silently, optimistic UI lies") from web-app annoyance to protocol obligation.

**Scoping:** the revision is explicitly **membership-only**. Losing a membership change to a retry is nothing; losing a week of offline file edits is a different conversation, and document trees have their own additive-merge story (`spec:tree-chains`). Document-write scoping of the discard rule stays as the shipped additive model.

*Alternative considered:* the order-free fold (walk-free v1). Rejected in full — it has no fixpoint on mutual removal, its floor is either self-bricking or reversible, and its head selection is a gameable ownership lever. Discard-and-retry deletes the requirement those mechanisms existed to satisfy.

### D5 — "Offline" decomposed; confirmation by an independent observer

"Offline" is not a protocol-visible state. Three distinct notions hid in the word, and specifications must be about the *write*, never about the *member*:

1. **Raced** (write-side): your write's parent was superseded before your write was accepted. Evaluable only retrospectively. **No threshold on the author's absence** — a two-second race and a two-week absence are the same case; what matters is contention.
2. **Stale** (read-side): your view may lag reality and you cannot prove otherwise. Bounded only by liveness and source diversity.
3. **Unwitnessed** (durability): how many independent parties hold a fact. A removal on exactly one host is one host-death from never having happened.

From this, the confirmation rule: **a write is accepted when an independent observer has seen it.** Your own PDS returning 200 proves nothing — it is the untrusted party. A write is *pending* until echoed from somewhere that is not your own host. Qualifying observers: the indexer (ergonomic default), a second relay, another member's client, or the client watching the public firehose for its own record — so a self-hoster with no indexer can still confirm. Crucially **not** "until the indexer has seen it," which would make the indexer load-bearing for writing — the one property the adversarial work agreed must not break.

The echo can carry the *verdict*, not just a receipt: "I saw your record, and here are the other records I have seen superseding the same parent." That is the compare-and-swap acknowledgement, with the indexer still acting as an auditor — it reports the record set, the client applies the deterministic rule and reaches its own verdict.

Of the five network links between an author and a peer, only one lies to you: PDS→relay (the write is real and stored, the world never hears, and you got a success). That is exactly what a hostile host does deliberately. Under discard-and-retry, the client→own-PDS link fails loudly, relay→indexer self-heals via cursor, indexer→client becomes a cheap race, and peer-PDS reads are a verification concern. Independent-observer confirmation closes the one dangerous link. **Detection is not remedy:** a host that refuses to broadcast is detected (no echo arrives) but cannot be forced; the remedy is account migration, a human action.

### D6 — Fork-timing ceiling, measured against the frontier

Discard-and-retry leaves one hole open: nothing forces a writer to supersede the *newest* record, so a hostile manager can fork against a year-old ancestor at will, and a cold joiner with no memory cannot tell an ancient malicious fork from the real head. The contention rule (D4) alone does not close this.

The ceiling: a write rooted more than a bounded distance behind the live frontier is discarded — even if it would win the contention rule. **"Late" is measured as how far behind the live frontier the write's parent has fallen, not how long its author was offline.** These come apart cleanly:

- Quiet workspace, member gone a month: nothing superseded their parent, so their parent still *is* the head; their write supersedes the current frontier; not late; accepted.
- Busy workspace, parent superseded and now far behind: rooted behind a frontier that has moved on; discarded.

So contention still governs (consistent with D5), with an absolute ceiling bolted on top for the malicious-ancient-fork case. This **revises** an earlier "no duration threshold" framing: strike it, replace with "no threshold on the author's absence — but an absolute ceiling on how far behind the live frontier a write may be rooted."

The catch that ties D6 to D7: you cannot enforce "N behind the head" with timestamps, because time is trusted nowhere. "N behind the head" is only checkable if something authoritative recorded *when the head was at record X*. That authority is the freshness beacon (D7). Without a beacon, the ceiling degrades to the structural frontier defence (detailed in D7).

### D7 — Auditable sequencing, layered and never necessary for truth

We do not invent a coordinator; we copy the one the ecosystem already trusts. `did:plc` is a centralized-but-auditable sequencer: it orders user-signed operations and publishes a self-certifying append-only log, and its trust model is exactly ours — it cannot forge, it can only reject or omit. So an auditable sequencer is ecosystem-native, not exotic. atproto ships no *trusted* sequencer otherwise (relay/firehose cursors are per-connection and unsigned; repo `rev` is a self-attested per-repo TID clock that cannot order Alice's record against Bob's; verified timestamps are a discussion, not a shipped feature).

The build (certificate-transparency playbook):

- An append-only Merkle log, per workspace, of the record CIDs the sequencer ingested. Our indexer already occupies this position and does the job unaccountably; this makes it accountable.
- Periodic **signed tree heads** — "as of now, this workspace's log commits to this root." Because every record underneath is self-signed (D2), the log can never launder an invalid record into a valid one.
- **Inclusion proofs** ("is my record in the log?") and **consistency proofs** ("is this new head an honest extension of the last one, or a rewrite?"). The consistency proof is what upgrades omission from undetectable to visible.

**Buys:** an auditable total order for tie-breaks; a **freshness beacon** (a fresh signed tree head is a checkable "the head was here at time T" — the thing that makes D6 enforceable); visible omission.

**Optional hardening:** witness cosigning (C2SP `tlog-witness`) — members' own daemons countersign the heads they see, so equivocation (head A to Alice, head B to Bob) becomes self-incriminating the way two contradictory signed records are.

**The invariant that keeps this from being the coordinator we refused:** the sequencer may help; it must never be necessary for truth. It layers — trustless contention rule at the bottom, human endorsement above it, auditable sequencer on top when present. A self-hoster who runs no log falls back to the structural frontier defence and loses only the cold-joiner protection and the tightened ceiling, not correctness. The managed offering can run the log as a service.

**What it still does not buy:** inclusion can be *refused* (now visibly, but it can), and nothing forces a hostile host to broadcast in the first place (the PDS→relay lie of D5). Those remedies stay human.

### D8 — Endorsement-weighted head selection; removal durable when built upon

Head selection must not reward *quantity* of self-asserted entries (the finding that made walk-free's "largest floor wins" an ownership lever). With signed records (D2), "another member built on this" is unforgeable, so head selection leans on genuine endorsement — which branch real members actually extended. The mechanism went through two rounds: an isolated convergence simulation surfaced the first cut (scope endorsement to the fork base, tie-break by a VRF), and an adversarial spec review then showed the sim's tidy *fixed-base* setup had hidden two holes, forcing the sharper form below.

- **Endorsement is frontier-scoped, not fork-base-scoped.** Count only distinct members who are managers in the verifier's *current frontier* roster and authored on the branch. The first cut scoped the electorate to the fork's common ancestor — which *inverts* under an attacker-chosen fork base: root the fork at an ancient record where your since-removed sockpuppets were still managers, and they re-enter the electorate while honest recent managers drop out. Anchoring to the current frontier fixes it: a since-removed member counts for nothing regardless of how deep the fork is rooted, and an honest recent manager counts. This shares the fork-timing ceiling's anchor (D6), so the two hold together — and it inherits the same residual: a device with neither a frontier nor a beacon has no anchor and faces endorsement inversion exactly as it faces stale state. That is the accepted memoryless-victim limitation (no cache *and* no indexer), not a defended case — the same "fork-danger and staleness-danger are one problem."

- **The tie-break is a VRF over the common ancestor, not the CID.** The obvious "lowest CID" tie-break is *grindable*: a CID is a hash of author-chosen bytes, so an author mines a low one — reverse proof-of-work. No hash of the record fixes it. The fix moves the entropy off the record: each fork-eligible record carries a VRF (verifiable random function) output over its own `supersedesCid`, and the fork is resolved at the **most-recent common ancestor** of the competing heads by comparing the branch-root records that directly supersede it — whose `supersedesCid` *is* that ancestor's CID, so all contenders share one VRF input and are comparable. (The first cut compared each head's own `supersedesCid`; adversarial review showed that breaks for cross-depth forks — a malicious writer never retries, so a stale-rooted head competes with an advanced one and their inputs differ.) A VRF output is unique per `(key, input)` — the author cannot search for a favourable one — and every observer computes the same winner offline. Losing a tie costs a retry (D4) and a tie dies the moment a real member endorses, so the tie-break governs only the pre-endorsement instant, unbiasably.

*Alternatives considered for the tie-break.* Sequencer log position ("first-witnessed") is ungrindable too and needs no new crypto, but it makes the tie-break depend on the semi-trusted sequencer and is unavailable to a pure self-hoster — against the "never necessary" ethos. External entropy postdating the fork (a Bitcoin block hash, or a public randomness beacon) is ungrindable but adds latency and an external availability dependency. The VRF was chosen because it is self-contained: it lives in the roster and the record, needs no sequencer, no beacon, and no external entropy, and resolves offline — the same aesthetic as the rest of walk-free. The VRF key is a separate mnemonic-derived key (its own HKDF path), not the Ed25519 signing key reused, to avoid cross-scheme key reuse.

Removal's durability is a liveness property, not a cryptographic instant. Skeptic review (freshness + availability lenses) found the earlier "effective when witnessed" framing conflated *observation* with *durability*: because the signed removal record sits on the remover's own untrusted PDS, a hostile host can delete it, rolling the head back and reinstating the member — witnessing (holding a copy) does not prevent this; only a live descendant superseding the removal does. The fix has two layers — a liveness floor that always holds, and an indexer ceiling that hardens it in practice:

- **Liveness floor.** Treat the revert as the same detected liveness attack as the PDS→relay lie (D5): the remover watches the resolved head, re-issues on a revert, and ultimately migrates off a persistently-hostile host. A removal is durable once built upon by a live descendant, or while the remover's host is honest. Never silent.
- **Forward-secrecy gate.** Forward-secure writes wait for durability — encrypting under the new epoch before the removal is durable would either orphan that content on a revert or leak it under the old key. This bounds a revert to "the member briefly reappears," not "reads new content."
- **Indexer ceiling.** Where an indexer is present, it retains and re-serves the signed removal record after the author's host deletes it, so the head does not roll back past it — closing the offline-remover and persistent-revert gaps. An availability help (the record is signed, so the client trusts the signature, not the indexer), never a truth dependency; a client with no indexer falls back to the liveness floor.

General member-to-member replication — which would make removal cryptographically durable without an indexer — stays deferred ([#19](https://github.com/Opake-at/Opake/issues/19), post-grant). This is the honest form of the first-fold durability window: named and bounded, not papered.

## Risks / Trade-offs

- **[Signing raises the stakes on rotation ([#18](https://github.com/Opake-at/Opake/issues/18)), which does not exist]** → Out of scope here, flagged in the `workspace` limitations and `workspace-identity` delta. Mnemonic/key compromise stays unrecoverable by protocol today; this change does not make rotation harder, and roster-as-registry (D3) gives rotation a concrete future home (update the roster of every workspace you're in).
- **[Roster-as-registry is trust-on-first-use]** → Accepted; it is the same human trust the invite already carries.
- **[The governance envelope leaks the social graph and activity timing]** → Known capability-list contradiction (confidentiality vs. enforcement); paid deliberately, as the shipped design already does. Documented in `record-signatures`, not hidden.
- **[The sequencer can refuse inclusion or a host can withhold broadcast]** → Detected (no echo / failed consistency proof), not prevented. Remedy is account migration — a human action. Stated in `workspace-sequencing` and `indexer-consistency`.
- **[Fork-timing ceiling needs a beacon; self-hosters without a log lose cold-joiner protection]** → The structural fallback (D7) holds; only the memoryless-victim protection degrades — the same victim set as the freshness problem. Named, not solved-by-assertion.
- **[Discard-and-retry could silently drop a member's write]** → The one non-negotiable: rejection is always surfaced to the human. Enforced as a protocol obligation ([#11](https://github.com/Opake-at/Opake/issues/11)), tested as such.
- **[Chain archives ([#68](https://github.com/Opake-at/Opake/issues/68)) partly lose their purpose]** → If onboarding never touches history, archives' cold-start role evaporates; their surviving value is audit availability, a smaller proposal. Reassessed, not silently kept.
- **[Fork-danger and staleness-danger are one problem in two hats]** → Same victim set (cold joiners, long-offline devices, freshly-reset indexer). Every defence (signed records, freshness beacon, witness cosigning, endorsement depth) pays out on both; hardening the memoryless paths is the whole remaining job.

## Migration Plan

1. **[#64](https://github.com/Opake-at/Opake/issues/64) first.** Byte-recomputed CIDs are a hard prerequisite for D2; nothing here lands before they do.
2. **Signatures as an additive field**, verified leniently on read before being required on write, so existing records are not orphaned mid-rollout (composes with `record-validity`'s read-lenient / write-strict posture).
3. **Sequencing log as opt-in**, shipped dark: the indexer publishes tree heads and proofs before any client requires them; clients that ignore the log keep working (the layered invariant makes this safe by construction).
4. **Client pending-state and replay** land with honest UI; the frontier cache ([#69](https://github.com/Opake-at/Opake/issues/69)) is promoted to load-bearing member state.
5. **Rollback:** because every layer above the trustless core is "when present," each can be disabled independently without breaking correctness — the migration's safety property is the same invariant as the architecture's.

## Open Questions

### Resolved (red pen)

- **Document-write scoping — decided.** Document-tree writes stay on their additive-merge path (`spec:tree-chains`) for current members; discard-and-retry does not reach them. A write from a *non-member* (never-member or removed) is discarded as inert, exactly as authority already dictates — a removed member's post-removal document writes do not render. So the discard rule stays membership-scoped, and the only thing "discarded" on the document side is a non-member's write, which was never valid anyway.
- **`did:plc` corroboration — decided: none.** External DID documents (PLC or `did:web`) are not consulted for signing-key provenance at all. The choice is all-or-none: uniform provenance would require verifying every member through the same registry, but `did:web` members (over-represented in this population) have no audit log, so a uniform PLC path is impossible and a mixed one is worse than none. The roster is the sole source (`spec:workspace-identity § External DID documents are not consulted for signing-key provenance`). This moots the PLC verification-method question.
- **Chain-archives ([#68](https://github.com/Opake-at/Opake/issues/68)) — decided: nothing survives.** Onboarding never touches history, so the archives' cold-start role is gone, and the audit-availability angle is not worth its own proposal. #68 does not carry into walk-free v2; close/deprioritise it.
- **No-indexer projection — decided: recommend an indexer.** Correctness never requires the indexer (a self-hoster can confirm writes via firehose self-observation and resolve state from records — the invariant holds). But the reactive *live projection* is an indexer-backed convenience, and a public indexer will be available for exactly this reason, so self-hosters are **strongly recommended** to use one rather than run a bespoke no-indexer projection. `spec:indexer-consistency § Client projections contain only indexer-confirmed state` stands as-is; the no-indexer client is a supported *correctness* path, not a supported *live-projection* path.

### Open

- **Small-n tie-breaks — documented, not solved.** The two-manager knife fight and all-managers-dead succession — no-owner + enforceable roles + no-coordination is jointly unsatisfiable at 2–3 people. Discard-and-retry makes losing cheap but does not create a quorum where there are only two principals. The model's residual attacks (endorsement Sybil, tie-break grinding, retry starvation) all require a competing *manager* and concentrate here; frontier-scoped endorsement and the VRF tie-break (D8) narrow them to "a manager who wins every real-time race, bounded by relenting", but they do not manufacture a quorum. Accepted as a documented limitation of the trust model, not a solved problem (`spec:workspace § Stated limitations no construction removes`).
- **Beacon cadence and ceiling distance — undecided.** What "N behind the head" is in practice, and how often a signed tree head must be published to be a useful freshness beacon, are not yet known; they need the end-to-end latency probe ([#21](https://github.com/Opake-at/Opake/issues/21)) to calibrate against real propagation before any number is written down.
