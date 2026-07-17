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
- **opake-wasm** — WASM bindings and the JsStorage bridge
- **packages/opake-sdk + @opake/react + @opake/daemon** — TypeScript layer (auth surfaces, storage adapters, SSE consumer wiring)
- **apps/web** — the React SPA (auth flows, state management, UI rendering of sensitive data)
- **apps/indexer** — the Elixir indexer (API auth, SSE token exchange, grant/keyring discovery, rate limiting)
- **lexicons** — schema definitions under `at.opake.*`

### Out of scope

- **The PDS itself.** Opake treats the PDS as an untrusted storage layer. PDS bugs should go to [bluesky-social/pds](https://github.com/bluesky-social/pds).
- **AT Protocol core.** DID resolution, federation, sync protocol — report those upstream at [atproto.com](https://atproto.com).
- **Dependencies.** If the issue is in a third-party crate or npm package, report it to the upstream maintainer. If it's exploitable _through_ Opake specifically, that's in scope.

> **Note for developers and researchers:** the e2e suite persists OAuth
> storage states under `tests/e2e/.auth/`. These snapshots contain live
> bearer credentials for the dev-stack test accounts. The directory is
> gitignored on purpose — never commit it, and strip its contents from
> logs, reproduction archives, or vulnerability reports.

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
- **Key wrapping:** hybrid `x25519-mlkem768-hkdf-a256kw-v2` — X25519 ECDH and ML-KEM-768 encapsulation combined through HKDF-SHA256, then AES-KW. The HKDF transcript commits to both static recipient public keys, the ephemeral X25519 key, and the ML-KEM ciphertext, so a tampered encapsulation cannot redirect the wrap
- **Key derivation:** BIP-39 24-word mnemonic → PBKDF2-HMAC-SHA512 → HKDF (separate paths) → X25519 + Ed25519 + ML-KEM-768 keypairs
- **Metadata:** Always encrypted with the same content key (separate nonce)
- **Wrapped-key layout:** `[X25519 ephemeral pubkey (32) || ML-KEM-768 ciphertext (1088) || AES-KW wrapped key (40)]`

The full encryption model, threat assumptions, and data flow are documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Trust model

Opake's federation design distributes authority across PDSes: each workspace member writes to their own PDS, and an indexer (AppView) aggregates the records into the view the client renders. Two distinct trust questions follow:

- **Can a PDS forge content?** No. Every record carries a PDS commit signature; tampered records fail signature verification. A compromised PDS can drop writes or refuse reads, but it can't substitute readable plaintext or forge another member's signature.
- **Can the indexer mislead the client?** Constrained. The indexer can lie about which records are canonical (which URI is the current head, which supersedes which) and which records exist, but it can't forge the records themselves. The client closes this gap by verifying the indexer's claims against the PDS-signed records on every read.

The indexer is treated as a **service, not a source of truth**.

### What the client verifies on every read

1. **Chain integrity.** On every keyring chain-head fetch, the client walks back from the indexer's claimed head to the genesis record via the chain's `supersedes` pointers, and verifies the genesis URI equals the workspace's stable identity. Closes "indexer points at a head from a different workspace's chain." Surfaces as `Error::ChainGenesisMismatch` on failure.

2. **Chain authority.** Walking the same chain, the client verifies every supersede in the keyring chain was authored by a manager of the immediately prior keyring. Closes "indexer accepted a supersede from a non-manager." Surfaces as `Error::ChainAuthorityViolation`.

3. **Directory additivity.** On every workspace tree load, the client checks each editor-authored directory supersede against its prior canonical and rejects any that drop entries. Managers are exempt from this check (deletions are their prerogative). Closes "indexer accepted a non-additive editor write that erased entries from a shared directory." Surfaces as `Error::ChainAdditivityViolation`.

These checks run client-side regardless of which indexer the client connects to. Swapping indexers is swapping caches, not swapping trust roots.

### What the client still trusts the indexer for

- **Freshness.** A compromised indexer can point at a real-but-older head and hide a newer supersede. This is unaddressable without out-of-band signals (polling every member's `listRecords`, defeating the indexer's bandwidth-aggregation purpose). Staleness is treated as a distributed-systems posture, not a security failure: the indexer can delay updates but can't fabricate them. Once the newer record is observed (via a different indexer, a re-sync after a cache eviction, or the firehose itself), the client's chain walk confirms it.
- **Discovery and listing.** "What documents exist in this workspace" comes from the indexer's snapshot. A compromised indexer could omit records — same staleness category. The client can detect omissions on chain walks (a missing intermediate breaks the back-walk) but can't enumerate what _should_ exist without consulting every member's PDS.

### What's out of scope of the trust checks

- **Within a member's PDS.** Each member's PDS is in their own TCB. A compromised member-PDS can forge that member's signatures. The federated model intentionally accepts this: distributing authority means each authority is a separate trust root.
- **Encryption keys on the device.** Client keys live on the device that holds them. Compromising the device compromises the keys it stores.
- **Bluesky social graph.** PDS handle resolution goes through the AT Protocol DID and PLC directory. Compromising those affects identity resolution upstream of Opake.

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
