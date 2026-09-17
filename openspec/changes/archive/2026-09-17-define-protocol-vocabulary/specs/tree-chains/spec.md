# tree-chains (delta)

## MODIFIED Requirements

### Requirement: Consumers build the live tree from chain heads only

An indexer snapshot SHALL be treated as containing whole chains — every superseded predecessor alongside the head. Any consumer building live tree state (root detection, parent/child indexing, snapshot construction, reachability) SHALL consider chain-head records only and exclude any record another record supersedes. Whole-chain operations that intentionally need predecessors are the exception and SHALL be explicit about it.

The head-only set is `DirectoryTree::canonical_directory_uris()`; the unfiltered set is `all_directory_uris()` and is correct only for whole-chain work. `find_parent` SHALL skip superseded records so it returns the canonical parent rather than an arbitrary superseded record whose ancestors walk up to a stale root.

Head adoption is subject to verifiability: when a chain walk cannot verify a proposed head — because a link is corrupt (see record-validity), or because a link's reported CID disagrees with the successor's `supersedesCid` content pin (`spec:lineage § Supersede references carry a content pin`) — the consumer SHALL NOT adopt the proposed head and SHALL continue presenting the newest head it can fully verify. This knowingly-stale presentation is a deliberate degradation state, not an error: the consumer SHALL surface that a newer, unverifiable head exists, and SHALL re-attempt verification when the chain changes. An unverifiable head never silently becomes canonical.

Directory supersede records, like every superseding record kind, carry the content pin alongside `supersedes`; cascades stamp it per level.

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

#### Scenario: pin-mismatched head degrades, not errors

- **GIVEN** a proposed directory head whose predecessor's reported CID disagrees with the head's content pin
- **WHEN** a consumer builds the live tree
- **THEN** the proposed head is not adopted, the newest fully-verifiable head remains presented, the unverifiable newer head is surfaced, and verification is re-attempted when the chain changes

### Requirement: A directory is an organizational record with no crypto envelope over its listing

A directory record (`at.opake.directory`) SHALL be a distinct record type whose listing carries only structure, not content. Each entry SHALL pin a target AT-URI and the target record's CID at write time (`#listingEntry`: `target` + `targetCid`); it SHALL NOT carry the target's name, type, or any other metadata. The directory's own name lives in its `encryptedMetadata`, decrypted under the directory's content key; entry names live in each target record's `encryptedMetadata` and are resolved by fetching the target. Entry kind (document vs directory) SHALL be derived from the target URI's collection segment, not stored.

The `targetCid` pin exists so a consumer can content-address a subtree and so the indexer can detect a conflicting concurrent supersede on a child path without refetching every target.

#### Scenario: listing exposes no plaintext names to the PDS

- **GIVEN** a directory record stored on a member's PDS
- **WHEN** the PDS or any unauthorized reader inspects the record
- **THEN** it sees a list of `(target, targetCid)` pairs and encrypted metadata, and no entry name or type
- Verified in `lexicons/at.opake.directory.json` (`entries` → `#listingEntry`) and `DirectoryTree::from_records`, which projects entries to bare target URIs
