# Change boundaries and approved scenario dispositions

Noï approved capturing findings R1–R5 and requested that the work not be treated as one
change. R6 is no-action, not a blocker. These are spec proposals, not implemented features.

| Change | Owns | Boundary |
|---|---|---|
| `verified-accounts` | Signed account bundles, verification resolution, transient identity authorization, key-bound consent and required caller integration | Membership/wrap separation and queued first-use safety stay here because they are necessary to implement verification without false removal or silent consent |
| `rotation-grace-periods` | R1 and R4: finite recipient waits, repair grace, ordinary expiry removal, historical-access sweep guard | Depends on verified-account member state; no clock-only revocation or guaranteed online manager |
| `rotation-write-safety` | R2: fresh encryption and truthful in-flight confidentiality | Write-path work is independent of account verification; no whole-workspace blob sweep |
| `bounded-key-history` | R3: bounded records, rotation-addressed history, permanent rotation 0, 256 current members | Requirements captured; detailed storage design and implementation explicitly gated |
| `membership-mutation-outcomes` | R5 and the ordinary R4 commit boundary: submitted/confirmed/conflict/unknown, explicit retry | Independent operation-result contract; not cross-PDS transactions or directory replay |

## Dependencies and coordinated rollout

- `membership-mutation-outcomes` can be delivered independently and is reused by grace-driven
  removals and verification-related membership mutations.
- `rotation-write-safety` fixes independent write-path behavior. The verified-account draft's
  confidentiality language now references its precise contract rather than retaining an
  unconditional wall-clock promise.
- `verified-accounts` owns verification-driven exclusion and historical-only representation.
  Do not enable that exclusion alongside the old destructive document sweep: the
  `rotation-grace-periods` access guard must ship with it, or the affected sweep stays disabled.
- `bounded-key-history` is a separate design-gated protocol change. A mechanical 4/4 artifact
  status does not lift that gate or establish massive-workspace support. Until the storage
  work lands, the current record/history limits remain real; do not advertise no history cliff.
- The isolated authorization spike is retained locally and excluded from this spec PR.
  Its feasibility findings informed the authorization contract; they do not complete any
  production task in these changes.

## Spec layering and sync order

These deltas are not all independently mergeable replacements. Canon is unchanged in this
pass. On a later authorized sync, use this order and rebase full replacement blocks against
the then-current spec before each sync:

1. `verified-accounts` — introduces account-verification and terminology plus caller integration.
2. `membership-mutation-outcomes` — adds outcome requirements and narrows membership fork UX.
3. `rotation-write-safety` — adds write confidentiality/freshness and carries forward historical-only reads.
4. `rotation-grace-periods` — adds deadline state and replaces the existing repair/sweep lifecycle.
5. `bounded-key-history` — only after layout approval; replaces storage-dependent wording while retaining all preceding decisions.

| Replacement requirement | Layering hazard |
|---|---|
| document-crypto: Keyring reads select the group key by the document's rotation | Preserve verification's historical-only errors, write-safety's confidentiality boundary, then history lookup's external storage |
| key-rotation: The rotation event is synchronous and self-sufficient | Preserve independent verification/approval decisions and confidentiality qualifications while replacing embedded history; retain grace/budget cross-references |
| background-work: Remaining work is derived from records, never stored | Grace replaces the verification draft's indefinite repair eligibility with pre-deadline repair and post-deadline removal due |
| workspace-membership: Keyring supersede authority is manager-only, except pure self-removal | Grace adds deadline preservation to the already-required role, wrap-presence, and approval preservation |
| workspace-membership: Removal rotates the group key; leave does not | History storage replaces embedded arrays, not recipient eligibility, role preservation, or no-rotation leave |
| workspace-membership: Membership state is the keyring head's member list | Preserve explicit current membership and optional wraps while replacing embedded historical-recipient wording |
| workspace-identity: Identity adoption verifies by derivation | Separate fetching rotation-0 material from the unchanged offline derivation; never bypass historical-only identity verification |
| background-work: Protocol correctness never depends on background completion | Preserve qualified readability and no-runner correctness while removing an obligatory linear history-walk model |

Do not archive or cherry-pick a full replacement as if it contained only its last paragraph.
Validate the combined effective spec and run a local semantic cross-spec review at sync time.
Purpose/open-question prose outside requirement blocks also needs reconciliation through the
authorized spec workflow; this pass does not edit canon directly.

## Remaining decisions

- Finite timeout/grace values and bounded clock-skew handling: policy tuning, no numerical TTL selected.
- Bounded history: wire layout, authenticated lookup, custody/replication, and measured byte limits
  require their own red-penned design pass before implementation.
- R6: no new self-wrap recovery requirement, task, or blocker was added.
