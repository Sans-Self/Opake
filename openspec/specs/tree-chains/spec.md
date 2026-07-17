# tree-chains Specification

## Purpose

Define the structure of a workspace's directory records: how a path's canonical state is a supersede chain rather than a single record, what an editor is allowed to write versus a manager, how a consumer recovers the live tree from an indexer snapshot that hands back whole chains, and the write ordering that keeps the indexer's authority check sound.

Directories are the coordinative half of the federation model. Documents federate — each member owns their own doc records — but a path has exactly one canonical directory at any moment, and every member with authority takes curatorial turns superseding it, so the chain for a single path alternates PDSes. Two properties of that arrangement demand care from every consumer and every writer:

- The indexer snapshot returns every record in a chain, superseded predecessors included. Consumers select chain heads before building any view; code that treats an arbitrary chain member as canonical renders stale entries, walks up through superseded parents, or reports live entries as unreachable.
- Editors may only grow a listing, never shrink it — but a wiki-style edit (replace a doc with an edited successor) looks like a shrink. The additivity rule distinguishes the two precisely, and it is mirrored in two codebases (indexer and client) that must agree: a rule too loose lets an editor delete another member's entry, too strict falsely rejects a legitimate edit.

This spec owns directory chain structure, additivity, the canonical view, cascade write ordering, and fork handling. It does not restate workspace identity — the genesis keyring URI and the `WorkspaceId` newtype are the workspace-identity spec's; directory chains reference workspace identity as a given. Keyring/membership authority (who is a manager) and document content crypto are their own specs.

Terms:

- Canonical directory: at a path P, the head of P's supersede chain — the directory record no other record supersedes. The live tree is built from canonical records only.
- Curatorial write: a supersede of a path's canonical by any member with directory authority. It is a full restatement of the directory's entries, not a delta.
- Cascade: the leaf-to-root sequence of directory writes that propagates one child mutation up through every ancestor whose pinned child CID changed.
- Advance: an editor replacing entry A with entry B where B's record `supersedes` A — a doc edit or directory rename expressed additively, distinct from a bare drop.

## Requirements

### Requirement: A directory is an organizational record with no crypto envelope over its listing

A directory record (`at.opake.directory`) SHALL be a distinct record type whose listing carries only structure, not content. Each entry SHALL pin a target AT-URI and the target record's CID at write time (`#listingEntry`: `target` + `targetCid`); it SHALL NOT carry the target's name, type, or any other metadata. The directory's own name lives in its `encryptedMetadata`, decrypted under the directory's content key; entry names live in each target record's `encryptedMetadata` and are resolved by fetching the target. Entry kind (document vs directory) SHALL be derived from the target URI's collection segment, not stored.

The `targetCid` pin exists so a consumer can content-address a subtree and so the indexer can detect a conflicting concurrent supersede on a child path without refetching every target.

#### Scenario: listing exposes no plaintext names to the PDS

- **GIVEN** a directory record stored on a member's PDS
- **WHEN** the PDS or any unauthorized reader inspects the record
- **THEN** it sees a list of `(target, targetCid)` pairs and an encrypted metadata blob, and no entry name or type
- Verified in `lexicons/at.opake.directory.json` (`entries` → `#listingEntry`) and `DirectoryTree::from_records`, which projects entries to bare target URIs

### Requirement: A path's canonical state is the head of a supersede chain

Each directory path SHALL be represented by a supersede chain: every curatorial write produces a new directory record with `supersedes` set to the prior canonical's URI and updated `entries`. The canonical directory at a path is the record no other record in that path's chain supersedes. Records SHALL NOT carry a `parent` field; upward traversal is resolved by the indexer, and back-walking a chain follows `supersedes` (crates/opake-core/src/directories/chain.rs::`walk_back_to_genesis`).

A curatorial write is a full statement of the directory's entries. The writer SHALL fetch the prior canonical, apply its modification, and carry forward every unmodified prior entry — the chain records state, not diffs.

#### Scenario: successive edits alternate PDSes on one chain

- **GIVEN** a directory whose canonical record lives on member A's PDS
- **WHEN** member B (with authority) supersedes it with a record on B's own PDS carrying `supersedes: <A's record URI>`
- **THEN** B's record becomes the canonical for that path, and the chain now spans both PDSes
- Genesis (the first record at a path) has no `supersedes`; see FEDERATION.md "Directory" and "Curatorial writes"

### Requirement: The workspace root is a flag-marked chain, forward-walked from genesis

The workspace-root path SHALL have no deterministic URI or rkey convention. Every record in the root chain SHALL be an ordinary directory record with a client-generated TID rkey (`spec:lineage § Records that seal ciphertexts to their own URI choose their own rkey` — the genesis root's metadata AAD binds its own URI, so the URI must be known before encryption), marked `isWorkspaceRoot: true` and carrying `workspaceId` set to the genesis keyring URI (`spec:workspace-identity § Genesis URI is the workspace identity`). Like every directory record, root-chain records after genesis carry `lineage` set to the root chain's genesis URI (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`).

