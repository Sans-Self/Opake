# Proposal

## Why

Clients treat the indexer as the source of chain heads and then re-verify its answer by walking every chain back to genesis across PDSes, one public getRecord per link, on every workspace resolution. The walk verifies no signatures and catches nothing the indexer's own write-time authority check does not already refuse, while it makes every membership operation depend on every historical PDS being up and every historical record still existing. This change states what a client trusts the indexer for, moves the walk to an on-demand audit, and makes the indexer rebuildable from the PDSes so that trust costs nothing in durability.

## What Changes

- A new `indexer-trust` capability states the trust boundary: the indexer is trusted for discovery, ordering, liveness and record bytes; the authoring PDS is trusted for blobs and for the published key record; the DID document is trusted for the identity key.
- Clients read `at.opake.*` records from the indexer, never from a PDS. The keyring chain-head lookup returns the head record and its CID, not a URI the client must go and fetch. The snapshot and keyring listing already return record bytes (`envelope/1`, apps/indexer/lib/opake_indexer_web/controllers/tree_helpers.ex); the chain-head endpoint and the pre-write fetches are the gap.
- Blob downloads and published key record reads stay on the authoring PDS. DID document resolution stays on the DID method's directory. The indexer never serves, relays or stores blobs.
- The head-to-genesis keyring walk with authority re-verification (`verify_and_walk_chain`, `verify_keyring_chain_authority`, crates/opake-core/src/directories/chain.rs, called from `fetch_keyring_chain_head_once`, crates/opake-core/src/opake.rs) leaves every read and write path and becomes `opake workspace audit`, a report with a non-zero exit status on a gap. No client operation consults it.
- The identity derivation check on adoption (`spec:workspace-identity § Identity adoption verifies by derivation`) is unchanged and remains the client's offline defence against a head from the wrong workspace.
- The indexer SHALL be able to rebuild its canonical state from an empty database given the reachable PDSes and one root, and two indexers holding the same live records SHALL serve the same heads. The current backfill (`backfill.ex`) seeds its DID list from the table it rebuilds, ingests per DID so cross-PDS `supersedes` authority checks reject with no second pass, and cannot reach a former member's PDS. It is relabelled a development resync and a rebuild that converges is specified.
- Tombstone rows go. Today a deleted record's row is kept with `deleted_at` set and purged after seven days (`tombstone_cleanup.ex`), and its only reader is incremental sync (`workspace_changes` and `cabinet_changes`, apps/indexer/lib/opake_indexer/queries/record_queries.ex, select `deleted_at > since`), which both the web keepers' reconnect and the CLI tree load use through `try_sync_deltas` (crates/opake-core/src/manager/tree.rs). The indexer drops the row at delete time after resolving and broadcasting the outcome. Sync responses carry a manifest of live URIs and CIDs, and clients drop what the manifest does not name. A rebuilt indexer then holds the same rows as one that watched every deletion.
- The second walk on a read path, `collect_ever_manager_dids` (crates/opake-core/src/manager/tree.rs), which walks the keyring chain across PDSes to exempt former managers in the additivity slow path, reads the keyring records the indexer holds instead.
- `definitions` gains `audit` and `manifest`; `tombstone` becomes the delete event rather than a stored row.

No record format or lexicon changes. One wire change: sync responses gain a `manifest` field, and `deletedAt` leaves the envelope once clients read the manifest.

## Capabilities

### New Capabilities
- `indexer-trust`: what a client trusts the indexer for and what it must fetch elsewhere; records are read from the indexer; the chain walk is an audit; the indexer rebuilds from the PDSes and converges.

### Modified Capabilities
- `tree-chains`: `Consumers build the live tree from chain heads only` — head verifiability is defined over the links the consumer holds from the indexer, not over a chain walk to a PDS.
- `keyring-tombstones`: `Rollback restores the newest live record and re-broadcasts it` — the restored head is the newest row held; no purge window is involved.
- `definitions`: `audit` and `manifest` added; `tombstone` redefined as the delete event.

## Out of scope

- Pinning a workspace to an indexer, or giving the indexer a DID. Heads are a pure function of the live record set, so which indexer a client uses stays client configuration.
- Signed indexer responses, head ratchets and ingest spot-checks (the hardening tiers above this one).
- Blob relaying or caching by the indexer, now or later.
- Fork replay and the losing client's response to `chain:forked` (open question in `tree-chains`).
- Removal durability against a PDS operator who deletes a removal record. With no tombstones, a deleted removal is gone from every indexer alike; the spec states that and does not defend it.
- Rewording `membership-mutation-outcomes`, which modifies the fork requirement this change relies on. That change is re-based after this one syncs.

## Impact

- `crates/opake-core`: `directories/chain.rs` (walk becomes audit-only), `opake.rs` (`fetch_keyring_chain_head_once` and the `VisibilityRetry` loop around it), `indexer/client.rs` (`IndexerChainHeadProvider`, chain-head response type), `indexer/types.rs` (`deleted_at` leaves the envelope, manifest arrives), `manager/tree.rs` (`with_delta` and `apply_and_cache_delta` apply the manifest; `collect_ever_manager_dids` reads indexer-held keyrings), `manager/{upload,directory,rename,move_entry,substitute,delete}.rs` (pre-write `fetch_chain_node` calls).
- `apps/indexer`: `workspace_controller.ex` and `cabinet_controller.ex` (`chain_head` returns the head envelope; sync responses carry the manifest), `record_queries.ex` (delete drops the row; `changes_since` loses its `deleted_at` branch), `backfill.ex` (rebuild), `tombstone_cleanup.ex` (removed), a keyring-history read for a workspace's members, rate limiting (`plugs/rate_limit.ex`, 30 requests per second per IP) now sits in front of every record read.
- `apps/cli`: `commands/workspace.rs` gains `audit`.
- `packages/opake-sdk`, `apps/web`: no API change; fewer PDS requests in the network tab.
- Tests: `tests/tests/federation/` (walk-free resolution across PDSes, rebuild convergence), `apps/indexer/test/` (rebuild from empty, chain-head envelope), `crates/opake-core` (audit report, no PDS fetch on resolution).
- Docs: `docs/ARCHITECTURE.md`, `docs/indexer.md` and `docs/FLOWS.md` describe the walk as the trust mechanism and need the trust statement.
