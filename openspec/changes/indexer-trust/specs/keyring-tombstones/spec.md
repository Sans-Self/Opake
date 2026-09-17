# Spec Delta

## MODIFIED Requirements

### Requirement: Rollback restores the newest live record and re-broadcasts it

When a head delete resolves to `rolled_back`, the indexer SHALL select the newest live keyring record for the workspace (latest `indexed_at` among the rows it holds) as the restored head. The indexer SHALL NOT select the deleted record's direct `supersedes` predecessor, whose row is not held when that record was itself deleted (`spec:indexer-trust § The indexer keeps no deleted records`). `torn_down` is therefore equivalent to "no live record remains."

After broadcasting the delete, the indexer SHALL re-broadcast the restored record as a normal `at.opake.keyring:upsert` on the same topics. A rollback changes the current member set, optional wraps, key-bound approvals, rotation, and metadata back to the restored record's contents; clients rebuild their projection through the ordinary upsert path rather than patching fields from the delete event.

#### Scenario: head delete rolls back to the predecessor

- **GIVEN** a keyring chain genesis → A → B with head B, all records live
- **WHEN** B is deleted
- **THEN** the chain head becomes A, the delete broadcast carries `outcome: rolled_back`, and A is re-broadcast as a keyring upsert

#### Scenario: head delete after an earlier intermediate delete still rolls back

- **GIVEN** a keyring chain genesis → A → B with head B, where A was deleted earlier
- **WHEN** B is deleted
- **THEN** the outcome is `rolled_back` to genesis (the newest live record), not `torn_down`

#### Scenario: rollback that undoes a membership change reinstates the member

- **GIVEN** a head record that removed member M from the previous head
- **WHEN** that head is deleted and the previous head is restored and re-broadcast
- **THEN** M's client evaluates their restored DID and role and rebuilds a workspace entry if identity verification succeeds using current or historical keys; membership restoration does not imply a current wrap exists

#### Scenario: rollback reinstates historical-only membership

- **GIVEN** the restored head admits M but supplies only usable historical wraps, including rotation 0
- **WHEN** its upsert arrives
- **THEN** M's workspace reappears after identity verification with its restored role and historical access, but no current-key readability is invented

#### Scenario: rollback does not borrow approval from the deleted head

- **GIVEN** the deleted head carried approval for bundle B but the restored head carries approval for bundle A
- **WHEN** a runner evaluates a new wrap to unverified bundle B
- **THEN** the deleted head's approval does not carry, and the item requires a fresh decision under the restored relationship state
