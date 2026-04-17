# Security Policy

Opake is an encryption project. Security isn't a feature — it's the product.

If you find a vulnerability, we want to know about it before anyone else does.

## Reporting a vulnerability

**Do not open a public issue.** Email [opake@sans-self.org](mailto:opake@sans-self.org) with:

- A description of the vulnerability
- Steps to reproduce (or a proof of concept)
- The component affected (core, CLI, web, indexer, WASM, lexicons)
- Your assessment of severity, if you have one

## Scope

### In scope

- **opake-core** — encryption, key wrapping, seed phrase derivation, XRPC client, record types
- **opake-cli** — command handling, file storage, credential management
- **opake-wasm** — WASM bindings and the web worker bridge
- **web/** — the React SPA (auth flows, state management, UI rendering of sensitive data)
- **indexer/** — the Elixir indexer (API auth, grant/keyring discovery, rate limiting)
- **lexicons** — schema definitions under `app.opake.*`

### Out of scope

- **The PDS itself.** Opake treats the PDS as an untrusted storage layer. PDS bugs should go to [bluesky-social/pds](https://github.com/bluesky-social/pds).
- **AT Protocol core.** DID resolution, federation, sync protocol — report those upstream at [atproto.com](https://atproto.com).
- **Dependencies.** If the issue is in a third-party crate or npm package, report it to the upstream maintainer. If it's exploitable _through_ Opake specifically, that's in scope.

## What counts as a vulnerability

- Plaintext leakage — encrypted content, metadata, or key material exposed in cleartext where it shouldn't be
- Key material exposure — content keys, identity keys, or seed phrases leaked to logs, network, disk, or memory beyond their intended scope
- Auth bypass — accessing documents, grants, or keyrings without proper authorization
- Grant escalation — obtaining access beyond what a grant permits
- Injection / XSS — in the web app or indexer API
- Cryptographic weakness — flaws in the encryption scheme, key derivation, or key wrapping that reduce the effective security level

## Encryption model (summary)

Opake uses client-side encryption exclusively. The PDS never sees plaintext.

- **Content encryption:** AES-256-GCM with random per-document keys
- **Key wrapping:** X25519-HKDF-A256KW (HKDF-SHA256, not JWE's Concat KDF)
- **Key derivation:** BIP-39 24-word mnemonic → PBKDF2 → HKDF → X25519 + Ed25519 keypairs
- **Metadata:** Always encrypted with the same content key (separate nonce)
- **Ciphertext layout:** `[32-byte ephemeral pubkey || 40-byte AES-KW wrapped key]`

The full encryption model, threat assumptions, and data flow are documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Response timeline

This is a solo open-source project, not a security team with a pager. That said:

| Step                                   | Target                                |
| -------------------------------------- | ------------------------------------- |
| Acknowledge receipt                    | 48 hours                              |
| Triage and severity assessment         | 7 days                                |
| Patch for critical vulnerabilities     | 30 days                               |
| Patch for non-critical vulnerabilities | Best effort, typically within 90 days |

If a fix will take longer than the target, we'll communicate that and explain why.

## Disclosure

We follow coordinated disclosure. Once a fix is released:

1. The reporter is credited in CHANGELOG.md (with permission — let us know your preferred name/handle)
2. A description of the vulnerability and fix is published
3. If relevant, affected versions are listed so users know whether to update

We won't publish details before a fix is available unless the vulnerability is already being actively exploited.

## Bug bounties

There's no formal bounty program. This is a one-person project and there's no budget for payouts. If that changes, this section will too.

What we can offer: credit, gratitude, and the knowledge that you helped keep people's data private.
