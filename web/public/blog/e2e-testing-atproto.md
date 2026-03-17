# E2E testing atproto apps, except less miserable

So you built something on the AT Protocol. You did the hard part — the custom lexicons, the encryption, the key management, the OAuth flow that involves more RFCs than you have fingers. And now you need to test it. Shit.

And by "test it" I don't mean unit tests. I mean _actually opening a browser_, watching a real login redirect chain complete, watching real WASM encrypt a real file, watching that file show up in a list with its decrypted name. Because your unit tests — lief, en gewaardeerd — are lying to you. The DPoP nonce rotation broke three releases ago. The mock said everything was fine.

You're looking at this stack. PDS. PLC directory. Authorisation server. AppView. WASM crypto worker. IndexedDB. One moment you're thinking "I'll mock it." and the next you've worked on an in-memory PDS Oops.

## fake-pds

We made [Opake](https://opake.app); an encrypted personal cloud on atproto. Every operation involves at minimum three async round-trips through WASM, and the test environment either speaks OAuth 2.0 with DPoP proofs or _nothing works at all_.

So we built [fake-pds](https://www.npmjs.com/package/fake-pds). An in-memory AT Protocol PDS. About 500 lines of TypeScript. No Docker. No Postgres. No YAML.

```typescript
import { createFakePds } from "fake-pds";

const pds = await createFakePds({
  accounts: [
    { did: "did:plc:alice", handle: "alice.test" },
    { did: "did:plc:bob", handle: "bob.test" },
  ],
  auth: "oauth",
});
```

It speaks XRPC — `getRecord`, `putRecord`, `createRecord`, `deleteRecord`, `uploadBlob`. Handles the full OAuth flow: PAR, authorise, token exchange, DPoP nonce rotation, refresh tokens. Serves DID documents. Resolves handles. Everything a real PDS does, minus the Merkle tree and federation — which don't matter for testing your app.

Starts in under 50ms.

## The OAuth bit

Real atproto OAuth is _a lot_. I only started working with in 2,5 weeks ago and I am loving it but, damn. Especially with oauth: Pushed Authorisation Request. Browser redirect to the authorisation endpoint. User consent. Redirect back with a code. DPoP-bound token exchange. Every RFC between 2020 and 2024, simultaneously, with proof-of-possession.

The thing is, all of this complexity exists for good reason. DPoP prevents token theft. PAR prevents authorisation request tampering. The browser redirect ensures the user actually consents. You _want_ your tests to exercise this path, because when the nonce tracking breaks or the PKCE verifier doesn't round-trip correctly, you want to find out before your users do.

fake-pds auto-approves at the authorisation step — the `/oauth/authorize` endpoint receives the `request_uri` from PAR, looks up the pending authorisation, and immediately 302-redirects back with the code. No consent screen. But every other part of the flow is real: real DPoP proofs, real nonce rotation, real token types.

```typescript
await page.getByLabel("AT Protocol handle").fill("alice.test");
await page.getByRole("button", { name: /Sign in/ }).click();

await expect(page.getByText(/Welcome to Opake|You're all set/)).toBeVisible({ timeout: 15_000 });
```

Handle resolution. AS discovery. PAR with DPoP. Authorisation redirect. Code exchange. Four seconds.

The value here isn't speed — it's coverage. Your test exercises the same code path as production. When the OAuth flow breaks, you find out in CI, not from someone on Bluesky saying "login doesn't work."

## Making resolution testable

Your app normally resolves handles via `public.api.bsky.app` and DID documents via `plc.directory`. In tests, both need to hit fake-pds instead. Two environment variables:

```typescript
const BSKY_PUBLIC_API =
  (import.meta.env.VITE_RESOLVE_API as string | undefined) ?? "https://public.api.bsky.app";
```

The Playwright global setup starts fake-pds, then launches Vite with these pointed at it. No mocks inside the app. No dependency injection. Your app runs its real code against a real (fake) server.

Denk daar even over na. The resolution path — handle to DID, DID to DID document, DID document to PDS URL — is one of the most failure-prone parts of atproto. Handles can point to the wrong DID. DID documents can have stale service endpoints. PDS URLs can be behind redirects. If you're mocking this path, you're skipping the part that actually breaks.

fake-pds doubles as a PLC directory. `GET /did:plc:alice` returns a valid DID document with the PDS URL filled in. Handle resolution and DID resolution both go to the same server. Your tests exercise the full resolution chain.

## Parallel isolation

56 tests. One shared PDS. Four parallel workers.

The naive approach: call `pds.reset()` between tests. This wipes _everything_ — including OAuth tokens from other workers' active sessions. The result: random timeout failures, completely nondeterministic, different test flakes each run.

The fix is per-test account isolation. fake-pds ships with `generateAccounts` and `AccountPool`:

```typescript
import { generateAccounts, AccountPool } from "fake-pds";

const pool = generateAccounts(50);
const pds = await createFakePds({ accounts: [...pool], auth: "oauth" });
```

`AccountPool` uses atomic file creation (`O_EXCL`) for cross-process locking. Each test acquires a unique account, does its thing, releases. No shared memory, works across any parallel test runner — Playwright, Vitest, Jest.

```typescript
const { account, release } = accountPool.acquire();
// test runs as anika.test — nobody else touches anika's state
release();
```

For cleanup, instead of the global reset, there's per-DID cleanup:

```
POST /_test/cleanup?did=did:plc:anika
```

Clears records, blobs, and tokens for one account. Other accounts keep working. This makes my tests pretty reliable! We're not just writing better assertions, we're not destroying the state our parallel workers depend on.

## Client-side crypto in the test loop

Opake encrypts everything client-side via WASM. The seed phrase flow generates a BIP-39 mnemonic, derives X25519 + Ed25519 keypairs, and publishes the public key to the PDS. This runs in the Playwright browser during tests and natively during my CLI integration tests.

We considered shortcuts. Pre-generating keypairs. Injecting sessions into IndexedDB. But you need a valid DPoP keypair that the WASM worker can use for subsequent API calls, and generating one in Node means matching the exact binary format the Rust code expects. The shortcut was more complex than the real thing.

So we do the real thing. Five seconds per test. The WASM worker generates real keys, IndexedDB gets real data, the PDS receives a real signed record.

The value: when the key derivation path changes — and it will, because post-quantum upgrades are on the roadmap — the tests catch it. When the encrypted metadata format changes, the tests catch it. The cost is five seconds of WASM execution per test. The alternative is shipping a broken key derivation to production and not knowing until someone can't decrypt their files.

## What we ended up with

56 browser tests across 12 files:

- OAuth login flow — success, invalid handle, callback errors, auth guards
- Seed phrase — generation, confirmation, recovery from another device, wrong phrase detection
- File operations — upload, delete, folder creation, navigation, download
- Sharing — dialog interactions, error states, access control
- Settings, logout, device pairing, mobile viewport, metadata editing

Plus 73 CLI e2e tests against the same fake-pds.

Two minutes, four parallel workers, zero external dependencies.

## Use it

[fake-pds is on npm](https://www.npmjs.com/package/fake-pds).

```bash
npm install --save-dev fake-pds
```

Account pool. OAuth auto-approve. Per-DID cleanup. DID document resolution. CORS headers for cross-origin browser testing. All out of the box.

Your tests talk to a real HTTP server that speaks the real protocol. The parts that matter — resolution, authentication, record storage — are all exercised. The parts that don't — federation, MST, firehose — are skipped. Help each other out. Share the tooling. Make it less painful for the next person.

---

_[Opake](https://opake.app) — encrypted collaboration on the AT Protocol. [Source](https://tangled.org/sans-self.org/opake)._
