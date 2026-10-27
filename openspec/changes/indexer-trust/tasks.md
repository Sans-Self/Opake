# Tasks

## 1. Indexer serves head records

- [ ] 1.1 Extend the chain-head response with the head record envelope for the keyring and root directory heads, keeping `head_uri` and `head_cid`; verified by a controller test asserting `record`, `uri` and `cid` per head and by the existing chain-head tests still passing.
- [ ] 1.2 Add a single-record read for a member of the record's workspace, refused with 403 for a non-member and 404 for an unindexed record; verified by controller tests for each of the three outcomes with per-file `x-forwarded-for` addresses.
- [ ] 1.3 Add a `manifest` of live URI and CID pairs to the workspace and cabinet sync responses; verified by controller tests asserting the manifest names every live record in scope and none deleted.
- [ ] 1.4 Add a keyring-history read returning every keyring record the indexer holds for a workspace, membership-checked; verified by controller tests for member, non-member and unknown workspace.
- [ ] 1.5 Drop a record's row at delete time after outcome resolution and broadcast, and remove `tombstone_cleanup.ex`; verified by `firehose_record_delete_test.exs` and `firehose_keyring_delete_test.exs` asserting no row remains and the outcomes are unchanged. Sequenced after 2.6 ships so no client still reads `deletedAt`.
- [ ] 1.6 Measure web-client request rates against the indexer after 2.x lands and raise or re-scope the per-IP rate limit so an ordinary session does not receive 429; verified by the e2e suite passing with the new limit and the measured rate recorded in the PR.

## 2. Client reads records from the indexer

- [ ] 2.1 Change the chain-head client type to carry the head record and CID, recompute the CID from the returned bytes, and fail resolution with a named error on mismatch; verified by unit tests for the match and mismatch cases.
- [ ] 2.2 Replace the walk in keyring head resolution with adoption of the returned head after the derivation check, removing the walk and authority re-verification from that path; verified by a test resolving a workspace against a fake indexer with a transport that fails any PDS `getRecord`.
- [ ] 2.3 Route every pre-write directory fetch in the manager modules to the indexer-confirmed projection or the single-record read; verified by a test per write path (upload, mkdir, rename, move, substitute, delete) with a transport that fails any PDS `getRecord`.
- [ ] 2.4 Add regression `bug__deleted_intermediate_wedges_resolution` where a chain with a deleted intermediate resolves successfully; verified by the test passing.
- [ ] 2.5 Confirm blob download and `publicKey/self` reads still go to the authoring PDS; verified by a test asserting the transport receives the PDS request for each.
- [ ] 2.6 Apply the sync manifest in `with_delta` and `apply_and_cache_delta`, dropping cached records the manifest does not name, and stop reading `deletedAt`; verified by a test where a cached record absent from the manifest is removed and by the existing delta tests passing.
- [ ] 2.7 Read the ever-manager set in `collect_ever_manager_dids` from the indexer's keyring history instead of walking PDSes; verified by an additivity slow-path test with a transport that fails any PDS `getRecord`.

## 3. Audit command

- [ ] 3.1 Add `opake workspace audit <workspace>` that walks head to genesis via the authoring PDSes, recomputes each link's CID against the successor's pin, checks each author's role in the prior record, and reports links the indexer holds that no PDS serves and fork points the indexer recorded; verified by a CLI test on a clean chain exiting zero with every link listed.
- [ ] 3.2 Exit non-zero when a link is missing or fails a check, naming the link; verified by a CLI test on a chain with a deleted intermediate.
- [ ] 3.3 Keep the walk helpers out of every read and write path; verified by a grep test or lint asserting the walk is called only from the audit module and its tests.

## 4. Indexer rebuild

- [ ] 4.1 Add a rebuild entry point taking a DID or a set of genesis URIs as seed; verified by a test that starts from an empty database and finishes with the seed's workspaces indexed.
- [ ] 4.2 Discover accounts transitively through member lists and `supersedes` authorities, including accounts absent from every current member list; verified by a test where a removed manager's supersede is ingested from a PDS the seed's member lists do not name.
- [ ] 4.3 Make ingestion order-independent by replaying records rejected for an unindexed predecessor until a pass changes nothing; verified by a test that ingests a supersede before its predecessor and asserts the same verdict as the forward order.
- [ ] 4.4 Document `mix opake.resync` as the per-DID development resync and the rebuild as the operator path in docs/indexer.md; verified by the doc naming both and their difference.
- [ ] 4.5 Convergence test: run one indexer through the federation stack from the start and rebuild a second from empty at the end, then compare chain heads for every workspace; verified by the federation test passing with identical heads, URIs and CIDs.

## 5. Docs and citations

- [ ] 5.1 Replace the walk-as-trust-mechanism description in docs/ARCHITECTURE.md and docs/FLOWS.md with the trust statement and a pointer to the audit; verified by grep showing no remaining claim that reads verify the chain.
- [ ] 5.2 Cite `spec:indexer-trust` requirements from the new tests and `just spec-lint` reports zero dangling citations.
