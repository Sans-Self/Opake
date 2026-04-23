<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the technical details, 
  architecture, or code style in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Contributing to Opake

Contributions welcome — from humans and AI agents alike.

## Getting started

1. Clone the repo
2. Install Rust 1.75+ via [rustup](https://rustup.rs)
3. Run the test suite: `cargo test`
4. Run the linter: `cargo clippy -- -D warnings`
5. For web frontend work: install [Bun](https://bun.sh), then `cd apps/web && bun install`

## Code style

- `cargo fmt` before every commit (enforced by CI and pre-commit hook)
- `cargo clippy -- -D warnings` must pass
- No `unwrap()` in library code — use `?` and proper error types
- `opake-core` uses `thiserror` for typed errors; `opake-cli` uses `anyhow` for application errors
- Prefer `&str` parameters, `String` for owned data
- Avoid `.clone()` unless necessary

## Architecture

```
crates/
  opake-core      platform-agnostic library (compiles to WASM)
                  - encryption/decryption (AES-256-GCM, x25519 key wrapping)
                  - XRPC client with automatic token refresh
                  - document operations (upload, download, list, delete, resolve)
                  - device pairing (ephemeral DH key exchange, identity transfer)
                  - AT Protocol record types and lexicon constants
                  - Storage trait + config/identity/session types (storage.rs)
                  - shared config path resolution (paths.rs)

  opake-derive    proc-macro crate
                  - #[derive(RedactedDebug)] with #[redact] field attribute
                  - generates Debug impls showing byte length instead of content
                  - used by opake-core (ContentKey, Session) and opake-cli (Identity)

  opake-wasm      WASM bridge (wasm-pack, wasm_bindgen)
                  - stateless crypto + tree exports
                  - OpakeContext + WasmFileManagerHandle for stateful JS interop

apps/
  cli/            CLI binary wrapping opake-core (package: opake-cli)
                  - clap command definitions
                  - FileStorage (impl Storage over filesystem, TOML + JSON)
                  - user interaction (prompts, formatting)

  indexer/        Elixir/Phoenix indexer + REST API for grant/keyring discovery
                  - Jetstream firehose consumer (WebSockex)
                  - PostgreSQL storage (Ecto)
                  - Phoenix API with DID-scoped Ed25519 auth (Erlang :crypto)
                  - rate limiting via Hammer

  web/            React SPA (Vite + TanStack Router/Start + Tailwind/daisyUI)
                  - opake-core via @opake/sdk (WASM under the hood, main-thread)
                  - IndexedDbStorage (Dexie.js/IndexedDB, bound into WASM via JsStorage)
                  - Zustand for small app-level state; @opake/react hooks for SDK data
                  - cabinet file browser UI with panel navigation, live via SSE

packages/
  @opake/sdk      TypeScript SDK wrapping WASM bindings
                  - Opake client, FileManager, auth, storage interfaces

  @opake/daemon   Background task scheduler
                  - session refresh, pair cleanup, grant healing, share retry

  @opake/react    React bindings
                  - OpakeProvider, hooks, query key management
```

`opake-core` must never depend on filesystem, stdin, or any platform-specific API. All I/O goes through the `Storage` trait — `FileStorage` (CLI) and `IndexedDbStorage` (web) are the platform-specific implementations.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the detailed crate structure and encryption model, and [docs/FLOWS.md](docs/FLOWS.md) for sequence diagrams of every operation.

## Testing

- Small test suites live inline in `#[cfg(test)]` modules. Larger test suites are extracted to sibling `*_tests.rs` files using `#[cfg(test)] #[path = "..._tests.rs"] mod tests;` — keeps implementation files focused
- Test contracts, not implementations — assert on inputs and outputs
- Name regression tests after the bug: `bug__float_not_rounding_correctly`
- Use `MockTransport` (from `opake-core/src/test_utils.rs`) for XRPC tests — enqueue responses, assert on captured requests
- Shared test fixtures live in `documents/mod.rs` (`mock_client`, `dummy_document`, etc.)
- The `test-utils` feature flag gates test infrastructure in `opake-core`

```sh
cargo test                          # all Rust tests
cargo test -p opake-core            # core only
cargo test -p opake-cli             # CLI only
cargo test -- --test-output         # show println output

cd apps/indexer && mix test          # indexer tests (Elixir/ExUnit)
cd apps/web && bun run test          # web frontend tests (Vitest + fake-indexeddb)
```

## Commit messages

- Imperative mood ("Add feature", not "Added feature")
- First line under 72 characters
- Body explains the *why*, not the *what*

## AI agents

AI-assisted contributions are welcome. No special rules beyond:

- Code must be indistinguishable from hand-written. No `// Generated by` comments, no boilerplate disclaimers.
- No `Co-Authored-By` trailers unless the human contributor requests it.
- Same quality bar as any other contribution — tests, clippy, formatting.

## Pull requests

- Keep PRs focused. One logical change per PR.
- All CI checks must pass (fmt, clippy, test).
- Describe what changed and why in the PR body.

## Security

Found a vulnerability? **Do not open a public issue.** See [SECURITY.md](SECURITY.md) for reporting instructions.

## License

By submitting a pull request, you agree that your contribution is licensed under [AGPL-3.0](LICENSE), the same license as the rest of the project. See [docs/LICENSING.md](docs/LICENSING.md) for details on what AGPL means for different use cases.

## Project management

This project uses [crosslink](https://github.com/forecast-bio/crosslink) for
issue tracking and AI agent workflow. After cloning, run `crosslink init` to
set up hooks and the local issue database. The rule files in
`.crosslink/rules/` have been trimmed from crosslink's defaults — Rust-specific
rules are deferred to `cargo clippy` and the project's `CLAUDE.md`.