Lineage names the chain; it does not resolve the head. A consumer SHALL find the current root by taking the flagged record that no other record supersedes, and SHALL advance to the head by walking `supersedes` back-edges forward, never by pinning the genesis record.

Creating the first directory in a fresh workspace SHALL genesis-cascade the root: when no indexed root head exists, the write builds a genesis root leaf carrying the new entry rather than superseding a root that was never created.

#### Scenario: root resolves to the chain head, not genesis

- **GIVEN** a workspace-root chain that has been superseded at least once
- **WHEN** a client builds its tree from an indexer snapshot containing the whole root chain
- **THEN** `root_uri` is the flagged record with no successor, reached by forward-walking from the genesis candidate
- Verified in `DirectoryTree::set_root` and `from_records` (crates/opake-core/src/directories/tree.rs); without the forward walk `root_uri` pins to the immutable superseded genesis and stale entries leak into the snapshot

#### Scenario: first directory create in an empty workspace

- **GIVEN** a workspace with a keyring but no indexed root directory
- **WHEN** a manager creates the first directory
- **THEN** the operation writes a genesis root leaf (no `supersedes`) carrying the new entry, rather than failing to supersede a nonexistent root
- Regressions: `genesis_root_cascade_when_no_indexed_root`, `create_directory_genesis_root_when_no_indexed_root` (crates/opake-core/src/manager/manager_tests.rs; fix `1d032cc`)

### Requirement: Editor supersedes are additive; managers are unrestricted

