# tree-chains (delta)

## MODIFIED Requirements

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
