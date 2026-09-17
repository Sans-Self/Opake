# indexer-trust

State what a client trusts the indexer for, read records from it, retire the chain walk to an audit, and make the indexer rebuildable from the PDSes.

## Sync and archive notes

- `keyring-tombstones § Rollback restores the newest live record and re-broadcasts it` renames one scenario: "head delete with a purged intermediate still rolls back" becomes "head delete after an earlier intermediate delete still rolls back". `openspec validate --strict` reports this as its only error, and archive applies the same check. Delete the old scenario from canon by hand immediately before archiving, as its own commit. When syncing, replace the requirement block wholesale; do not merge the old scenario back in.
- The indexer test `head delete with a purged intermediate rolls back to the newest live record` (apps/indexer/test/opake_indexer/firehose_keyring_delete_test.exs) is renamed with task 1.5.
- `membership-mutation-outcomes` modifies `tree-chains § Concurrent supersedes fork, and the indexer picks a deterministic winner`, which this change cites. Re-base that change on canon after this one syncs.
