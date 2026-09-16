# workspace-identity Specification

## Purpose

Define what identifies a workspace across the lifetime of its keyring supersede chain, and which URI — genesis or chain head — each layer of the system uses.

A workspace exists as a chain of `at.opake.keyring` records: every rotation and membership change supersedes the current head with a new record, so the head URI churns while the workspace persists. Its stable identity is the genesis URI, and everything keyed on a workspace — indexer lookups, AEAD wrap contexts, keeper state, routes, long-lived references — keys on genesis. The head URI is what callers naturally hold, which makes the mix-up silent: head equals genesis until the first supersede, so a call site using the wrong one passes every test on a fresh workspace and fails only in workspaces with history. The rules here make that mix-up checkable at every call site, and on the Rust side unrepresentable.

Terms:

- Genesis URI: the AT-URI of the first keyring record in the chain. Stable for the workspace's lifetime.
- Head URI: the AT-URI of the current (most recently superseding) keyring record. Changes on every supersede.
- `lineage`: field on every non-genesis keyring record, holding the genesis URI. Absent on genesis itself (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`).
- `workspace_id`: the indexer-side reference to a workspace (row columns, API parameters, SSE payloads) and the `workspaceId` reference field on documents and directories. Always holds the genesis URI; never the name of the keyring's own identity field.

## Requirements

### Requirement: Genesis URI is the workspace identity

The genesis keyring URI SHALL be the sole stable identifier of a workspace, and its rkey SHALL be derived from the workspace's genesis (rotation-0) group key together with the owner's DID: the key and the DID seed a workspace identity keypair, and the rkey is an encoding of the identity public key's hash. The identity thereby commits to both segments of the URI — a workspace identity cannot be minted without holding the rotation-0 group key it names, and a given tag is valid under exactly one authority, so neither the rkey nor the owner attribution is forgeable independently.

Every keyring record after genesis SHALL carry `lineage` set to the genesis URI — the workspace is the keyring chain's object, and its identity field is the universal chain-identity field (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`), not a keyring-specific one. Any component holding a keyring record SHALL derive the workspace identity as `lineage.unwrap_or(record_uri)` — a record without `lineage` is genesis and identifies itself. Genesis integrity is enforced by the derivation check, not by a separate structural test: a genesis record's anchor is its own URI, so it adopts only if its own rkey equals the tag derived from its rotation-0 key and authority (§ Identity adoption verifies by derivation). A no-lineage record whose rkey is not its own derived tag therefore fails adoption exactly as a forged supersede does.

The resolved `Workspace.uri` SHALL be the genesis URI, regardless of which chain record the resolution started from.

Records that *reference* a workspace from outside the keyring chain (documents, directories) continue to do so via their `workspaceId` field; that field's value is the keyring chain's lineage. `lineage` always answers "which object am I"; `workspaceId` always answers "which workspace do I belong to".

#### Scenario: identity survives a supersede

- **GIVEN** a workspace whose keyring has been superseded at least once
- **WHEN** any member resolves the workspace from the current head
- **THEN** `Workspace.uri` equals the genesis URI, not the head URI

#### Scenario: identity derived from an arbitrary chain record

- **GIVEN** any keyring record in the chain (genesis, superseded intermediate, or head)
- **WHEN** a component derives the workspace identity from it
- **THEN** the result is `lineage.unwrap_or(record_uri)` and equals the genesis URI

#### Scenario: creation derives the rkey before the record exists

- **GIVEN** a workspace being created
- **WHEN** the genesis keyring record is built
- **THEN** the genesis group key was generated first, the rkey was derived from it, and every URI-bound value in the record (wrap contexts, metadata AAD) binds the resulting genesis URI

### Requirement: Identity adoption verifies by derivation

Every path that adopts a keyring record into workspace-keyed state SHALL verify the identity it is about to adopt: resolve the rotation-0 group key from the unwrapped key material (directly at rotation 0, else through the same historical-key resolution used for rotation-selected reads), derive the identity tag using the *declared* anchor's authority DID, and compare it against the rkey of the lineage anchor. Deriving from the declared authority means a forged owner attribution fails the same comparison a forged rkey does — the check covers the whole URI, offline.

The adopting paths are enumerated, because the check is only as good as its coverage of them:

