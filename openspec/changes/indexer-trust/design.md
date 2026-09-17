# Design

## Context

See proposal.md for motivation. The facts below are what the approach rests on, each checked in the code.

**What the client fetches from a PDS today.** Three things, all through unauthenticated `getRecord` after a DID-document lookup per authority (`fetch_with_cache`, crates/opake-core/src/directories/chain.rs):

1. Every link of the keyring chain, head to genesis, on every workspace resolution (`verify_and_walk_chain` called from `fetch_keyring_chain_head_once`, crates/opake-core/src/opake.rs). The walk is wrapped in `VisibilityRetry` (`fetch_keyring_chain_head`, same file), so a missing link is retried as a visibility gap and surfaces as `Error::VisibilityTimeout`.
2. The parent or target directory record before a write (`fetch_chain_node` calls in crates/opake-core/src/manager/{upload,directory,rename,move_entry,substitute,delete}.rs).
3. The recipient's `publicKey/self` record and, separately, their DID document (`verify_public_key_record`, crates/opake-core/src/resolve.rs).

Blobs are fetched from the authoring PDS (`get_blob`, crates/opake-core/src/client/xrpc/blobs.rs; `get_blob_public`, crates/opake-core/src/client/did.rs).

**What the indexer already serves.** The workspace snapshot returns every directory and document record byte-identical to the PDS copy, with `uri`, `indexedAt` and `deletedAt` beside it (`envelope/1`, apps/indexer/lib/opake_indexer_web/controllers/tree_helpers.ex). The keyring listing returns keyring head records the same way (`KeyringsController.index`). Only `chain_head` returns URIs and CIDs without bytes (`WorkspaceController.chain_head`, apps/indexer/lib/opake_indexer_web/controllers/workspace_controller.ex). Every one of these routes sits behind `OpakeIndexer.Auth.Plug` and `Plugs.RateLimit` (30 requests per second per IP, `@burst_size`, apps/indexer/lib/opake_indexer_web/plugs/rate_limit.ex).

**What the walk verifies.** `verify_and_walk_chain` follows `supersedes` to a record with none and checks that record's URI equals the expected genesis. `verify_keyring_chain_authority` checks that each supersede's authority DID was a manager in the prior record's member list. Neither checks a signature. Neither computes a CID; content pins compare against the CID the PDS reports (`fetch_with_cache`). The indexer refuses the same authority violation at ingest (`check_keyring_supersede/4`, apps/indexer/lib/opake_indexer/authority.ex), so the client walk re-derives a decision the indexer has already made over the same unsigned bytes.

**What actually binds content.** A group-key wrap is AEAD-bound to the workspace genesis (`spec:workspace-identity § Group-key wraps are AEAD-bound to genesis`). A record that seals ciphertext binds its own URI (`spec:lineage § Records that seal ciphertexts to their own URI choose their own rkey`). Every keyring adopted into workspace state passes the identity derivation check (`spec:workspace-identity § Identity adoption verifies by derivation`), which is offline and needs no chain. A substituted head from another workspace fails to unwrap; a forged genesis fails derivation. The walk added nothing to either.

**How heads are chosen.** The fork winner is `createdAt` with a `(did, rkey)` tiebreak (`spec:tree-chains § Concurrent supersedes fork, and the indexer picks a deterministic winner`). Authority checks read the predecessor's member list; additivity checks read the two listings (`additive?/3`, authority.ex). All three are functions of the record set, not of when a record arrived.

**What tombstones are for.** A delete sets `deleted_at` on the row (`RecordQueries`, apps/indexer/lib/opake_indexer/queries/record_queries.ex) and `tombstone_cleanup.ex` purges the row after seven days. The only reader of `deleted_at` after the delete itself is incremental sync: `workspace_changes/3` and `cabinet_changes/3` select `deleted_at > since`, and the client's `with_delta` (crates/opake-core/src/directories/tree.rs) and `apply_and_cache_delta` (crates/opake-core/src/manager/tree.rs) drop envelopes with `deleted_at` set. Snapshot queries exclude deleted rows entirely (`workspace_records/2`, `cabinet_records/2`). Both clients reach the incremental path: `try_sync_deltas` (manager/tree.rs) calls sync when the cache holds a `__sync__` cursor and snapshot otherwise, and the web keepers' reconnect goes through `load_tree` (`resync_workspace_tree`, crates/opake-wasm/src/sse_wasm.rs), so a reconnecting web client and a CLI tree load both depend on tombstones to learn about deletions. Delete-outcome resolution and the rollback re-broadcast run at delete time from live rows and read no tombstone afterwards.

