# workspace-identity Specification

## Purpose

Define what identifies a workspace across the lifetime of its keyring supersede chain, and which URI — genesis or chain head — each layer of the system is required to use.

A workspace is born as a single `app.opake.keyring` record. Every rotation and membership change supersedes the current head with a new record, so the head URI churns while the workspace persists. Two shipped bugs (`2c9b32d`, `e607210`) and one live finding share a single root cause: a call site handing the head URI to something keyed on genesis. Both bugs passed every test on a genesis-only workspace, because there head == genesis; they broke on the first supersede. This spec names the invariant so the class can be checked at call sites instead of rediscovered in production.

Terms:

- Genesis URI: the AT-URI of the first keyring record in the chain. Stable for the workspace's lifetime.
- Head URI: the AT-URI of the current (most recently superseding) keyring record. Changes on every supersede.
- `workspace_id`: field on every non-genesis keyring record, holding the genesis URI. Absent on genesis itself.

## Requirements

### Requirement: Genesis URI is the workspace identity

The genesis keyring URI SHALL be the sole stable identifier of a workspace. Every keyring record after genesis SHALL carry `workspace_id` set to the genesis URI. Any component holding a keyring record SHALL derive the workspace identity as `workspace_id.unwrap_or(record_uri)` — a record without `workspace_id` is genesis and identifies itself.

The resolved `Workspace.uri` SHALL be the genesis URI, regardless of which chain record the resolution started from.

#### Scenario: identity survives a supersede

- **GIVEN** a workspace whose keyring has been superseded at least once
- **WHEN** any member resolves the workspace from the current head
- **THEN** `Workspace.uri` equals the genesis URI, not the head URI

#### Scenario: identity derived from an arbitrary chain record

- **GIVEN** any keyring record in the chain (genesis, superseded intermediate, or head)
- **WHEN** a component derives the workspace identity from it
- **THEN** the result is `workspace_id.unwrap_or(record_uri)` and equals the genesis URI

### Requirement: Group-key wraps are AEAD-bound to genesis

Every member group-key wrap SHALL bind its AEAD context to the genesis URI, via `Keyring::wrap_anchor(self_uri)` (crates/opake-core/src/records/keyring.rs). Every unwrap SHALL reconstruct the context the same way. Wrapping or unwrapping against a head URI is a context mismatch and SHALL NOT occur.

#### Scenario: decrypt after supersede

- **GIVEN** a workspace superseded after a member was added
- **WHEN** that member unwraps their group key from the current head
- **THEN** the unwrap succeeds using the genesis URI as AEAD context
- Regression: `bug__superseded_keyring_decrypts_name_via_genesis_anchor` (shipped fix `2c9b32d`)

### Requirement: Workspace-scoped indexer calls pass genesis

Every workspace-scoped indexer request (`/workspace`, `/workspace/snapshot`, `/workspace/sync`, `/workspace/chain-head`, and any membership check routed through `RecordQueries.is_member?/2`) SHALL pass the genesis URI as `workspace_id`. The indexer's `chain_heads` table is keyed on genesis; a head URI resolves to no row and the caller is rejected as a non-member.

#### Scenario: membership mutation after supersede

- **GIVEN** a workspace superseded at least once
- **WHEN** a manager adds, removes, or re-roles a member from the web client
- **THEN** the indexer request carries the genesis URI and succeeds, rather than a head URI and a 403
- Shipped fix: `e607210` (crates/opake-wasm/src/opake_wasm.rs membership bindings)

### Requirement: Head URI use is limited to head-record operations and resolution input

The head URI SHALL be used only for: (a) PDS record operations on the head record itself — putRecord in place, or writing the record that supersedes it — and (b) as input to workspace resolution. Resolution SHALL fetch the record at the given URI to obtain the live member list, then derive genesis per the identity requirement. `resolve_workspace_by_uri` and `resolve_foreign_workspace` require head input by design; this is the sanctioned boundary between the two URI kinds.

Long-lived references — invitation targets, cache keys, routes, stored record fields — SHALL NOT hold a head URI.

#### Scenario: invitation target survives membership churn