- Direct resolution — `Opake::resolve_workspace_by_uri` (both branches) and `Opake::resolve_foreign_workspace` (crates/opake-core/src/opake.rs). A mismatch fails resolution with a distinct error.
- Keeper adoption — the `WorkspaceKeeper` bootstrap from `listWorkspaces` and the `keyring:upsert` patch path (entry construction in crates/opake-wasm — the entry builder unwraps the group key and holds everything the derivation needs). These are the fan-out channels a forged keyring reaches a web client through.
- CLI daemon sync — `Opake::sync_single_workspace` (crates/opake-core/src/opake.rs), fed by the indexer's member-workspace set. The CLI has no keepers, so this loop is its *sole* identity-adoption surface; it must run the same derivation check before building a `Workspace` or walking its tree, or the forged-keyring vector stays open on the CLI exactly where the keeper closes it on the web. A mismatch adopts nothing and reports a sync error.

The enumeration is the contract: any future path that keys workspace state under a declared identity — a new sync loop, a direct-PDS keeper hydration, a new client — SHALL run the check, because a single unguarded adoption path reopens the attack.

At keeper and listing surfaces a failed check SHALL be silently dropped: no entry, no placeholder, no user-facing degradation signal — trace-level logging only. A record that fails this check is a forgery targeting this user, and surfacing it in any form hands the forger a rendered artifact; this is deliberately NOT the skip-and-report posture record-validity applies to corrupt records, because a mismatch record is structurally valid and its only purpose is to be seen. Nothing downstream SHALL be keyed under the declared identity on any adopting path.

A naked-lineage keyring (lineage declared without a superseding chain) is an invalid identity claim and SHALL fail adoption on the same footing as a derivation mismatch — silently dropped at keeper/listing surfaces, distinct error on direct resolution. This is the keyring identity-*adoption* boundary; it is deliberately not the skip-with-degradation-signal posture the general chain-consumption path applies to a naked-lineage document or directory (`spec:lineage § Lineage never flips across a supersede`), where the record renders as a visible degraded node rather than being adopted as an identity. The adoption path drops uniformly because at that boundary a wrong-tag forgery and a malformed identity claim are equally not-this-workspace, and distinguishing them for the user would only inform a forger.

The check runs on every resolution, own and foreign alike — it is a single offline derivation against key material the resolver already holds, requires no network access, no chain walk, and no persisted verification state, and its cost does not grow with chain length or workspace history.

An outsider consequently cannot construct a keyring that resolves as another workspace: producing wrapped key material whose rotation-0 key derives the victim's tag is a preimage attack. Key-holders (members and ex-members) can mint identity-valid records; forks by key-holders remain the jurisdiction of chain authority enforcement, and the parties able to forge a workspace's identity are exactly the parties already trusted with its content.

A missing current-rotation wrap SHALL NOT bypass this check or require a current key when usable historical material already supplies rotation 0. Every adopting path SHALL derive and verify identity from that historical material before adopting a historical-only workspace. Failure to obtain the rotation-0 key SHALL not be replaced by trust in the declared lineage, the member list, or approval evidence: direct resolution reports unavailable identity-verification material, and keeper/listing paths adopt nothing. The no-current-wrap state is not itself an identity mismatch.

#### Scenario: impersonating keyring fails derivation

- **GIVEN** a keyring record on an attacker's PDS declaring `lineage` equal to another workspace's genesis URI, with the attacker's own group key wrapped to the target
- **WHEN** the target resolves it
- **THEN** the rotation-0 key unwraps to a key whose derived tag does not match the declared genesis rkey, resolution fails, and no state is keyed under the victim identity

#### Scenario: forged keyring arriving on the fan-out is dropped silently

- **GIVEN** a `keyring:upsert` event (or a `listWorkspaces` bootstrap entry) whose record unwraps for the local member but fails the derivation check
- **WHEN** the keeper patch or bootstrap processes it
- **THEN** no keeper entry is created or modified under the declared identity, nothing renders in listing surfaces, and the only trace is diagnostic logging

#### Scenario: forged owner attribution fails derivation

- **GIVEN** a keyring whose group key honestly derives its declared rkey, but whose declared lineage names a different DID as authority than the workspace's creator
- **WHEN** a resolver verifies the identity
- **THEN** the tag derived under the declared authority does not match, resolution fails, and no workspace attributed to the spoofed owner is adopted

#### Scenario: honest workspace resolves offline-verified

- **GIVEN** an honest keyring head, own or foreign, however deep its supersede history
- **WHEN** a member resolves it
- **THEN** the derivation check passes using only the unwrapped key material, with no chain-record fetches attributable to identity verification

#### Scenario: rollback does not disturb verification

