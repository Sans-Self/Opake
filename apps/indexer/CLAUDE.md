# Indexer (Elixir)

See **[docs/indexer.md](../../docs/indexer.md)** for tables, endpoints, deployment, and firehose details.

## Conventions for agents

- Event parser returns tagged tuples or `:ignore`. Indexer dispatches via `dispatch/3` function clauses grouped by domain.
- All public functions have `@spec`. Schemas use `.t()` types.
- Query list functions return `{[results], cursor | nil}`. Cursor format: `"{iso8601}::{uri}"`.
- Workspace-scoped endpoints must check `KeyringQueries.is_member?/2` — returns 403 for non-members.
- `Pagination.build_next_cursor/1` expects items with `:uri` and `:indexed_at` fields. If your schema uses a different PK name, map it.

## Adding a new collection

1. `@collection` constant + parser in `event.ex`, `dispatch/3` clause in `indexer.ex`
2. Migration, schema, query module (follow existing patterns)
3. Collection string in `@wanted_collections` in `consumer.ex`
4. Controller + route if API endpoint needed
5. Tests: event parser, query, pipeline e2e, controller

## Test setup

- DataCase for queries (`async: true`), ConnCase for controllers (`async: false`)
- Controller tests need `set_mox_global`, `verify_on_exit!`, `:ets.delete_all_objects(:key_cache)` in setup
- Pipeline e2e tests feed JSON through `Indexer.process_message/2`
