<!--
  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the technical details,
  command list, or installation steps in this README, you MUST also update
  the corresponding MDX content in `apps/web/src/content/` to prevent
  documentation drift.
-->

# Opake

**/oʊˈpɑːk/** — like "opaque," but built for the AT Protocol.

An encrypted personal cloud where privacy and collaboration stop being a tradeoff. Opake stores your files on a [Personal Data Server](https://atproto.com/guides/self-hosting) — the same storage-and-identity layer that backs the AT Protocol — and treats it as a blind medium. Everything is encrypted client-side (AES-256-GCM content, hybrid X25519 + ML-KEM-768 key wrapping) before it reaches the network. The PDS holds ciphertext and signed metadata records; it never holds a key.

The protocol is the product. A PDS gives Opake DID-based identity, schema-validated records in a Merkle Search Tree, blob storage, and federation for free. Opake adds a set of `at.opake.*` lexicons for files, encryption envelopes, and sharing, plus the client-side cryptography that keeps the server honest. The web app and CLI are two conveniences over that protocol; a third client that spoke the same lexicons and ran the same key hierarchy would interoperate without asking anyone's permission.

Your data is opaque to everyone without the key. That is the point.

[The Handbook](https://opake.app/docs) · [Architecture](docs/ARCHITECTURE.md) · [Security Policy](SECURITY.md)

## Quick Start

### 1. Build

The toolchain — Rust, `wasm-pack`, the Elixir stack for the indexer — is pinned in the Nix flake. Enter the dev shell, then build the CLI:

```sh
nix develop
just build            # cargo build --workspace
```

To install the `opake` binary onto your `PATH`:

```sh
cargo install --path apps/cli
```

### 2. Log in

Authenticates over OAuth (DPoP-bound tokens), derives your identity from a 24-word seed phrase, and publishes your X25519 + ML-KEM-768 encryption public keys as a PDS record.

```sh
opake account login you.bsky.social
```

Write the seed phrase down when prompted — it derives every device's keys and is the only recovery path.

### 3. Use

```sh
opake upload secret.pdf --description "quarterly numbers"
opake share new secret.pdf bob.bsky.social
opake ls --long
```

## How It Works

1. **Encrypt.** Plaintext and its metadata → AES-256-GCM under a fresh per-document content key.
2. **Wrap.** The content key → hybrid `x25519-mlkem768-hkdf-a256kw-v2` (X25519 ECDH and ML-KEM-768 encapsulation combined through HKDF, then AES-KW), sealed to each authorized recipient's public keys.
3. **Publish.** The ciphertext blob and a metadata record whose real fields are all inside the encrypted envelope → your PDS.

No modifications to the PDS. Every cryptographic operation happens on your machine.

## Security Status

Opake v1 is a **public beta**. The cryptographic design is documented (see [docs/CRYPTO.md](docs/CRYPTO.md)) and specified in machine-checked capability specs under `openspec/specs/`, and it has been adversarially reviewed in-house. It has **not** yet been independently audited; an external audit is scheduled work, not a completed milestone. Treat the guarantees below as the current honest boundary, not a finished one.

| Known limitation | What it means |
|---|---|
| No identity-key rotation | A compromised identity keypair cannot yet be retired and replaced; recovery is re-derivation from the seed phrase, not rotation. |
| Content-pin detects honest disagreement, not tampering | Superseding records carry a `supersedesCid` pin. The indexer compares the CID a host *reports*, so the pin catches an honest host that disagrees about which record is current — it does not byte-bind against a hostile host that serves altered content under a matching CID claim. Byte-level binding is deferred. |
| Indexer auth key provenance | An indexer authenticates callers against the Ed25519 signing key published in their `at.opake.publicKey` record; that key is not yet cryptographically bound to the caller's atproto identity. |
| No revocation of historical access | Removing a member rotates the group key forward but cannot un-share what they already decrypted. This is the git-crypt posture, accepted by design: true revocation requires re-encrypting content under a new key. |

Report vulnerabilities per [SECURITY.md](SECURITY.md) — privately, never as a public issue.

## Repository Structure

- `crates/opake-core/` — Platform-agnostic protocol library (Rust, compiles to WASM): records, XRPC client, domain API, chain walking, keepers.
- `crates/opake-crypto/` — Cryptographic primitives: content encryption, hybrid key wrapping, seed-phrase derivation. No I/O.
- `crates/opake-derive/` — Proc macros: `RedactedDebug` (zeroizing `Debug`) and `signoff` (session persistence).
- `crates/opake-wasm/` — `wasm-pack` bindings; the security boundary where tokens and keys live.
- `apps/cli/` — The `opake` binary (package `opake-cli`).
- `apps/indexer/` — Elixir/Phoenix indexer: firehose consumer, discovery API, SSE stream.
- `apps/web/` — React SPA (Vite + TanStack Router).
- `packages/opake-sdk/` — TypeScript SDK over the WASM bindings.
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
just indexer-test   # mix test in apps/indexer
just spec-lint      # check spec citation integrity
just validate       # fmt, clippy, tests, web, indexer, spec-lint
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for code style, testing conventions, and the spec workflow.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — System overview, encryption model, data model, identity derivation
- [docs/CRATE_STRUCTURE.md](docs/CRATE_STRUCTURE.md) — Annotated file tree for every crate and app
- [docs/CRYPTO.md](docs/CRYPTO.md) — Algorithms, constants, key hierarchy, operation reference
- [docs/AUTH.md](docs/AUTH.md) — OAuth/DPoP authentication, multi-account, device pairing
- [docs/STORAGE.md](docs/STORAGE.md) — Storage abstraction, local record cache, permissions
- [docs/FEDERATION.md](docs/FEDERATION.md) — Distributed authority, supersede chains, lineage
- [docs/FLOWS.md](docs/FLOWS.md) — Sequence diagrams for every operation
- [docs/indexer.md](docs/indexer.md) — Indexer config, auth, endpoints, firehose
- [docs/BACKGROUND_WORK.md](docs/BACKGROUND_WORK.md) — Daemon task registry, scheduling
- [docs/LICENSING.md](docs/LICENSING.md) — AGPL-3.0 implications for self-hosters, plugin developers, contributors
- [lexicons/README.md](lexicons/README.md) — Full lexicon schema reference
- [lexicons/EXAMPLES.md](lexicons/EXAMPLES.md) — Annotated example records
- [SECURITY.md](SECURITY.md) — Vulnerability reporting, scope, trust model, response timeline

## License

[AGPL-3.0](LICENSE) — see [docs/LICENSING.md](docs/LICENSING.md) for what this means for self-hosters, plugin developers, and contributors.
