# Spec Delta

## ADDED Requirements

### Requirement: audit

A spec MUST use `audit` to mean:

The on-demand walk of a workspace's keyring chain from head to genesis that fetches each link from its authoring PDS and reports the authority trail, content pins, missing links and fork points. It is a report a member runs; no client operation consults it.

- **Admitted:** workspace audit

#### Scenario: In a sentence

- **WHEN** a member wants a second opinion on the head the indexer serves
- **THEN** they run the audit, and its non-zero exit tells them a link is missing or failed a check

### Requirement: manifest

A spec MUST use `manifest` to mean:

The list of URI and CID pairs for every live record in a sync response's scope. A client drops what it holds that the manifest does not name.

#### Scenario: In a sentence

- **WHEN** a client syncs after a document was deleted
- **THEN** the manifest does not name the document, and the client drops it

## MODIFIED Requirements

### Requirement: tombstone

A spec MUST use `tombstone` to mean:

The delete the firehose delivers for a record. The indexer resolves its outcome from the rows it holds, broadcasts it, and keeps no row for the deleted record.

> Note: a tombstone is an event, not stored state. Nothing in the indexer survives a delete except the outcome it broadcast.

#### Scenario: In a sentence

- **WHEN** a keyring delete tombstone arrives for a workspace whose chain still has a live record
- **THEN** the workspace is not dropped, because the tombstone is record cleanup and not destruction
