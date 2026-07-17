# tree-chains — delta for verify-foreign-lineage

## MODIFIED Requirements

### Requirement: Consumers build the live tree from chain heads only

An indexer snapshot SHALL be treated as containing whole chains — every superseded predecessor alongside the head. Any consumer building live tree state (root detection, parent/child indexing, snapshot construction, reachability) SHALL consider chain-head records only and exclude any record another record supersedes. Whole-chain operations that intentionally need predecessors are the exception and SHALL be explicit about it.

The head-only set is `DirectoryTree::canonical_directory_uris()`; the unfiltered set is `all_directory_uris()` and is correct only for whole-chain work. `find_parent` SHALL skip superseded records so it returns the canonical parent rather than an arbitrary prior version whose ancestors walk up to a stale root.

Head adoption is subject to verifiability: when a chain walk cannot verify a proposed head — because a link is corrupt (see record-validity), or because a link's reported CID disagrees with the successor's `supersedesCid` content pin (`spec:lineage § Supersede references carry a content pin`) — the consumer SHALL NOT adopt the proposed head and SHALL continue presenting the newest head it can fully verify. This knowingly-stale presentation is a deliberate degradation state, not an error: the consumer SHALL surface that a newer, unverifiable head exists, and SHALL re-attempt verification when the chain changes. An unverifiable head never silently becomes canonical.

Directory supersede records, like every superseding record kind, carry the content pin alongside `supersedes`; cascades stamp it per level.

#### Scenario: parent lookup returns the canonical parent

- **GIVEN** a child entry listed by both a superseded directory record and its current head
- **WHEN** a consumer resolves the child's parent
- **THEN** it returns the head, not the superseded predecessor

#### Scenario: pin-mismatched head degrades, not errors

- **GIVEN** a proposed directory head whose predecessor's reported CID disagrees with the head's content pin
- **WHEN** a consumer builds the live tree
- **THEN** the proposed head is not adopted, the newest fully-verifiable head remains presented, the unverifiable newer head is surfaced, and verification is re-attempted when the chain changes