- **GIVEN** a keyring delete whose outcome restores an earlier record as head (`spec:keyring-tombstones § Rollback restores the newest live record and re-broadcasts it`)
- **WHEN** a member re-resolves the workspace from the restored head
- **THEN** the derivation check passes identically — verification is direction-agnostic and holds no head-position state

#### Scenario: historical-only adoption still verifies identity

- **GIVEN** a head listing the local member without a current wrap but with a usable rotation-0 historical wrap
- **WHEN** direct resolution, keeper bootstrap, SSE adoption, or daemon sync processes it
- **THEN** the genesis derivation check runs before historical-only state is adopted, and a forged identity still adopts nothing

#### Scenario: membership alone cannot replace identity proof

- **GIVEN** a head names the local DID but supplies no usable material from which that client can obtain rotation 0
- **WHEN** the client attempts identity adoption
- **THEN** it does not create workspace-keyed state merely because membership or approval is present

### Requirement: Group-key wraps are AEAD-bound to genesis

Every member group-key wrap SHALL bind its AEAD context to the genesis URI, via `Keyring::lineage_anchor(self_uri)` (crates/opake-core/src/records/keyring.rs) — the keyring's lineage anchor. Every unwrap SHALL reconstruct the context the same way. Wrapping or unwrapping against a head URI is a context mismatch and SHALL NOT occur.

The same genesis binding SHALL extend one layer down to the keyring's `encryptedMetadata`: its AES-256-GCM AAD names the lineage anchor with the `keyring-metadata` type (`spec:document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type`), never a head URI. Chain advances copy the metadata ciphertext verbatim into new head records (crates/opake-core/src/opake.rs), so a head-URI binding would break every advance; the anchor is the identity the ciphertext travels under for its whole life.

#### Scenario: decrypt after supersede

- **GIVEN** a workspace superseded after a member was added
- **WHEN** that member unwraps their group key from the current head
- **THEN** the unwrap succeeds using the genesis URI as AEAD context
- Regression: `bug__superseded_keyring_decrypts_name_via_genesis_anchor` (shipped fix `2c9b32d`)

#### Scenario: keyring metadata decrypts from any head in the chain

- **GIVEN** a workspace whose keyring metadata ciphertext has been carried verbatim across one or more supersedes
- **WHEN** a member decrypts the workspace name from the current head record
- **THEN** the AAD reconstructed from the head's lineage anchor matches the AAD it was sealed under and decryption succeeds

### Requirement: Workspace-scoped indexer calls pass genesis

Every workspace-scoped indexer request (`/workspace`, `/workspace/snapshot`, `/workspace/sync`, `/workspace/chain-head`, and any membership resolution the indexer performs for them) SHALL pass the genesis URI as `workspace_id`. The indexer's `chain_heads` table is keyed on genesis; a head URI resolves to no row and is answered `workspace_not_indexed` (`spec:indexer-consistency § Unknown workspace is distinguishable from non-membership`).

That answer is the client's *retryable* class: a head URI passed as `workspace_id` is a permanent programming error wearing the transient signal, and a client cannot distinguish it from pipeline lag — the call retries to window exhaustion and surfaces as a visibility wait, not as the defect it is. The wire contract therefore cannot catch this mistake; only this requirement does. Call sites SHALL derive `workspace_id` from resolution (genesis), never from a head pointer.

#### Scenario: membership mutation after supersede

- **GIVEN** a workspace superseded at least once
- **WHEN** a manager adds, removes, or re-roles a member from the web client
- **THEN** the indexer request carries the genesis URI and succeeds, rather than a head URI burning the visibility-retry window to a timeout
- Enforced by resolve-first in the membership bindings (crates/opake-wasm/src/opake_wasm.rs; fix `e607210`)

### Requirement: Head URI use is limited to head-record operations and resolution input

The head URI SHALL be used only for: (a) PDS record operations on the head record itself — putRecord in place, or writing the record that supersedes it — and (b) as input to workspace resolution. Resolution SHALL fetch the record at the given URI to obtain the live member list, then derive genesis per the identity requirement. `resolve_workspace_by_uri` and `resolve_foreign_workspace` require head input by design; this is the sanctioned boundary between the two URI kinds.

Long-lived references — cache keys, routes, stored record fields — SHALL NOT hold a head URI.

#### Scenario: a stored record field survives membership churn

- **GIVEN** a directory record created in a workspace
- **WHEN** the workspace keyring is superseded
- **THEN** the record's `workspaceId` still identifies the workspace, because it stores the genesis id, never a head URI
- Regression: genesis-root cascade asserts `workspace_id == genesis` (crates/opake-core/src/manager/manager_tests.rs)

