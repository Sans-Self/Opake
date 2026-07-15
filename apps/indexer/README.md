# Opake indexer

Elixir/Phoenix service that consumes the AT Protocol firehose, indexes
`at.opake.*` records, and serves the discovery API plus SSE event streams
that Opake clients use for live sync.

Full documentation — configuration, authentication, API endpoints,
deployment, firehose details — lives in [docs/indexer.md](../../docs/indexer.md).

## License

AGPL-3.0-or-later, like the rest of the repository. Mix manifests carry no
standard license field; the repository-level [LICENSE](../../LICENSE) governs.
