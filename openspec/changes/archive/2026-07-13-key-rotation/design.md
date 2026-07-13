# Design: key-rotation

## Context

Two of the five requirements document shipped truth (event self-sufficiency, history-as-cost); one pins a live defect (projection adoption); one re-founds a dead mechanism on the background-work contract (the sweep); one needs code verification before apply (new-member history access). Sequenced after background-work — the sweep requirement cites its contract.

## Decisions

### Keeper rotation adoption: fix by completing the handler

The tree keeper's `KeyringUpsert` rotation path today bumps the rotation counter and calls name-invalidation, but neither swaps the active group key nor archives the prior one — so post-rotation, invalidated names can never re-decrypt (old key gone from the keeper's view, never archived) and new entries can't decrypt either (new key never adopted). The fix is completing the handler to mirror what a fresh bootstrap would produce: adopt the event's key material, retain the prior key for historical resolution, trigger re-derivation of key-derived caches. The invariant (projection after event ≡ projection after re-bootstrap) is the test's shape, and it belongs to the same family as keeper idempotency under `spec:indexer-consistency § Snapshot and stream jointly lose nothing`.

Workspace keeper and inbox keeper are swept for the same class (rotation-relevant state held stale); expected clean — the sweep result is reported evidence.

### Sweep: re-implement on the contract, delete the dead path

**Settled at review: delete and re-implement.** The existing bulk re-encryption implementation (complete, zero callers, predates the background-work contract) is deleted, and the sweep is implemented fresh as a background task: derive per item (wrap rotation < head rotation), re-wrap under the head key resolved at write time, write CAS-conditioned, skip on conflict. Rationale: the old path was written as a drain-everything batch without per-item CAS or head re-resolution — a mid-sweep second rotation would write wraps to a superseded rotation, exactly what the delta forbids — so retrofitting costs more than rewriting against the contract, and dead mechanisms rot into false documentation. Wiring the existing code as-is was considered and rejected.

The sweep registers in both runner tiers: daemon drains it on its task loop; web runs it opportunistically on the existing maintenance timers. Per the contract, neither registration is load-bearing.

### New-member history: verify, then implement the gap if real

The requirement says admission grants historical keys. Whether the current add-member flow already does this depends on the keyHistory wire shape — whether history entries carry per-member wraps that the admitting supersede extends to the new member, or only the members-at-that-rotation's wraps (in which case a post-rotation joiner cannot resolve old rotations and the requirement forces a change: the admitting manager re-wraps historical keys to the joiner). Apply step one is reading `keyring.rs`/lexicon and the manager add path and reporting which world we're in; implementation follows only if the gap is real. If it is real, the fix rides the admitting supersede (synchronous, manager holds all keys) — not the sweep.

### What rotation does not do (docs burden)

Rotation is forward secrecy only. It does not revoke content a former member already fetched or could have unwrapped (`spec:sharing-grants` documents the same posture for grants), and the re-wrap sweep has zero security effect. CRYPTO.md's rotation section states this in plain language because it is the most commonly misunderstood property of the design.

## Documentation (first-class deliverable)

- **docs/CRYPTO.md**: rotation lifecycle section — the event (what one supersede accomplishes), key history and rotation-selected reads (pointer to existing material), the sweep (what it optimizes, what it cannot protect), history growth, the forward-secrecy-only guarantee stated bluntly.
- **docs/FLOWS.md**: two sequence diagrams — the rotation event (remove-member → mint → wrap → supersede → SSE → keeper adoption), and the sweep with a two-runner CAS race (shared with BACKGROUND_WORK.md's walkthrough).
- **docs/BACKGROUND_WORK.md**: sweep row in the task table (created by the background-work change).

## Sync notes (for the canon merge)

- New capability dir `openspec/specs/key-rotation/` from the delta as-is.
- Non-requirement to record: rotation-triggered revocation of previously accessible content (forward secrecy only; same class as sharing-grants' historical-access posture).
- Open question to carry: automatic rotation after leave — remains workspace-membership's open question (policy, not lifecycle); this capability takes no position.
- Crossref: workspace-membership's open-question prose mentioning "unreliable bulk re-encryption" becomes stale once the sweep replaces it — expected small wording delta there or an editorial note, per crossref review.

## Testing

- Keeper adoption: unit — rotation event on a live keeper ≡ fresh bootstrap (names re-decrypt, new-rotation entries decrypt); regression named for the names-stay-readable behavior.
- Sweep: units for derivation (mixed-rotation wrap set → exact remainder), CAS-conflict skip, head re-resolution mid-sweep; federation-tier interrupted-sweep resume.
- Rotation e2e (coverage roadmap batch 4 lands here): remove member → rotates; removed member cannot decrypt new upload; remaining member reads pre-rotation doc live (no reload — cites projection adoption); post-rotation joiner reads old doc (cites new-member history).
- History depth: unit with a many-rotation keyring fixture resolving the oldest rotation.