### Requirement: The WASM boundary resolves to genesis before core operations

Keyring URIs supplied by JS are head URIs. Every WASM binding that invokes a workspace-scoped core operation or indexer call SHALL resolve the workspace first and pass `ws.uri`. Bindings SHALL NOT forward a JS-supplied keyring URI directly into a genesis-keyed operation.

The two URI kinds SHALL be distinct types on the Rust side of the boundary: a `WorkspaceId` newtype for the genesis URI and a separate type for head URIs, so that handing one where the other is expected fails to compile. Genesis-keyed core and indexer signatures SHALL accept `WorkspaceId`, not a raw string. `Workspace::uri` and resolution are the only constructors of `WorkspaceId`; bindings obtain one by resolving, never by wrapping a JS argument.

The typed boundary ends where `WorkspaceId` would have to cross into opake-crypto or into a serialized record field. `WorkspaceId` is a domain concept: opake-crypto sits below opake-core in the dependency graph and takes wrap contexts as plain URIs (`WrapContext::Keyring`), and record fields (`KeyringUploadParams.workspace_id`) are wire format, which the newtype deliberately doesn't deserialize into. Chain-generic helpers that walk directory and keyring chains alike (`verify_and_walk_chain`) also take the genesis as a plain URI — a directory chain's genesis is not a workspace identity. Below that line call sites convert via `as_str()`, and the guarantee is carried by the typed signature above them — the last typed function before the conversion is responsible for having received a genuine `WorkspaceId`, so every `as_str()` sits directly under one.

#### Scenario: a new binding cannot skip resolution

- **GIVEN** a new WASM binding taking a keyring URI from JS
- **WHEN** it passes that argument to a genesis-keyed core or indexer function
- **THEN** compilation fails until the binding resolves the workspace and passes the resulting `WorkspaceId`

#### Scenario: membership bindings resolve first

- **GIVEN** the web client holding a head URI for SDK operations
- **WHEN** it calls addMember, removeMember, updateWorkspaceMetadata, or updateMemberRole
- **THEN** the binding resolves the workspace and passes the genesis URI to core
- Enforced in addMember/removeMember/updateWorkspaceMetadata/updateMemberRole (opake_wasm.rs)

#### Scenario: leave workspace resolves first

- **WHEN** the web client calls leaveWorkspace with a head URI
- **THEN** the binding resolves before invoking the core operation
- The binding resolves and passes `ws.id()`; core `leave_workspace` takes `&WorkspaceId`. Leave semantics are owned by `spec:workspace-membership § Removal rotates the group key; leave does not`

#### Scenario: sync-by-URI accepts what its caller holds

- **WHEN** the SDK calls syncWorkspaceByUri with a head URI after a supersede
- **THEN** the workspace is found and synced, not silently skipped
- The binding resolves first and core takes `&WorkspaceId`; an unresolvable URI errors rather than returning null. Regression: `bug__sync_workspace_by_uri_matches_envelope_by_derived_genesis`

### Requirement: SSE keyring dispatch keys on derived genesis

The SSE dispatch layer SHALL derive the workspace identity from the event's record (`record.lineage.unwrap_or(envelope.uri)` — the lineage anchor) before invoking any keeper operation. Keeper entries are keyed on genesis; passing the envelope URI is correct only for genesis events and silently wrong for every event on a superseded chain.

For keyring delete events the record is gone, so the identity SHALL come from the payload's `workspace_id`, and whether any keeper operation runs at all is governed by the payload's outcome (`spec:keyring-tombstones § Clients act on the outcome, never on URI matching`): only `torn_down` removes an entry, keyed by the payload's `workspace_id` — never by the deleted `uri`.

For upserts, removal SHALL be decided by absence of the local DID from the explicit current member list, never by absence of `wrappedKey`. A retained member without a current wrap SHALL take the identity-verified historical-only adoption path, preserving the workspace entry when that path succeeds rather than handling the event as removal.

#### Scenario: removed member's sidebar drops the workspace

- **GIVEN** member B of a workspace whose SSE consumer is connected
- **WHEN** a manager removes B, superseding the keyring, and B receives the resulting keyring event before the server unsubscribes them
- **THEN** the workspace entry is deleted from B's WorkspaceKeeper and disappears from the sidebar
- The dispatch derives the id via `IndexerEnvelope<Keyring>::workspace_id()` (crates/opake-wasm/src/sse_wasm.rs; fix `84cb5a9`). Regression: `bug__removal_supersede_drops_workspace_keyed_by_genesis`

