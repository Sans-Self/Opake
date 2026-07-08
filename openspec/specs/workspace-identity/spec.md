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
- Currently violated: `create_invitation` stores the caller-supplied `keyring_uri` verbatim, and the binding receives `headUri` (audit finding 3; latent, no mounted redemption path)

### Requirement: The WASM boundary resolves to genesis before core operations

Keyring URIs supplied by JS are head URIs. Every WASM binding that invokes a workspace-scoped core operation or indexer call SHALL resolve the workspace first and pass `ws.uri`. Bindings SHALL NOT forward a JS-supplied keyring URI directly into a genesis-keyed operation.

The two URI kinds SHALL be distinct types on the Rust side of the boundary: a `WorkspaceId` newtype for the genesis URI and a separate type for head URIs, so that handing one where the other is expected fails to compile. Genesis-keyed core and indexer signatures SHALL accept `WorkspaceId`, not a raw string. `Workspace::uri` and resolution are the only constructors of `WorkspaceId`; bindings obtain one by resolving, never by wrapping a JS argument.

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
- Currently violated: the binding forwards the JS URI unresolved (audit finding 5; benign only because core `leave` is an unimplemented stub)

#### Scenario: sync-by-URI accepts what its caller holds

- **WHEN** the SDK calls syncWorkspaceByUri with a head URI after a supersede
- **THEN** the workspace is found and synced, not silently skipped
- Currently violated: core matches the argument against derived genesis URIs, so a head URI misses and returns null (audit finding 4; no production caller today)

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

#### Scenario: auto-resolve download path

- **GIVEN** a post-genesis member downloading without pre-resolved keys
- **WHEN** the download auto-resolves the keyring from the document's `keyring_ref` (which holds genesis)
- **THEN** the member is not rejected on the genesis record's member list
- Currently violated: crates/opake-core/src/documents/download.rs:172 gates on the genesis record's members (audit finding 2; latent — FileManager::download always passes keys, reachable via the cabinet-sharing path)

## Open questions

- Keyring delete tombstones: a `KeyringDelete` event carries only the deleted record's URI. Deleting a superseded old head should not remove the workspace; deleting the entire workspace should. Whether workspace destruction emits a delete on genesis, on the head, or on every chain record is undefined — the indexer acknowledges the same ambiguity (events_controller.ex:118). Dispatch semantics can't be specified until destruction semantics are (audit finding 6).
- Dead pre-federation code: `keyrings/add_member.rs` and `remove_member.rs` export raw functions that wrap under a caller-supplied keyring URI with no genesis derivation. Test-only callers today. Delete, or bring under this spec before any new caller appears.

## Non-requirements

Cleared by the 2026-07-08 call-site audit and intentionally not legislated here:

- Manager operations (tree, delete, directory, move, substitute, upload) uniformly pass `ws.uri` via `FileContext::Workspace`.
- Core membership operations and `download_as_workspace_member` key both the chain-head lookup and the wrap context on genesis.
- The CLI resolves by name and holds `ws.uri` throughout.
- The indexer's SSE fan-out and topic subscription already key workspace topics on derived genesis.
- Walking DOWN a directory tree from the root needs no genesis awareness; entries are canonical by construction (see the canonical-vs-full-chain audit).
