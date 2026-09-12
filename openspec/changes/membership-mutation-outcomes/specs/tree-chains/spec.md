## MODIFIED Requirements

### Requirement: Concurrent supersedes fork, and the indexer picks a deterministic winner

When two curators supersede the same prior canonical concurrently, the chain forks: both records exist and both name the same `supersedes` target. The indexer SHALL detect the fork (a record whose `supersedes` target already has a successor), pick the winner deterministically by `createdAt` with a `(did, rkey)` tiebreak, keep the loser's record on its PDS but out of the canonical chain, and emit a `chain:forked` SSE event scoped to the affected workspace and chain, carrying the loser's URI, the fork point, and the winner's URI + CID (`SseChainForked`, crates/opake-core/src/indexer/sse/events.rs).

Fork detection operates on the `supersedes` back-edge and needs no plaintext path, which is why it holds even though directory paths are encrypted-name-derived. Fan-out is stateless (crates/opake-core/src/indexer/chain_fork_keeper.rs). For membership mutations on the keyring chain, the losing client SHALL surface that the intent was not applied and offer explicit semantic retry under `spec:workspace-membership § Membership mutations report canonical outcomes and retry intent`. It SHALL NOT automatically replay the losing member list. Directory-operation replay remains unspecified; this membership outcome rule changes neither winner selection nor the directory conflict policy.

#### Scenario: two editors add entries against the same canonical

- **GIVEN** editors B and C each fetch the same canonical directory and write concurrent supersedes pointing at it
- **WHEN** the indexer processes both
- **THEN** one wins by `createdAt`/`(did, rkey)`, and the loser's client receives a `chain:forked` event naming the fork point and the winning head
- Contract in FEDERATION.md "Concurrent writes"; event shape in `SseChainForked`

#### Scenario: a losing membership change needs explicit retry

- **WHEN** a client's keyring mutation receives decisive evidence that a competing supersede won
- **THEN** it reports the intent as not applied, preserves canonical membership in its projection, and offers explicit retry against fresh state rather than automatically publishing the losing member list
