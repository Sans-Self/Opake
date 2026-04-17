# Licensing Guide

Opake is licensed under [AGPL-3.0](../LICENSE). This page explains what that means in practice for self-hosters, plugin developers, and contributors.

> **This is not legal advice.** It's a plain-language summary of how we interpret AGPL-3.0 as it applies to Opake. When in doubt, read [the license text](../LICENSE) or consult a lawyer.

## Quick Reference

| Scenario | Source disclosure required? |
|---|---|
| Running unmodified Opake for yourself or your org | No |
| Running modified Opake as a network service for others | **Yes** |
| Building a plugin that links against `opake-core` or `opake-wasm` | **Yes** — your plugin is AGPL |
| Building a tool that only talks to Opake over HTTP/XRPC APIs | No |
| Bundling `@opake/wasm` in your web app | **Yes** — your app is AGPL |
| Submitting a pull request | Your contribution is licensed under AGPL-3.0 |

## For Self-Hosters

**Running unmodified Opake has no extra obligations.** You don't need to publish anything — the source is already public.

**If you modify Opake and serve it to users over the network, you must provide your source code.** This is the AGPL's "network copyleft" clause (Section 13). It closes the "SaaS loophole" in regular GPL: even if you never distribute a binary, making your modified version available as a service triggers the same obligation.

Concretely:

- You fork Opake and add Dropbox sync, then let your team use it → you must make the fork's source available to those users.
- You patch a bug for your own personal use and never expose it to anyone else → no obligation (there are no "users" besides you).
- You run unmodified Opake behind a VPN for your household → no obligation.

"Provide source" means giving your users a way to get the complete corresponding source — a link to your Git repo is the simplest approach.

## For Plugin and Extension Developers

The AGPL's copyleft applies to **derivative works**, which in practice means anything that links against Opake's code at the library level.

### What triggers copyleft

- **Importing `opake-core` as a Rust dependency.** Your crate becomes a derivative work. AGPL applies to the combined work.
- **Bundling `@opake/wasm`** (the WASM module built from `opake-core`) in a web application. The entire application that incorporates the module is covered.
- **Calling `opake-core` functions from a WASM host.** Same as linking — the host application is a derivative work.

### What does NOT trigger copyleft

- **Communicating with Opake over HTTP or XRPC.** The API boundary is not a linking boundary. A mobile app that talks to the Opake Indexer over REST, or a script that calls PDS endpoints, is an independent work — license it however you want.
- **Reading or writing `app.opake.*` records on a PDS.** Lexicon schemas are interface definitions. Implementing them independently doesn't create a derivative work.
- **Running Opake alongside your software** without linking (an "aggregate" in GPL terms). Shipping a Docker Compose stack that includes Opake as a separate container is fine.

### The short version

If your code calls Opake functions by importing the library, it's AGPL. If your code talks to Opake over the network, it's not. The boundary is linking, not communication.

## For Contributors

By submitting a pull request, you agree that your contribution is licensed under AGPL-3.0, the same license as the rest of the project. There is no CLA (Contributor License Agreement) — you retain copyright over your code.

If your contribution includes code from another source, it must be compatible with AGPL-3.0. MIT, BSD, and Apache-2.0 are compatible. GPL-2.0-only is not.

## Why AGPL?

Opake is a privacy tool. The AGPL ensures that if someone takes this code, modifies it, and runs it as a service, their users can verify it still respects their privacy. Without the network clause, a hosted version could strip encryption, weaken key derivation, or add telemetry — and users would have no way to know.

The tradeoff is that the AGPL is more restrictive for commercial integrations than permissive licenses. That's intentional. If you're building a proprietary product that needs Opake's crypto, talk to the Opake over APIs instead of linking against the library.