Directory authority SHALL follow the member's role in the current keyring (`spec:workspace-membership § Membership state is the keyring head's member list`): a viewer authors nothing, a manager may add, drop, substitute, and reorder freely, and an editor may only ADD entries or ADVANCE an existing one. An editor's supersede SHALL be rejected unless every entry present in the prior canonical is either still present or covered by an advance — an entry the superseding record ADDs whose target record `supersedes` the dropped entry. A dropped entry with no such coverage is a disguised delete and SHALL be rejected.

The additivity check SHALL be evaluated over target URIs only; CIDs and ordering may differ across an additive supersede. Because an advance's coverage link lives on the *replacing target's* `supersedes` field — and for a document that record is not among the directory records — the check SHALL be fed both directory and document supersede links. The rule is enforced in two places that must agree: the indexer at write time (apps/indexer/lib/opake_indexer/authority.ex — `check_directory_supersede/4`, `additivity_check/2`, `additive?/3`) and the client as defense-in-depth over an indexer snapshot (crates/opake-core/src/directories/chain.rs::`verify_directory_additivity`).

The client's exemption predicate SHALL account for former managers: a manager's legitimate historical deletion stays in the chain after they are demoted, so a current-managers-only predicate would later trip on it. The client therefore runs a two-pass check — fast pass over current managers, then, only on a violation, a pass over the union of everyone who was ever a manager across the keyring chain (crates/opake-core/src/manager/tree.rs::`verify_directory_chain_additivity`). It uses an ever-was-a-manager union rather than point-in-time authority keyed on `createdAt`, because `createdAt` is inside the author-signed record and so author-controlled; the union never grants authority to someone who never held it.

#### Scenario: editor advances a document via supersede

- **GIVEN** an editor replacing another member's document with an edited successor whose record `supersedes` the original
- **WHEN** the editor writes a directory supersede dropping the old doc entry and adding the new one
- **THEN** the supersede is accepted — the dropped entry is covered by the added target's supersede link
- Regression `bug__additivity_allows_editor_doc_edit_via_document_supersede` (crates/opake-core/src/manager/tree_tests.rs); pure-decision test `passes_editor_advance_when_dropped_entry_is_superseded` (crates/opake-core/src/directories/chain_tests.rs)

#### Scenario: editor bare drop is rejected

- **GIVEN** an editor superseding a directory with an entry removed and no added target that supersedes it
- **WHEN** the supersede is validated
- **THEN** it is rejected as non-additive and does not enter the canonical chain
- Regression `bug__additivity_rejects_editor_drop_without_document_supersede` (crates/opake-core/src/manager/tree_tests.rs); indexer `additive?/3` returns `{:rejected, :additivity_violation}`

#### Scenario: former manager's deletion does not brick tree load

- **GIVEN** a chain containing a manager's legitimate deletion, where that manager was later demoted to editor
- **WHEN** a client loads the tree and the fast additivity pass flags the old deletion
- **THEN** the slow pass exempts anyone who was ever a manager across the keyring chain and the load succeeds
- Verified in `verify_directory_chain_additivity` two-pass logic (crates/opake-core/src/manager/tree.rs)

### Requirement: Consumers build the live tree from chain heads only

An indexer snapshot SHALL be treated as containing whole chains — every superseded predecessor alongside the head. Any consumer building live tree state (root detection, parent/child indexing, snapshot construction, reachability) SHALL consider chain-head records only and exclude any record another record supersedes. Whole-chain operations that intentionally need predecessors are the exception and SHALL be explicit about it.

The head-only set is `DirectoryTree::canonical_directory_uris()`; the unfiltered set is `all_directory_uris()` and is correct only for whole-chain work. `find_parent` SHALL skip superseded records so it returns the canonical parent rather than an arbitrary prior version whose ancestors walk up to a stale root.

Head adoption is subject to verifiability: when a chain walk cannot verify a proposed head because a link is corrupt (see record-validity), the consumer SHALL NOT adopt the proposed head and SHALL continue presenting the newest head it can fully verify. This knowingly-stale presentation is a deliberate degradation state, not an error: the consumer SHALL surface that a newer, unverifiable head exists, and SHALL re-attempt verification when the chain changes. An unverifiable head never silently becomes canonical.

#### Scenario: parent lookup returns the canonical parent

- **GIVEN** a child entry listed by both a superseded directory record and its current head
- **WHEN** a consumer resolves the child's parent
- **THEN** it returns the head, not the superseded predecessor
- Regression `bug__find_parent_skips_superseded_parent` (crates/opake-core/src/directories/tree.rs::`find_parent`, tree_tests.rs:431)

#### Scenario: a legitimate editor edit is not reported unreachable

- **GIVEN** a cross-author document edit that superseded a parent directory
- **WHEN** the client rebuilds its tree from a snapshot carrying both the old and new parent records
- **THEN** the edited entry resolves under the canonical parent rather than surfacing a stale-parent "not reachable from root" error
- Part of the canonical-vs-full-chain fix class, commit `6b3bf7f`; named API `DirectoryTree::canonical_directory_uris()` introduced there

#### Scenario: stale-but-verified over fresh-but-unverifiable

- **WHEN** a chain's proposed head requires walking through a corrupt link
- **THEN** the consumer presents the newest verifiable head, surfaces that a newer unverifiable head exists, and adopts the new head only once the walk verifies

#### Scenario: recovery on cleanup

- **WHEN** the corrupt link is superseded or removed and the chain walk verifies end-to-end
- **THEN** the consumer adopts the current head and the degradation state clears

### Requirement: Cascades write leaf-first so the indexer resolves additivity in arrival order

A curatorial edit that changes a record's identity SHALL cascade: write the deepest changed record first, then walk up, rewriting each ancestor's `targetCid` (and, for a substitution, its entry target) to point at the freshly written child, up to the workspace root. The whole cascade SHALL be bundled in one signed `applyWrites` on the curator's PDS, and the writes SHALL be ordered bottom-up (`execute_cascade` writes the leaf first; `build_deep_cascade_levels` and `substitute_entry_and_cascade` construct levels leaf-to-root).

This ordering is load-bearing for the indexer's authority check, not just efficiency. The firehose consumer processes a commit's operations in write order, and the indexer authorizes an editor's advance by resolving the added target's `supersedes` link against already-indexed records (`claimed_supersedes` in authority.ex). Emitting the replacing target before the directory supersede that references it guarantees the link resolves; an out-of-order arrival reads as a bare drop and is rejected, healing only on reprocess.

#### Scenario: substitution's replacement is indexed before the referencing directory

- **GIVEN** an editor substituting document A for its successor B in a directory
- **WHEN** the curator emits the `applyWrites` with B's record before the directory supersede
- **THEN** the indexer, consuming in write order, has B (and its `supersedes: A` link) available when it validates the directory supersede, so the advance is authorized
- Verified in crates/opake-core/src/manager/substitute.rs::`substitute_entry_and_cascade` (writes new target first, then cascades) and authority.ex moduledoc ("the leaf doc must be indexed before the directory supersede")

#### Scenario: partial cascade under failure leaves orphan heads, not corruption

- **GIVEN** a cascade that fails partway (network error between writes)
- **WHEN** some lower levels are already committed but no ancestor points at them yet
- **THEN** those levels are stranded chain heads at their paths, live state is unchanged above them, and the caller surfaces a retryable error rather than silently retrying
- Documented partial-failure contract in crates/opake-core/src/directories/cascade.rs::`execute_cascade`; orphan GC is future work (see open questions)

### Requirement: Concurrent supersedes fork, and the indexer picks a deterministic winner

When two curators supersede the same prior canonical concurrently, the chain forks: both records exist and both name the same `supersedes` target. The indexer SHALL detect the fork (a record whose `supersedes` target already has a successor), pick the winner deterministically by `createdAt` with a `(did, rkey)` tiebreak, keep the loser's record on its PDS but out of the canonical chain, and emit a `chain:forked` SSE event scoped to the affected workspace and chain, carrying the loser's URI, the fork point, and the winner's URI + CID (`SseChainForked`, crates/opake-core/src/indexer/sse/events.rs).

Fork detection operates on the `supersedes` back-edge and needs no plaintext path, which is why it holds even though directory paths are encrypted-name-derived. Fan-out is stateless (crates/opake-core/src/indexer/chain_fork_keeper.rs). Detection and surfacing end at the client's doorstep: what a losing client does with the event — refetch, replay, or surface to the user — is unspecified today (see open questions).

#### Scenario: two editors add entries against the same canonical

- **GIVEN** editors B and C each fetch the same canonical directory and write concurrent supersedes pointing at it
- **WHEN** the indexer processes both
- **THEN** one wins by `createdAt`/`(did, rkey)`, and the loser's client receives a `chain:forked` event naming the fork point and the winning head
- Contract in FEDERATION.md "Concurrent writes"; event shape in `SseChainForked`

## Open questions

- Per-path directory head lookup versus encrypted paths. FEDERATION.md describes the indexer keying `chain_heads` on `(workspace_id, kind, path)` and resolving parent queries by computing the parent path string. The chain module states the opposite — that the indexer deliberately does not expose per-path directory heads because "path is encrypted-name-derived in our model, so the indexer can't key on it without leaking," and that clients discover subtree heads by walking listings down from the root (crates/opake-core/src/directories/chain.rs::`ChainHeadProvider`). Only the root and keyring heads are fetched by identity today; `ChainHeadQueries.create` takes `(workspace_id, kind, head_uri, head_cid)` with no path column, so the code is consistent with the chain module and FEDERATION.md is the outlier. Which model is authoritative for nested paths — indexer path-keyed lookup or client top-down walk — needs one story, and FEDERATION.md needs correcting toward whichever wins. Fork detection itself is unaffected (it rides the `supersedes` edge), but upward traversal and nested-path head resolution need one story.
- Replay-on-fork. Detection and the `chain:forked` event are canon (requirement above); the losing client's response is not. No refetch-and-replay implementation exists (confirmed by test-coverage audit: nothing exercises a replay path at any tier), and the semantics need a design pass of their own — what "replaying" an editor's additive supersede means when the winning head also touched the same entries, and which curatorial entry points (add, delete, rename, move, substitute) participate. Until that lands, a fork surfaces to the client and stops there.
- Orphan and forked-out record cleanup. A partial cascade strands lower-level chain heads; a lost fork leaves a record on its author's PDS; a mid-chain self-delete leaves a gap and can orphan documents. All are described as best-effort future GC (cascade.rs partial-failure contract, FEDERATION.md "Self-delete" and "Future work"). No collection or GC mechanism is specified.
- Dangling target references after a member departs. Directory entries can point at a target on a PDS whose owner has left or deleted their account; the entry stays valid but unguaranteed. This is the cross-cutting custody/replication design pass tracked in the workspace-identity spec's open questions, not resolved here.

## Non-requirements

Out of scope for this spec, by design:

- Pre-publish moderation. No directory write requires approval before becoming visible; moderation is social (removal via keyring supersede) or a non-cryptographic client-side review layer. See FEDERATION.md "What this gives up."
- Atomic cross-PDS writes and multi-writer real-time collaboration. Concurrency is last-write-wins plus retry; pathological same-path concurrency and CRDT/OT merge are explicitly deferred (FEDERATION.md "Future work").
- Snapshot content-addressing and recovery. Cascade-with-CID makes a root CID address the whole tree, but snapshots are a recovery/audit primitive, not load-bearing for any current directory operation.
- Cabinet (personal, non-workspace) chain machinery. Cabinet trees use a `self`-rkey root and direct key wrapping with no supersede chain, additivity check, or fork handling; their structure and write semantics live in the tree-cabinet spec. Tree-shape invariants that hold in both contexts — cycle refusal at the domain API — live in the tree-topology spec, not here.
- Forking as a divergent-lineage operation (a workspace branching from another). The supersede primitive could carry a `kind: fork` extension, but no current use case needs it (FEDERATION.md "Future work").
