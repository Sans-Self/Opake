<!--
  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the technical details,
  command list, or installation steps in this README, you MUST also update
  the corresponding MDX content in `apps/web/src/content/` to prevent
  documentation drift.
-->

<p align="center">
  <a href="https://opake.at">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset=".github/assets/banner-dark.png" />
      <img src=".github/assets/banner-light.png" alt="Opake — your data, freely shared, privately kept" width="780" />
    </picture>
  </a>
</p>

<p align="center">
  <a href="https://github.com/Opake-at/Opake/actions/workflows/qa.yml"><img src="https://github.com/Opake-at/Opake/actions/workflows/qa.yml/badge.svg" alt="QA" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-AGPL--3.0-9a7840" alt="Licence: AGPL-3.0" /></a>
  <a href="https://atproto.com"><img src="https://img.shields.io/badge/built%20on-AT%20Protocol-9a7840" alt="Built on the AT Protocol" /></a>
</p>

<p align="center">
  <a href="https://opake.at">opake.at</a> ·
  <a href="https://opake.at/docs">The Handbook</a> ·
  <a href="https://github.com/Opake-at/Opake/issues">Issue Tracker</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="https://opake.at/docs/ai">AI and Opake</a>
</p>

---

**/oʊˈpɑːk/** — like "opaque," but built for the AT Protocol.

An encrypted personal cloud where privacy and collaboration are no longer a tradeoff. Opake uses your PDS as a blind storage layer. Files are encrypted client-side (AES-256-GCM) before they ever touch the network.

Your data is opaque to everyone without the key. That's the point.

> **On AI.** The web UI and most markdown files were produced with AI assistance;
> the code is AI-assisted under human-dictated architecture, specs, and review, and
> every published line is the maintainer's responsibility. The same standards apply to
> external contributions. The full note, including where the maintainer's own position
> on AI has changed and where it hasn't, is
> [AI and Opake](https://opake.at/docs/ai).

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

Contributing requires signing the [Contributor License Agreement](CLA.md); the process runs automatically on your first pull request. If the AGPL rules out a use you want to pursue, [commercial licensing](docs/LICENSING.md#commercial-licensing) is a conversation worth having.