#### Scenario: keeper contract test with head distinct from genesis

- **GIVEN** the wasm keyring dispatch under test
- **WHEN** it is fed a keyring envelope whose URI differs from its `workspace_id` and whose member list excludes the local DID
- **THEN** the genesis-keyed keeper entry is removed
- Covered by `bug__removal_supersede_drops_workspace_keyed_by_genesis` (crates/opake-core/src/indexer/workspace_keeper/tests.rs)

#### Scenario: genesis delete tombstone leaves the keeper entry

- **GIVEN** the wasm keyring dispatch tracking a workspace whose chain has superseded past genesis
- **WHEN** it is fed a keyring delete payload whose `uri` equals the tracked `workspace_id` and whose outcome is `unchanged`
- **THEN** the keeper entry survives — the delete is record cleanup, not destruction
- Regression: `bug__genesis_delete_tombstone_drops_living_workspace` (crates/opake-core/src/indexer/workspace_keeper/tests.rs)

#### Scenario: a withheld wrap is not a sidebar removal

- **GIVEN** the local member remains in the new head with a missing current wrap and usable rotation-0 history
- **WHEN** the keyring upsert is consumed
- **THEN** the identity-verified workspace entry remains, retains the member's role, and signals current-key unavailability separately

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
- **THEN** it refuses with an explicit error, without fetching the genesis record or gating on its frozen member list — callers resolve the workspace at the head and pass `ws.group_keys()` (the document-side contract is `spec:document-crypto § The PDS-only download layer will not resolve group keys itself`)
- The genesis record's member list, wrapped keys, and `keyHistory` are frozen at creation, and this layer has no indexer access to walk to the head — so it refuses instead of resolving from the past. Regression: `bug__keyring_doc_without_keys_errors_instead_of_stale_genesis_gate`

## Open questions

- Workspace destruction: deliberately unspecified for now (decided 2026-07-08). There is no destruction operation, and none can be built on record deletion: the keyring chain is distributed — genesis on the creator's PDS, each supersede on the authoring manager's PDS — so no single party can delete it, and FEDERATION.md explicitly allows the genesis record to be deleted while the workspace lives on (the genesis URI identifies the workspace, not a live record). Two constraints bind future work meanwhile:
  - A keyring delete tombstone SHALL NOT drop a workspace whose chain still has a live record. Tombstones are record cleanup, not destruction; a workspace whose last live record is deleted has no wrapped keys anywhere and its tracked state is removed (decided 2026-07-11). The delete-outcome contract is `spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome`; clients act on the indexer-resolved outcome, never on matching the deleted URI against tracked state. Regression: `bug__genesis_delete_tombstone_drops_living_workspace` (crates/opake-core/src/indexer/workspace_keeper/tests.rs).
  - When destruction is designed, the leading candidate is a terminal supersede (a keyring record marking the chain ended), not deletion: it rides the existing chain mechanics — single-manager authority, fork detection, upsert dispatch with full context — and should land with the supersede/fork custody design pass. Record litter after destruction is garbage collection: best-effort, client-initiated, out of scope here.
  - UX until then: members exit via leave or removal (the workspace-membership spec), and hiding dead workspaces is client-local state.
- Departure does not re-home documents: entries referencing a departed member's PDS stay valid but unguaranteed — the ex-member may delete records or the account outright, dangling the entries. Account deletion is the forcing function: it's unobservable in advance, so recovery-after is impossible and the answer is replication-before (mirroring encrypted blobs across member PDSes needs no key material). Custody transfer (re-home on leave, mechanically the cross-author substitute cascade) and a replication policy are queued as their own design pass (decided 2026-07-08).
- Dead pre-federation code: `keyrings/add_member.rs` and `remove_member.rs` export raw functions that wrap under a caller-supplied keyring URI with no genesis derivation. Test-only callers today. Delete, or bring under this spec before any new caller appears.

## Non-requirements

Cleared by the 2026-07-08 call-site audit and intentionally not legislated here:

- Manager operations (tree, delete, directory, move, substitute, upload) uniformly pass `ws.uri` via `FileContext::Workspace`.
- Core membership operations and `download_as_workspace_member` key both the chain-head lookup and the wrap context on genesis.
- The CLI resolves by name and holds `ws.uri` throughout.
- The indexer's SSE fan-out and topic subscription already key workspace topics on derived genesis.
- Walking DOWN a directory tree from the root needs no genesis awareness; entries are canonical by construction (see the canonical-vs-full-chain audit).
