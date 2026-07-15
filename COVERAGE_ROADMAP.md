# Coverage roadmap

Full-coverage drive on two axes:

- **Requirement coverage** — every canon-spec requirement carries at least one cited
  test at the right tier. Tracked mechanically: `python3 scripts/spec_lint.py --coverage`.
- **Feature coverage** — every user-visible action (per the 2026-07-12 surface
  inventory) has an e2e or CLI test exercising it. Tracked in the checklist below.
  Feature tests cite requirements where one exists; where none does, they simply don't
  cite — canon stays protocol-shaped, we do not invent spec capabilities for UX flows.

One batch ≈ one cowboy brief. Opus designs tests and touches anything crypto/protocol-
adjacent; sonnet follows established patterns. Ordering: F1 first (highest-traffic
surface, establishes the pattern library), then protocol batches interleave.

## Ground truth (2026-07-13, post batch 1 + key-rotation + F1)

- Ledger: 274 citations, 0 dangling, 77/96 requirements cited (17 canon specs).
- The 19 uncited requirements map as: 9 infra-self-referential (dev-env 5,
  e2e-testing 4 → batch 7), 2 workspace-identity (SSE dispatch on genesis, WASM
  genesis resolution → batch 2), 3 tree-chains/tree-cabinet (dir-no-crypto-envelope,
  concurrent-fork [design-gated], cabinet fixed root → batches 2/3), 2 background-work
  (bg-completion independence, honest tiers → batch 7-adjacent), 3 stragglers
  (auth-identity publicKey record, document-crypto per-doc content key,
  indexer-consistency acceptance≠visibility).
- Web e2e now 14 spec files (F1 cabinet suite, sharing, background-work cross-tier,
  device pairing, auth/session families); federation tier includes the rotation suite.

## Verdicts (Noï, 2026-07-12) — settled

1. Invitations: descope the UI now (hide InviteDialog, no more dead links); finishing
   the accept/onboarding flow is its own future change. Lexicon + SDK stay.
   SUPERSEDED 2026-07-12 (evening): invitations removed wholesale — lexicons, core,
   WASM, SDK — via change `drop-invitations-and-grant-expiry`. Sharing to a not-ready
   DID warns + offers the pending-share queue instead.
2. Fork-replay: spec amended to detection-only canon; replay is an open design pass.
3. Workspace deletion keeps its honest stub; trash + encrypted UI stubs are removed
   (they promise unbacked behavior).
4. Fake-pds tier: split — keep the 42 passing local-only tests as the fast tier, port
   the indexer-dependent scenarios to the federation tier (batches 4/5), delete the rest.
   SUPERSEDED 2026-07-12 (evening): abandon fake-pds ENTIRELY. No split, no fast tier —
   the pairing base64 bug proved the class: fake-pds preserves padding (and who knows
   what else) that a real PDS does not, so green fake-pds tests can certify broken
   flows. Port anything still worth keeping (login/recover/config/multi-account UX,
   rotation scenarios) to the dev-env federation tier; delete the rest, drop the
   `fake-pds` dependency, and retire the `e2e-cli` justfile target or repoint it at
   the federation tier. Reshapes F5 and batch 4's porting plans: everything lands in
   the federation tier now.

## Feature axis

### F1 — cabinet file management — DONE (2026-07-12: cabinet-tree suite + cycle-rejection)
Web e2e, one spec file per cluster: upload into a subfolder; download from a subfolder;
new folder + nested folder; rename directory; move document between folders incl.
cycle-rejection UX; delete file; delete folder recursively (descendant-count confirm);
tree navigation + state across reload. Cite directory-chains / document-crypto
requirements where they genuinely apply.

### F2 — editor + metadata dialogs (sonnet; DONE 2026-07-13, 6/6 green)
New note → edit → save → persists across reload; edit existing document content;
metadata dialog rename / describe / tag add+remove; preview rendering.

### F3 — sharing UX (merged into protocol batch 1 below)

### F4 — workspace daily driver (sonnet; DONE 2026-07-14, 3/3 green)
Upload/edit/delete inside a workspace context; workspace editor; role-change UI;
workspace metadata (name/icon) — the settings page beyond what lifecycle covers.

### F5 — account lifecycle (opus)
Web recovery-from-mnemonic flow (RecoverIdentityView); settings indexer-URL change;
logout clears local state; CLI local tier (from the fake-pds split) keeps login/
recover/config/multi-account UX covered.

### F6 — smoke tier (sonnet, minutes)
Docs pages render; docs search opens/filters; landing page loads.

## Feature checklist (surface inventory × coverage)

Status: ✓ covered · ○ planned (batch) · — out of scope
- Cabinet: upload root ✓(document-roundtrip) · upload subfolder ✓(F1) · download ✓ ·
  new folder ✓(F1) · rename dir ✓(F1) · move ✓(F1) · delete file ✓(F1) ·
  delete folder ✓(F1) · metadata edit ✓(F2) · notes editor ✓(F2) · preview ✓(F2)
- Workspaces: create ✓ · rename ✓ · add member ✓ · leave ✓(federation) ·
  remove member ○B4 · role change ✓(F4) · ws file ops ✓(F4) · ws editor ✓(F4) ·
  delete = honest stub ✓(pinned)
- Sharing: share ✓(B1) · inbox ✓(B1) · download-from-grant ✓(B1) · revoke ✓(B1) ·
  pending queue ✓(B1) · invitations — removed wholesale
- Auth/devices: OAuth login ✓ · callback errors ✓ · session restore ✓ ·
  pairing ○B5 · recovery (web) ○F5 · logout ○F5 · CLI login/recover ✓(fast tier)