**A second walk on a read path.** `collect_ever_manager_dids` (crates/opake-core/src/manager/tree.rs) calls `walk_back_to_genesis` across PDSes during tree load to collect every DID that was ever a manager, so the additivity slow path can exempt a former manager's deletions. It needs chain history, not just the head, and the indexer holds every keyring record it ingested.

**What the current backfill is.** `Backfill.backfill_known_dids/0` seeds its DID list from `records.author_did` (`all_known_dids/0`, apps/indexer/lib/opake_indexer/backfill.ex), so an empty database has nothing to seed from. `backfill_did/1` walks one DID's collections in order keyring, directory, document, grant and replays each record through the firehose dispatch. A supersede whose predecessor lives on a PDS not yet visited is rejected with `:prior_not_indexed` (authority.ex), and nothing replays it later; the code comments say it "heals on reprocess" and no reprocess exists. A former member's PDS is never visited, because the seed comes from records already held. Tombstones are purged after seven days (`@tombstone_ttl_days`, apps/indexer/lib/opake_indexer/tombstone_cleanup.ex).

**Constraints.** Records stay client-encrypted, so the indexer holding bytes changes nothing about what it can read. Blobs never touch the indexer: no relay, no cache. The indexer must stay replaceable by anyone who can run it against the same PDSes, or "load-bearing" turns into "precious".

## Goals / Non-Goals

**Goals:**
- Name the trust boundary once, in a spec a builder can read.
- Zero PDS record reads on the client's read and write paths. Blob and key-record reads remain.
- A keyring resolution that cannot be wedged by a deleted or unreachable historical record.
- An indexer whose canonical state is a pure function of the reachable live records, demonstrated by rebuilding one from empty.
- The old walk preserved as a tool an operator or member runs by choice.

**Non-Goals:**
- Defending against a hostile indexer. That is the tier above this one and needs signed responses.
- Freshness guarantees against a stale indexer. Head ratchets are also the tier above.
- Fairness in fork resolution. See the risk on `createdAt` below.
- Serving records to non-members. Every record read stays behind indexer authentication and the membership check.

## Decisions

### D1. The trust statement is a capability, not a section of `indexer-consistency`

`indexer-consistency` is about the indexer's own consistency: cursor, snapshot-plus-stream, visibility gap. What a *client* is allowed to believe from the indexer is a different subject, and it is the one a builder opens the specs to find. A separate `indexer-trust` capability also keeps this change off the requirements `membership-mutation-outcomes` modifies, so the two deltas do not collide at sync.

Alternative: ADDED requirements inside `indexer-consistency`. Rejected for the collision and because the capability's purpose statement would then be wrong.

### D2. Record bytes come from the indexer; the head endpoint returns the record

The snapshot already does this for directories and documents. The change extends the same envelope to the chain-head response, so a keyring resolution is one authenticated request that returns the head record and its CID. The client verifies what it can offline: the derivation check, the record's declared version, and the CID recomputed from the returned bytes against the CID the indexer reports. It does not fetch the record again from the PDS.

Pre-write directory fetches take the target record from the indexer-confirmed projection where the client holds one (`spec:indexer-consistency § Client projections contain only indexer-confirmed state`), and from a single-record indexer read otherwise. Which of the two each write path uses is an implementation matter; the requirement is that neither goes to a PDS.

Alternative: keep fetching bytes from the PDS and only stop walking. Rejected because it keeps the DID-document-then-PDS round trip per record and keeps every write dependent on every author's PDS being up, for no verification the client can actually perform.

### D3. The walk becomes `opake workspace audit`, a report

The audit walks a workspace's keyring chain head to genesis, fetching each link from its authoring PDS, and reports what it finds: the authority trail, each link's CID recomputed against the successor's pin, links missing from a PDS, and links the indexer holds that a PDS no longer serves. It exits non-zero when any link is missing or fails a check. No client operation reads the audit or its result; a member runs it when they want a second opinion on the indexer.

Alternative: a verdict clients consult before trusting a head. Rejected because it puts the walk back on the hot path under a different name.

### D4. No indexer pin, no indexer identity

Head selection, authority and additivity are all pure functions of the live record set (see Context). Two indexers holding the same records serve the same heads, so which indexer a client talks to is client configuration and needs no protocol field. Giving the indexer a DID only matters when its responses are signed, which is the tier above.

Alternative: pin the indexer's DID in the keyring genesis. Rejected because migration then needs two indexers to agree on ordering during the handover, which is the problem the axiom exists to avoid, and no one self-hosts an indexer yet.

### D5. No tombstones; sync carries a manifest

The indexer resolves a delete's outcome from the rows it holds, broadcasts it, and drops the row. Incremental sync returns the changed records plus a manifest: the URI and CID of every live record in scope. The client drops whatever it holds that the manifest does not name. Snapshot semantics are unchanged.