- **GIVEN** an invitation created for a workspace
- **WHEN** the workspace keyring is superseded before the invitation is redeemed
- **THEN** the invitation's `target` still identifies the workspace
- Conformant since the WorkspaceId pass: the binding resolves first and core stores the genesis id as `target` (audit finding 3; regression `bug__create_invitation_stores_genesis_target`)

### Requirement: The WASM boundary resolves to genesis before core operations

Keyring URIs supplied by JS are head URIs. Every WASM binding that invokes a workspace-scoped core operation or indexer call SHALL resolve the workspace first and pass `ws.uri`. Bindings SHALL NOT forward a JS-supplied keyring URI directly into a genesis-keyed operation.

The two URI kinds SHALL be distinct types on the Rust side of the boundary: a `WorkspaceId` newtype for the genesis URI and a separate type for head URIs, so that handing one where the other is expected fails to compile. Genesis-keyed core and indexer signatures SHALL accept `WorkspaceId`, not a raw string. `Workspace::uri` and resolution are the only constructors of `WorkspaceId`; bindings obtain one by resolving, never by wrapping a JS argument.

The typed boundary ends where `WorkspaceId` would have to cross into opake-crypto or into a serialized record field. `WorkspaceId` is a domain concept: opake-crypto sits below opake-core in the dependency graph and takes wrap contexts as plain URIs (`WrapContext::Keyring`), and record fields (`Invitation.target`, `KeyringUploadParams.workspace_id`) are wire format, which the newtype deliberately doesn't deserialize into. Chain-generic helpers that walk directory and keyring chains alike (`verify_and_walk_chain`) also take the genesis as a plain URI — a directory chain's genesis is not a workspace identity. Below that line call sites convert via `as_str()`, and the guarantee is carried by the typed signature above them — the last typed function before the conversion is responsible for having received a genuine `WorkspaceId`, so every `as_str()` sits directly under one.

#### Scenario: a new binding cannot skip resolution

- **GIVEN** a new WASM binding taking a keyring URI from JS
- **WHEN** it passes that argument to a genesis-keyed core or indexer function
- **THEN** compilation fails until the binding resolves the workspace and passes the resulting `WorkspaceId`

#### Scenario: membership bindings resolve first

- **GIVEN** the web client holding a head URI for SDK operations
- **WHEN** it calls addMember, removeMember, updateWorkspaceMetadata, or updateMemberRole
- **THEN** the binding resolves the workspace and passes the genesis URI to core
- Conformant since `e607210`

#### Scenario: leave workspace resolves first

- **WHEN** the web client calls leaveWorkspace with a head URI
- **THEN** the binding resolves before invoking the core operation
- Conformant since the WorkspaceId pass: the binding resolves and passes `ws.id()`; core `leave_workspace` takes `&WorkspaceId` so its future implementation can't regress (audit finding 5; core is still an unimplemented stub)

#### Scenario: sync-by-URI accepts what its caller holds

- **WHEN** the SDK calls syncWorkspaceByUri with a head URI after a supersede
- **THEN** the workspace is found and synced, not silently skipped
- Conformant since the WorkspaceId pass: the binding resolves first and core takes `&WorkspaceId` (audit finding 4; regression `bug__sync_workspace_by_uri_matches_envelope_by_derived_genesis`). Behavior note: an unresolvable URI now errors instead of returning null; no production caller depended on the null.

### Requirement: SSE keyring dispatch keys on derived genesis

The SSE dispatch layer SHALL derive the workspace identity from the event's record (`record.workspace_id.unwrap_or(envelope.uri)`) before invoking any keeper operation. Keeper entries are keyed on genesis; passing the envelope URI is correct only for genesis events and silently wrong for every event on a superseded chain.

#### Scenario: removed member's sidebar drops the workspace

- **GIVEN** member B of a workspace whose SSE consumer is connected
- **WHEN** a manager removes B, superseding the keyring, and B receives the resulting keyring event before the server unsubscribes them
- **THEN** the workspace entry is deleted from B's WorkspaceKeeper and disappears from the sidebar
- Currently violated: dispatch passes `envelope.uri` (the new head) to `apply_keyring_record`, whose removal branch deletes by that key and no-ops against the genesis-keyed entry (audit finding 1, crates/opake-wasm/src/sse_wasm.rs:982; the tree dispatch at :496 derives correctly)

