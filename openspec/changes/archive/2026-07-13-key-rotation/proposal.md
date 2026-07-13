# Proposal: key-rotation

## Why

Rotation's trigger and read path are already canon — workspace-membership owns "Removal rotates the group key; leave does not" and document-crypto owns rotation-selected key resolution with `keyHistory` fallback. What canon does not cover is the *lifecycle between those two points*, and that gap holds live defects and a dead mechanism:

- A live client's projection does not survive rotation: the tree keeper's rotation handling bumps the counter and invalidates decrypted names but never adopts the new group key nor archives the prior one — after an in-place rotation, directory names decrypt to placeholders indefinitely and the historical fallback cannot recover what was never archived. No requirement exists for it to violate.
- Bulk re-encryption is implemented with zero callers. Nothing states whether migrating old wraps to the new key is required for correctness, optional hygiene, or dead weight — and the answer decides whether a mechanism with no writer gets wired or deleted.
- Key history grows monotonically for an unswept workspace, and nothing records that this is the accepted cost of the design rather than an oversight.

The standing product gate — rotation does not ship beyond us until this story is sound — currently anchors to folklore. This change anchors it to canon, and takes its background posture from the background-work contract: rotation is *correct* the moment the keyring supersede lands; everything after is hygiene.

## What Changes

- New canon capability `key-rotation`: the rotation event is synchronous and self-sufficient; live projections adopt a rotation (new key in use, prior key archived) without re-bootstrap; the re-wrap sweep is an optional background task under the background-work contract; unbounded history on unswept workspaces is the recorded cost; new members receive history access so pre-membership-era documents shared into their tenure remain readable per document-crypto's read rules.
- Fix the live-projection defect: keepers apply rotation events completely (adopt new group key, archive prior to history, re-decrypt names).
- Bulk re-encryption verdict (red-pen decision, drafted as): the existing zero-caller implementation is replaced by the sweep-as-background-task — re-implemented against the background-work contract (per-item derivation, CAS-conditioned writes) rather than wired as-is; the old mechanism is deleted. If red-pen prefers wiring the existing code, the delta text stands and only the design/tasks change.
- New documentation: rotation lifecycle section in docs/CRYPTO.md (event vs sweep, history growth, what rotation does and does not protect), FLOWS.md rotation + sweep sequence diagrams.

## Capabilities

### New Capabilities
- `key-rotation`: the rotation lifecycle between the membership trigger and the document-crypto read path — event self-sufficiency, live-projection obligations, the sweep, history cost.

### Modified Capabilities
<!-- None drafted. workspace-membership §Removal-rotates and document-crypto §rotation-selected-reads stay authoritative for trigger and reads; this capability cites both. Crossref review checks whether membership's open question on post-leave auto-rotation should move here — expected disposition: it stays a membership policy question. -->

## Impact

- **Core:** keeper rotation handling (tree/workspace keepers), group-key adoption + archival; sweep task (new, under the background-work contract); deletion of the uncalled bulk re-encryption path.
- **WASM/web:** no new surface expected; keeper fix flows through existing events. Web runs the sweep opportunistically via the existing maintenance timers; the daemon drains it.
- **Tests:** keeper rotation regression (the names-stay-readable scenario); sweep derivation/CAS/idempotence units; federation-tier rotation e2e (batch 4 of the coverage roadmap lands against these requirements).
- **Docs:** CRYPTO.md, FLOWS.md, BACKGROUND_WORK.md task table entry.
- **Sequencing:** depends on background-work (cites its contract); syncs after it.
