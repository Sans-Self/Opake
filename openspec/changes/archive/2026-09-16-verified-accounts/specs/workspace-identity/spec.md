## MODIFIED Requirements

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