#### Scenario: keeper contract test with head distinct from genesis

- **GIVEN** the wasm keyring dispatch under test
- **WHEN** it is fed a keyring envelope whose URI differs from its `workspace_id` and whose member list excludes the local DID
- **THEN** the genesis-keyed keeper entry is removed
- Currently missing: existing keeper tests insert and delete under one URI and never exercise head != genesis (audit finding 7)

### Requirement: Membership authority is the live chain head

Membership checks SHALL evaluate the member list of the resolved chain head, never the genesis record. The genesis member list is a historical artifact; members added after genesis do not appear in it.

#### Scenario: post-genesis member opens a pre-membership document

- **GIVEN** a member added after genesis
- **WHEN** they download a document uploaded before they joined
- **THEN** access is granted via their unwrapped group key
- Regression: `bug__post_genesis_member_opens_pre_membership_document` (shipped fix `2c9b32d`)

#### Scenario: a layer that cannot reach the head makes no membership decision

- **GIVEN** a keyring-encrypted document and a caller without pre-resolved group keys
- **WHEN** the PDS-only download layer is asked for the content key
- **THEN** it refuses with an explicit error, without fetching the genesis record or gating on its frozen member list — callers resolve the workspace at the head and pass `ws.group_keys()`
- Conformant since the finding-2 fix: the auto-resolve branch was removed rather than repaired — the genesis record's member list, wrapped keys, and `keyHistory` are all frozen at creation, and the layer has no indexer access to walk to the head (audit finding 2; regression `bug__keyring_doc_without_keys_errors_instead_of_stale_genesis_gate`)

## Open questions

- Workspace destruction: deliberately unspecified for now (decided 2026-07-08). There is no destruction operation, and none can be built on record deletion: the keyring chain is distributed — genesis on the creator's PDS, each supersede on the authoring manager's PDS — so no single party can delete it, and FEDERATION.md explicitly allows the genesis record to be deleted while the workspace lives on (the genesis URI identifies the workspace, not a live record). Two constraints bind future work meanwhile:
  - A keyring delete tombstone SHALL NOT drop a workspace from tracked state. Tombstones are record cleanup, never destruction. The current keeper behavior violates this by accident — `delete(payload.uri)` matches when the deleted record is genesis — and needs correcting whenever the dispatch is next touched (audit finding 6).
  - When destruction is designed, the leading candidate is a terminal supersede (a keyring record marking the chain ended), not deletion: it rides the existing chain mechanics — single-manager authority, fork detection, upsert dispatch with full context — and should land with the supersede/fork custody design pass. Record litter after destruction is garbage collection: best-effort, client-initiated, out of scope here.
  - UX until then: members exit via leave/removal (leave shipped 2026-07-08 as a self-removal keyring supersede — any role, no rotation, indexer-enforced pure-leave check), and hiding dead workspaces is client-local state.
- Departure does not re-home documents: entries referencing a departed member's PDS stay valid but unguaranteed — the ex-member may delete records or the account outright, dangling the entries. Account deletion is the forcing function: it's unobservable in advance, so recovery-after is impossible and the answer is replication-before (mirroring encrypted blobs across member PDSes needs no key material). Custody transfer (re-home on leave, mechanically the cross-author substitute cascade) and a replication policy are queued as their own design pass (decided 2026-07-08).
- Dead pre-federation code: `keyrings/add_member.rs` and `remove_member.rs` export raw functions that wrap under a caller-supplied keyring URI with no genesis derivation. Test-only callers today. Delete, or bring under this spec before any new caller appears.

## Non-requirements

Cleared by the 2026-07-08 call-site audit and intentionally not legislated here:

- Manager operations (tree, delete, directory, move, substitute, upload) uniformly pass `ws.uri` via `FileContext::Workspace`.
- Core membership operations and `download_as_workspace_member` key both the chain-head lookup and the wrap context on genesis.
- The CLI resolves by name and holds `ws.uri` throughout.
- The indexer's SSE fan-out and topic subscription already key workspace topics on derived genesis.
- Walking DOWN a directory tree from the root needs no genesis awareness; entries are canonical by construction (see the canonical-vs-full-chain audit).