- Daemon: pair-cleanup ✓(unit) · share-retry ✓(unit) · grant-healing ○B1-adjacent ·
  session-refresh ○B5 · bulk re-encryption — blocked on product fix
- Docs/marketing: ○F6

## Protocol axis (unchanged numbering)

## Batch 0 — DONE (2026-07-12): ledger seeded, 48 → 129 citations, 0 dangling.

## Batch 1 — sharing-grants — DONE (2026-07-12 evening: 6/6 requirements cited, incl. pending-share queue; invitations removed wholesale so the spec-pass item below is moot)

1. Spec pass first: the invitation story is half-dead (create/list/revoke wired in web,
   `/invite` accept route missing, no CLI equivalent) — needs a product verdict
   (finish or descope) before tests can cite it.
2. Web e2e: share to cross-PDS actor → recipient inbox shows it (SSE) → download from
   grant decrypts → revoke → discovery stops. Two browser contexts, membership-spec
   pattern.
3. Federation tier: pending-share queue (share to an actor whose publicKey record is
   deleted/not yet published → queued → publish key → retry completes). Needs a
   fixture actor variant without a seeded pubkey, or an unpublish helper.
4. Unit: "Sharing is cabinet-only" (workspace-context share refused) — currently
   untested anywhere; "revocation does not stop historical access" — unit-level
   (old ciphertext + cached key still decrypts).

## Batch 2 — workspace-identity regression net — DONE (2026-07-14 night, `9c9ca64`)

Prerequisite contract shipped first as `disambiguate-workspace-403` (`c97e0e8`):
404+`workspace_not_indexed` vs definitive 403, client retry narrowed to the body
code, name→workspace resolution brought inside the visibility retry (the CLI
create-then-mutate first-response failure the old save-retry masked).
Batch itself: leave-on-churned-chain (federation), rename+cross-PDS-add on a
twice-superseded chain (web e2e, mutation-verified — head-URI-keyed resolution
fails it), SSE keeper dispatch keyed on workspace_id (unit), publicKey both-KEM
-halves and per-upload fresh key+nonce (units), acceptance≠visibility snapshot
-served-not-necessarily-containing (federation). Coverage 80/97 → 85/97, 329
citations 0 dangling. Membership-authority-is-live-chain-head e2e was already
covered by identity-churn (roadmap text predated it). "Invitation target survives
churn" dead — invitations removed. Findings: mnemonic-serde substring flake (#30),
aged-stack soak data on #5, #3 resolved-pending-signoff.

## Batch 3 — directory-chains federation tier (opus; protocol-heavy)

Cross-author document edit e2e (the known missing test: editor B edits author A's doc
via documentUpdate supersede); editor non-additive supersede rejected end-to-end by the
indexer (authority.ex says no → client sees rejection); cascade ordering through the
real pipeline. EXCLUDED until a design verdict: "Concurrent supersedes fork, loser
replays" — no test anywhere and possibly described-only (memory flag); needs Noï's
call on whether the behavior exists before anything can cite it.

## Batch 4 — rotation + document-crypto e2e — MOSTLY DONE (2026-07-13)

The key-rotation change landed the federation rotation suite: all 5 key-rotation
requirements cited (rotation self-sufficient, key-history reads, re-wrap as hygiene).
Remaining crumb: document-crypto "Each document has its own random content key" is
uncited — likely a unit-level property assertion, fold into a straggler pass.

## Batch 5 — auth/session + pairing e2e (opus)

Device pairing end-to-end (request → approve → new device decrypts) — unit-tested,
zero e2e; the pair.request orphan-cleanup fix (2026-07-12) needs a regression e2e
(navigate away mid-pair → request cancelled on the PDS). Session: proactive refresh
keeps a session alive across token expiry (shortened TTL or clock manipulation);
logout clears local state. Cites wasm-security-boundary § JS auth-state access is
expiry-timestamp-only (currently uncited).

## Batch 6 — indexer controller + parser unit tests — DONE (2026-07-14: 54 tests, suite 154/0)

Controllers (403 non-member, pagination, snapshot/sync shapes), `jetstream/event.ex`
parser, broadcaster incl. delete-fanout regression pin. Deliberately NOT pinned: the
workspace-403-race semantics (`is_member?` conflates not-member with not-yet-indexed)
— that contract decision moves to batch 2. Pipeline-e2e convention confirmed to have
no surviving file; the CLAUDE.md claim is stale and should be corrected, not restored.

## Batch 7 — infra self-coverage (sonnet; last)

dev-env + e2e-testing spec requirements: mostly enforced by construction (blockade,
spec-lint) — decide per requirement: cite the enforcing mechanism in the spec text, or
add a meta-test (e.g. reset→bootstrap→stable-encryption-key assertion as a federation
test). Cheap idea for wasm-security-boundary § no-export-returns-secrets: a lint/test
over the generated `.d.ts` asserting no export type mentions session/token/private-key
shapes — mechanical enforcement instead of trust.

## Standing product-bug gates (block or shape batches)

- Web first-write gap (empty cabinet upload short-circuits) — blocks a "fresh web-only
  user" e2e; ticket exists.
- Creator-mutates-fresh-workspace 403 race — shapes batch 2.
- Boot-hang under accumulated workspaces (~6 suite runs to failure) — operational drag
  on every e2e batch until fixed or auto-reset is wired into the harness.
- Workspace deletion unimplemented product-wide + trash/encrypted UI stubs — spec
  verdict needed (destruction is deliberately unspecified per workspace-identity).
- heal_stale_grants re-wrap path — RESOLVED 2026-07-13: rotation is self-sufficient
  and the re-wrap sweep is hygiene under the background-work contract (cited 4×).
- Fake-pds CLI tier: decide revive/migrate/retire — its scenarios are the seed corpus
  for batches 4 and 5.
