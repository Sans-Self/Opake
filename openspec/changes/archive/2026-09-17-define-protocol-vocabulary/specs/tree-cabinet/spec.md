# tree-cabinet (delta)

## MODIFIED Requirements

### Requirement: A missing root is created on demand

An operation that needs the cabinet root when no root record exists SHALL create it rather than fail or silently do nothing (`FileManager::ensure_root` → `get_or_create_root`, crates/opake-core/src/manager/directory.rs). A missing root is a normal state, not an error: a fresh account has never written one, and a recursive root delete removes it deliberately. Clients SHALL NOT treat a missing root as "nothing to do" — a client that short-circuits a write on a missing root strands the user in a cabinet that can never receive its first document.

#### Scenario: first write on a fresh cabinet creates the root

- **GIVEN** an account that has never written a directory record
- **WHEN** the owner uploads a document or creates a folder
- **THEN** the root record is created as part of the operation and the write lands under it
- Verified in crates/opake-core/src/manager/upload.rs (directory `None` → `ensure_root`)

#### Scenario: write after a recursive root delete recreates the root

- **GIVEN** a cabinet whose root was recursively deleted (record gone)
- **WHEN** the owner performs the next tree write
- **THEN** a fresh root record is created on demand and the write succeeds
- Verified via `get_or_create_root` (crates/opake-core/src/directories/get_or_create_root.rs)
