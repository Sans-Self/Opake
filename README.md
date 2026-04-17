<!--
  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the technical details,
  command list, or installation steps in this README, you MUST also update
  the corresponding MDX content in `web/src/content/` to prevent
  documentation drift.
-->

# Opake

**/oʊˈpɑːk/** — like "opaque," but built for the AT Protocol.

An encrypted personal cloud where privacy and collaboration are no longer a tradeoff. Opake uses your PDS as a blind storage layer. Files are encrypted client-side (AES-256-GCM) before they ever touch the network.

Your data is opaque to everyone without the key. That's the point.

[The Handbook](https://opake.app/docs) · [Issue Tracker](https://tangled.org/sans-self.org/opake.app/issues) · [Architecture](docs/ARCHITECTURE.md)

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

- `opake-core/` — Platform-agnostic library (Rust/WASM).
- `opake-cli/` — CLI implementation.
- `indexer/` — Elixir/Phoenix indexer for grant discovery.
- `web/` — React SPA (Vite + TanStack).
- `lexicons/` — AT Protocol schemas (`app.opake.*`).

## Development

```sh
cargo test           # Rust tests
bun run wasm:build   # Build WASM for web
mix setup            # Setup Indexer
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the "mini-nuke" policy and commit conventions.

## License

[AGPL-3.0](LICENSE) — see [docs/LICENSING.md](docs/LICENSING.md) for what this means for self-hosters, plugin developers, and contributors.
