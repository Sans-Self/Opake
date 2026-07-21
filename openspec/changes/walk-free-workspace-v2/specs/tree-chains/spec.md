## MODIFIED Requirements

### Requirement: Concurrent supersedes fork, and the indexer picks a deterministic winner

When two curators supersede the same prior canonical concurrently, the chain forks: both records exist and both name the same `supersedes` target. The indexer SHALL detect the fork (a record whose `supersedes` target already has a successor), pick the winner by the ungrindable tie-break below, keep the loser's record on its PDS but out of the canonical chain, and emit a `chain:forked` SSE event scoped to the affected workspace and chain, carrying the loser's URI, the fork point, and the winner's URI + CID (`SseChainForked`, crates/opake-core/src/indexer/sse/events.rs).

The tie-break SHALL be neither `createdAt` nor the record CID. `createdAt` is author-controlled and freely set, so it rewards backdating. The CID is no better: it is a hash of author-chosen bytes, so a curator can pad or reorder content and mine a low CID — a reverse proof-of-work, not an ungrindable value (an earlier draft of this delta wrongly claimed the CID could not be ground; it can). The tie-break SHALL instead be the lowest VRF output over the shared parent CID, computed with the author's roster-carried VRF key, exactly as membership head selection resolves an equal-endorsement tie (`spec:workspace-membership § Head selection is endorsement-weighted, pre-fork-scoped, and tie-broken ungrindably`); a record with no valid VRF proof ranks after those that carry one, with lowest CID as the same degraded fallback. Because directory forks are additive and the losing branch's data survives for replay, the tie-break decides only which head renders canonical, not what is lost; making it ungrindable removes a steering lever without changing that additive-merge property.

Fork detection operates on the `supersedes` back-edge and needs no plaintext path, which is why it holds even though directory paths are encrypted-name-derived. Fan-out is stateless (crates/opake-core/src/indexer/chain_fork_keeper.rs). Detection and surfacing end at the client's doorstep: what a losing client does with the event — refetch, replay, or surface to the user — is unspecified today (see open questions). Clients SHALL recompute the winner from the signed records rather than trusting the indexer's selection (`spec:indexer-consistency § The indexer is an auditor, never necessary for writing or truth`).

#### Scenario: two editors add entries against the same canonical

- **GIVEN** editors B and C each fetch the same canonical directory and write concurrent supersedes pointing at it
- **WHEN** the indexer processes both
- **THEN** one wins by the lowest VRF output over the shared parent CID, and the loser's client receives a `chain:forked` event naming the fork point and the winning head

#### Scenario: neither backdating nor CID-grinding steers the fork

- **GIVEN** two concurrent directory supersedes of the same canonical, one carrying a backdated `createdAt` and padded to mine a low CID
- **WHEN** the fork is resolved
- **THEN** the winner is decided by the VRF tie-break, independent of `createdAt` and of record content, so neither the backdate nor the CID-grind confers any advantage