This removes the retention window as a concept. A client offline for a year syncs the same way as one offline for a minute. And it closes the last difference between a live indexer and a rebuilt one: with tombstones, the live indexer holds rows the rebuild never saw; without them, they hold the same rows. "Same heads" becomes "same state".

Cost: the manifest grows with the live record count in scope, at roughly a hundred bytes per record. At the workspace sizes #5 and #27 describe as already degrading, that is tens of kilobytes per sync, well under the changed records themselves. The rollback rule in `keyring-tombstones` already selects the newest live row and never the deleted predecessor, so it needs only its stated reason changed.

Alternative: keep tombstones with a longer or operator-set retention. Rejected because any finite window leaves a client that exceeds it silently holding deleted records, and because a rebuilt indexer can never have them, so a spec that relied on them would be true of some deployments only.

Alternative: full snapshot on every reconnect. Rejected because it is the whole tree per reconnect, which is what incremental sync exists to avoid.

### D6. Rebuild converges, and the current backfill is a development resync

The load-bearing requirement is that an indexer started from an empty database, given one root and the reachable PDSes, ends with the same heads as an indexer that watched the firehose throughout. Meeting it needs three things the current backfill lacks: an external seed (one DID or a set of genesis URIs), a discovery step that follows member lists and `supersedes` authorities to every PDS that holds a link, and ingestion that either follows `supersedes` back before accepting a record or replays rejected records until a pass changes nothing.

The existing `mix opake.resync` keeps its name and is documented as a per-DID development tool. The rebuild is a separate entry point with a test that runs two indexers against one set of PDSes and compares heads.

Alternative: fix the existing backfill in place. Rejected because its seed is the table it rebuilds, which is the wrong shape; the per-DID walk is useful as it is for development.

### D7. Rate limiting moves with the traffic

Every record read now lands on the indexer, behind a 30 request-per-second per-IP bucket sized when the indexer served only snapshots and events. The limit is raised or scoped per authenticated DID; which is decided while implementing against measured request rates from the web client. The requirement is only that a member's ordinary session does not hit it.

## Risks / Trade-offs

- **`createdAt` is written by the author, and it is the first key of the fork tiebreak.** A forker who backdates wins the fork. This is unchanged by this change, and the client walk never caught it either. What changes is that no client-side second opinion exists on the hot path, so the indexer's choice is the whole story. → The spec says the indexer's obligation is consistency, not fairness, and the audit reports fork points so a member can see them. Fairness is future work on the tiebreak itself.
- **A stale indexer serves a stale head and nothing on the client notices.** Also unchanged: a walk from a stale head verifies. → Named as the tier above (head ratchet). Not defended here.
- **Removal durability against a hostile PDS.** With no tombstones, a removal record its host deletes is gone from every indexer alike, and a rollback readmits the member. Today the tombstone bought seven days. → Stated in the spec. The durable answer is a removal that is also carried by a record on a remaining member's PDS, which is the design work `walk-free` finding 10 asked for and is not this change.
- **Every record read becomes a rate-limited, authenticated indexer request.** → D7. Also a gain: today any ciphertext is a public `getRecord` away; after this, record reads are membership-checked.
- **The audit can report gaps a rebuilt indexer cannot fill.** A former manager's supersedes live on their PDS; a rebuild seeded from current members may never visit it. → The rebuild's discovery step follows `supersedes` authorities, not just member lists. The audit reports a link as missing rather than silently skipping it. The ever-manager set read from indexer-held keyrings shrinks by the same links, so a former manager whose keyring link was deleted loses the additivity exemption; that is the live record set speaking, not a defect.
- **The pre-write fetch from the projection can be stale by the visibility gap.** → This is already true: the head URI came from the indexer before, only the bytes came from the PDS. `spec:indexer-consistency § Dependent operations tolerate the visibility gap` and the fork event cover the consequence.

## Migration Plan

No record or lexicon changes. Two additive wire changes: the chain-head response gains `record` per head (`head_uri` and `head_cid` stay), and sync responses gain `manifest`. Deploy order: indexer first, then clients. Once every shipped client applies the manifest, `deleted_at` stops being set and `deletedAt` leaves the envelope; until then the indexer sets both, so an old client keeps working. Dropping `tombstone_cleanup.ex` waits for that second step. Old clients keep walking against the new indexer without change. New clients against an old indexer fail resolution with a clear error naming the missing field, so the indexer must be deployed first. Rollback is redeploying the previous client.


## Open Questions

- Whether the pre-write directory fetch should read the projection or make a single-record indexer request is decided per write path while implementing; both satisfy the requirement.
- The rate limit's new value or scope is set from measured web-client request rates during implementation.
