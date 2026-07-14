## Why

Every workspace-scoped indexer endpoint answers "workspace not yet indexed" and "caller is not a member" with the same 403. Membership is resolved by reading the keyring chain head (`RecordQueries.is_member?/2` → `workspace_keyring_head/1`), and until the genesis keyring travels PDS → firehose → consumer → postgres, no head row exists — so the workspace's own creator is told they are not a member of it. The response is byte-identical to a genuine authorization denial, which forces clients into blanket retry-on-403 (the current `indexer-consistency § Dependent operations tolerate the visibility gap` contract) and makes real authorization failures indistinguishable from pipeline lag. The batch-2 regression net needs a pinned contract to cite; the ambiguous 403 is the wrong contract to pin.

## What Changes

- **BREAKING** — Workspace-scoped indexer endpoints (`/workspace/snapshot`, `/workspace/sync`, `/workspace/chain-head`) return 404 with a machine-readable `workspace_not_indexed` error code when no keyring chain head exists for the workspace, instead of today's 403. 403 is returned only when a head exists and the caller is absent from its `members[]` — it becomes a definitive authorization answer.
- A torn-down workspace (`keyring-tombstones § torn_down` removes the chain-head row) answers `workspace_not_indexed` as well: with no live keyring record, the workspace is materially dead, and "the indexer has nothing to answer for" is the honest response for both the not-yet and the no-longer case.
- The client visibility-gap retry narrows: dependent operations retry on the transient `workspace_not_indexed` signal within the bounded window; a 403 now surfaces immediately as the authorization error it is. The WASM/SDK chain-head resolution path and the e2e save-retry pattern move onto the new signal.
- Existence disclosure is bounded and stated: distinguishing "indexed" from "unknown" reveals only whether a public firehose record has been consumed — keyring records are public ciphertext on the authoring PDS, so the split discloses nothing membership-private.

## Capabilities

### New Capabilities

None — this is a refinement of the existing consistency contract.

### Modified Capabilities

- `indexer-consistency`: adds a requirement that the indexer distinguishes an unknown workspace (no keyring chain head) from a non-member request (head exists, caller not in `members[]`), including that the unknown answer is deliberately ambiguous between not-yet-indexed and torn-down; amends `Dependent operations tolerate the visibility gap` so the client retry window is gated on the transient `workspace_not_indexed` signal rather than blanket 403 tolerance, and an authorization 403 is definitive and surfaces without retries.
- `workspace-identity`: amends `Workspace-scoped indexer calls pass genesis` — its no-row outcome is now `workspace_not_indexed` rather than a non-member 403, which moves the head-URI-as-workspace-id programming error into the client's retryable class; the requirement becomes the sole guard against that defect and its scenario is restated against the new wire contract.

## Impact

- **Indexer**: `workspace_controller.ex` membership helper splits the nil-head case out of `check_membership/2`; `record_queries.ex` membership queries gain a way to report "no head" distinctly from "not a member". SSE `events_controller.ex` is unaffected — workspace-topic subscription is event-driven off the keyring upsert and self-heals when genesis lands.
- **Client (WASM/SDK)**: chain-head resolution and workspace-scoped fetches classify `workspace_not_indexed` as retryable-within-window; 403 becomes terminal. Error surface distinguishes "visibility wait exhausted" from "not authorized".
- **Tests**: indexer controller tests pin the 404/403 split (including the torn-down case); federation-tier e2e drops the save-retry-on-403 pattern in favor of the pinned contract. Batch 2 (workspace-identity regression net) cites the amended requirements.
- **Docs**: `docs/indexer.md` endpoint table gains the response contract.
