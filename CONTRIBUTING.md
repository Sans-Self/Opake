<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the technical details, 
  architecture, or code style in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Contributing to Opake

Contributions welcome — from humans and AI agents alike.

## Getting started

The toolchain — Rust, `wasm-pack`, cargo helpers, and the Elixir stack for the indexer — is pinned in the Nix flake, so a working checkout needs nothing installed beyond Nix and [direnv](https://direnv.net). The `just` recipes route through `direnv exec`, so they work from a bare shell once the environment is allowed.

1. Clone the repo
2. `direnv allow` (or `nix develop` to enter the dev shell manually)
3. Run the tests: `just rust-test`
4. Run the linter: `just clippy`
5. For web work, install the JS dependencies: `just setup` (runs `bun install` and installs the pre-commit hook)

## Code style

- `just fmt` before every commit (enforced by CI and the pre-commit hook)
- `just clippy` — `cargo clippy -- -D warnings` — must pass
- No `unwrap()` in library code — use `?` and proper error types
- `opake-core` and `opake-crypto` use `thiserror` for typed errors; `opake-cli` uses `anyhow` for application errors
- Prefer `&str` parameters, `String` for owned data
- Avoid `.clone()` unless necessary

## Architecture

```
crates/
  opake-core      platform-agnostic protocol library (compiles to WASM)
                  - XRPC client with automatic token refresh
                  - domain API (Opake → FileManager / WorkspaceAdmin)
                  - document, directory, keyring, grant, pairing operations
                  - supersede-chain walking and authority verification
                  - AT Protocol record types and lexicon constants
                  - Storage trait + config/identity/session types (storage.rs)
                  - shared config path resolution (paths.rs)

  opake-crypto    cryptographic primitives (no I/O, platform-agnostic)
                  - AES-256-GCM content + metadata encryption
                  - hybrid X25519 + ML-KEM-768 key wrapping (splice-resistant HKDF transcript)
                  - two-layer keyring key wrapping (AES-KW under a group key)
                  - BIP-39 mnemonic parsing, generation, and key derivation

  opake-derive    proc-macro crate
                  - #[derive(RedactedDebug)] with #[redact] field attribute
                  - generates zeroizing Debug impls (byte length, never content)
                  - #[signoff] attribute macro for automatic session persistence

  opake-wasm      WASM bridge (wasm-pack, wasm_bindgen)
                  - stateless crypto + tree exports
                  - OpakeContext + WasmFileManagerHandle for stateful JS interop
                  - the security boundary: tokens, DPoP keys, and crypto stay here

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

See [docs/CRATE_STRUCTURE.md](docs/CRATE_STRUCTURE.md) for the annotated file tree, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the encryption model, and [docs/FLOWS.md](docs/FLOWS.md) for sequence diagrams of every operation.

## Testing

- Small test suites live inline in `#[cfg(test)]` modules. Larger test suites are extracted to sibling `*_tests.rs` files using `#[cfg(test)] #[path = "..._tests.rs"] mod tests;` — keeps implementation files focused
- Test contracts, not implementations — assert on inputs and outputs
- Name regression tests after the bug: `bug__float_not_rounding_correctly`
- Use `MockTransport` (from `opake-core/src/test_utils.rs`) for XRPC tests — enqueue responses, assert on captured requests
- Shared test fixtures live in `documents/mod.rs` (`mock_client`, `dummy_document`, etc.) — reach for the factory, don't hand-craft objects per test file
- The `test-utils` feature flag gates test infrastructure in `opake-core`

```sh
just rust-test                       # all Rust tests (cargo test --workspace)
cargo test -p opake-core             # core only
cargo test -p opake-cli              # CLI only

just indexer-test                    # indexer tests (Elixir/ExUnit)
just web-test                        # web frontend tests (Vitest + fake-indexeddb)
just test                            # rust + sdk + web + indexer
```

## Specs and change proposals

Opake is spec-driven. Capability specs — the behavioral contracts the code and its tests are held to — live as canon under `openspec/specs/*/spec.md`. Nothing there is hand-authored in place: canon is only ever updated by syncing a reviewed change.

A change to protocol behavior starts as an OpenSpec *change* under `openspec/changes/`, scaffolded by the CLI (`bunx @fission-ai/openspec` — the bare `openspec` binary is not on `PATH`). A change carries its own proposal, delta specs, design notes, and task list. You implement against the delta, then sync the delta into canon once it is approved. Editing a canon spec directly, or writing a delta spec outside a change, bypasses the status tracking the CLI is the source of truth for.

Protocol contracts are cited inline where they are exercised, in the form `spec:capability § Requirement Name`. `just spec-lint` checks that every citation resolves to a real requirement; it does not check semantics, so cite the requirement that actually governs the behavior, not a neighbor. Changes that touch federation or authority get their spec delta reviewed before implementation begins.

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

Work is tracked in the project's issue tracker; check the repository host for the
current list before filing, and search existing issues before opening a new one.
Coding standards live in `CLAUDE.md`; Rust style is enforced by `just fmt` and
`just clippy`.
