## ADDED Requirements

### Requirement: Historical-key storage has explicit per-record bounds and a declared format

The live keyring head and every separately stored historical-key record SHALL have
declared structural and encoded-size bounds enforced by writers, clients, and indexer
ingestion. Validation SHALL distinguish an individual oversized or malformed record
from the number of valid historical generations. Exceeding one page's capacity SHALL
require partitioning, not silently truncating referenced keys or declaring the workspace
too old to rotate.

Replacing embedded historical member arrays SHALL be declared as a structural protocol
change under `spec:record-validity § schema evolution is additive and vocabulary is version-pinned`.
For the current pre-v1 work, the chosen wire layout SHALL ship only with coordinated
client, indexer, lexicon, and fixture regeneration under the declared version-1 reset
policy. Old readers SHALL NOT be assumed to understand a replacement history layout by
ignoring new fields. This specification does not authorize a development-state reset
during proposal work.

#### Scenario: an individual record exceeds its declared bound

- **WHEN** a writer, client, or indexer encounters a historical-key record beyond its declared size or structural limit
- **THEN** it rejects that record with a specific validity error rather than accepting truncated history or discarding unrelated valid generations

#### Scenario: history partitioning is not an ignore-safe field addition

- **WHEN** the embedded-history representation is replaced by separately stored material
- **THEN** the deployment declares the format break and coordinates all readers and fixtures, rather than allowing old clients to interpret absent embedded keys as complete history
