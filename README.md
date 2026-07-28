<!--
  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the technical details,
  command list, or installation steps in this README, you MUST also update
  the corresponding MDX content in `apps/web/src/content/` to prevent
  documentation drift.
-->

# Opake

**/oʊˈpɑːk/** — like "opaque," but built for the AT Protocol.

An encrypted personal cloud where privacy and collaboration are no longer a tradeoff. Opake uses your PDS as a blind storage layer. Files are encrypted client-side (AES-256-GCM) before they ever touch the network.

Your data is opaque to everyone without the key. That's the point.

[The Handbook](https://opake.at/docs) · [Issue Tracker](https://github.com/Opake-at/Opake/issues) · [Architecture](docs/ARCHITECTURE.md)

## Quick Start

### 1. Install

Requires Rust 1.75+.

```sh
cargo install --path apps/cli
```

### 2. Login

Authenticates via OAuth (DPoP), generates a 24-word seed phrase, and publishes your public encryption key.

```sh
opake login you.bsky.social
```

Write down the seed phrase when prompted — it's your recovery key for all devices.

### 3. Use

```sh
opake upload secret.pdf --tags confidential
opake share secret.pdf bob.bsky.social
opake ls --long
```

## How It Works

1. **Encrypt:** Plaintext → AES-256-GCM (random key K).
2. **Wrap:** Key K → X25519-HKDF-A256KW (wrapped to your DID).
3. **Publish:** Ciphertext blob + Metadata record → PDS.

No modifications to the PDS. All crypto happens on your machine.

## Repository Structure

- `crates/opake-core/` — Platform-agnostic library (Rust/WASM).
- `crates/opake-wasm/` — WASM bindings compiled by `wasm-pack`.
- `apps/cli/` — CLI implementation (`opake` binary).
- `apps/indexer/` — Elixir/Phoenix indexer for grant discovery.
- `apps/web/` — React SPA (Vite + TanStack Start).
- `packages/opake-sdk/` — TypeScript SDK wrapping the WASM bindings.
- `packages/opake-react/` — React hooks over the SDK.
- `packages/opake-daemon/` — Scheduled maintenance tasks.
- `lexicons/` — AT Protocol schemas (`at.opake.*`).

## Development

```sh
just build          # cargo build --workspace
just rust-test      # cargo test --workspace
just wasm           # wasm-pack build → packages/opake-sdk/wasm
just sdk-build      # build @opake/sdk (implies wasm)
just web-build      # build apps/web (implies sdk-build)
just indexer-test   # mix test --cd apps/indexer
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the "mini-nuke" policy and commit conventions.

## License

[AGPL-3.0](LICENSE) — see [docs/LICENSING.md](docs/LICENSING.md) for what this means for self-hosters, plugin developers, and contributors.
