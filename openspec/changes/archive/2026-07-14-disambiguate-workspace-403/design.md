## Context

Membership checks on workspace-scoped indexer endpoints resolve through `RecordQueries.is_member?/2` → `member_role/2` → `workspace_keyring_head/1` (apps/indexer/lib/opake_indexer/queries/record_queries.ex). The head query joins `chain_heads` to `records`; it returns `nil` both when the genesis keyring has not been consumed and when a `torn_down` tombstone outcome removed the chain-head row. `WorkspaceController.check_membership/2` maps every falsy result to `{:error, 403, "not a member of this workspace"}` — one wire response for three distinct states (not yet indexed, torn down, genuinely not a member).

The client side compensates with blanket tolerance: `indexer-consistency § Dependent operations tolerate the visibility gap` currently blesses retrying authorization failures during a bounded window, and the federation-tier e2e suite carries a save-retry pattern. Both exist only because the wire signal is ambiguous.

SSE workspace-topic subscription (`events_controller.ex`) is not affected: subscription is event-driven off the keyring upsert envelope and self-heals when genesis lands, so it never consults `is_member?/2` in the racy window.

## Goals / Non-Goals

**Goals:**

- One wire contract: 404 + `workspace_not_indexed` when no keyring chain head exists; 403 only after an indexed head was consulted and the caller is absent from `members[]`.
- 403 becomes definitive — clients surface it immediately, retry only the transient signal.
- The batch-2 regression net can cite a pinned contract instead of a race workaround.

**Non-Goals:**

- The write-visibility successor (cursor exposure, per-record visibility probe) stays gated on the consume-lag data, per the canon open question. This change narrows the retry trigger; it does not let a client await its own write.
- No SSE payload or subscription-gating changes.
- No change to `authority.ex` write-time enforcement — this is a read-path response contract only.
- No per-record 404 semantics for documents/directories; only the workspace-scoping membership gate is disambiguated.

## Decisions

**404 + machine-readable body over alternative statuses.** The indexer genuinely has no such resource: honest REST semantics, no exotic codes. 425 Too Early is defined by RFC 8470 exclusively for TLS-early-data replay refusal — using it for pipeline lag is code-squatting that would mislead any middleware or reader. 503 + Retry-After is the only conventional home for Retry-After but reads as "server broken" to monitoring. Clients branch on the body's `error` code (`workspace_not_indexed`), not the bare integer — the status exists for proxies and logs, the code is the contract.

**Torn-down workspaces share the `workspace_not_indexed` answer.** After `torn_down` the chain-head row is gone and no live keyring record remains; the workspace is materially dead (spec:keyring-tombstones). Distinguishing "never indexed" from "no longer indexed" would require consulting tombstone rows that are purged on a 7-day TTL — the distinction would be temporary, unreliable, and useful to no client decision. Both cases mean "the indexer has nothing to answer for."

**Query layer reports three states, controller maps to wire.** `is_member?/2`'s boolean collapses the states too early. The membership resolution exposed to controllers becomes three-valued — no head / head-without-caller / member role — with the controller mapping to 404/403/serve. The workspace controller is `is_member?/2`'s sole production caller, so the boolean helper is deleted with the migration — keeping a two-valued twin next to the three-valued resolver is how a second source of truth grows back.

**Client retry narrows to the transient code.** The WASM/SDK chain-head resolution and workspace-scoped fetch paths classify `workspace_not_indexed` as retryable-within-window and 403 as terminal. Error types distinguish "visibility wait exhausted" (names the wait, actionable as latency) from "not authorized" (actionable as membership). The e2e save-retry pattern is deleted in favor of the client's own conforming retry.

**Existence disclosure accepted and stated.** A prober now learns "indexed vs not" for a workspace id instead of a uniform 403. Keyring records are public ciphertext on the firehose and workspace ids are unguessable genesis AT-URIs, so the split reveals nothing beyond what the firehose already publishes. Stated in the spec so the trade-off is deliberate, not accidental.

## Risks / Trade-offs

- [The retryable class swallows permanent defects: `workspace_not_indexed` conflates pre-genesis, torn-down, and wrong-URI-kind (a head URI or garbage id passed as `workspace_id`), and the third is a programming error wearing the transient signal — it burns the full retry window and surfaces as a visibility wait instead of failing fast] → The wire contract cannot catch this; `workspace-identity § Workspace-scoped indexer calls pass genesis` (delta in this change) is the guard, enforced at call sites by resolve-first. The window-exhaustion error names the workspace id so a mispassed head URI is visible in the failure, not laundered into "slow pipeline".
- [A dead workspace reads as lag: a client holding a stale projection of a torn-down workspace (missed `torn_down` event, tombstones purged) receives the retryable code on every read, forever] → Pinned in the ADDED requirement: the signal is deliberately not-yet/no-longer ambiguous, client copy claims neither deletion nor lag, and reconciliation (bootstrap or keyring event) resolves which case it was. The projection self-heals on the next sync-then-stream cycle; the error surface never invites the user to wait for a workspace that is gone.
- [A mistyped workspace name on the CLI burns the retry window before erroring: name→workspace resolution is a dependent operation (it lists the actor's workspaces via the indexer), so an unknown name is indistinguishable from a not-yet-indexed one until the window exhausts] → Accepted, and unavoidable at this layer: workspace names are encrypted metadata the indexer never sees, so no wire signal can separate "not yet listed" from "never existed" by name. The exhaustion error names the awaited workspace so the typo is visible in the failure.
- [Clients in the wild still treating 403 as retryable] → Single deployment surface: the web app, CLI, and daemon ship from this repo; the client change lands in the same change as the indexer change. No third-party clients exist yet; the spec is the notice.
- [A 404 during a genuine outage window looks like a dead workspace to a naive client] → The retry window applies to `workspace_not_indexed` exactly as it did to the old 403; window exhaustion surfaces a visibility-wait error, not "workspace gone". Client copy never claims deletion off this signal.
- [Three-valued membership resolution drifts from `head_member_roles/1` and SSE fanout membership reads] → All membership reads keep resolving through the same head query; the change adds a discriminated result, not a second source of truth.

## Migration Plan

Indexer and client land together on the feature branch; dev-env federation tier exercises both sides in one gate. No data migration — the change is response-shape only. Rollback is a revert; the old blanket-retry client tolerates the old indexer by construction.

## Open Questions

None — the write-visibility successor question stays open in canon and is explicitly out of scope here.
