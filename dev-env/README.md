# Opake dev-env

A hermetic, fully-local atproto network for e2e testing and manual development:
a local PLC directory, three PDSes (each behind Caddy TLS), the indigo relay,
jetstream (built from source), and the Opake indexer with its database. Every
component runs unmodified production code — all dev-env behaviour is
configuration, so nothing here can drift from what ships.

## Layout

```
dev-env/
  docker-compose.yml         the stack
  caddy/Caddyfile            TLS vhosts: pds-{a,b,c}.test, plc.test, indexer.test
  certs/                     local dev CA + one leaf cert (SANs for every vhost)
  fixtures/actors.json       6 fixed-mnemonic actors, 2 per PDS  (owned by fixtures task)
  fixtures/generate_mnemonics.py  regenerates the fixture set (run once, checked in)
  bootstrap/bootstrap.sh     provisions the actors (runs inside the network)
  bootstrap/verify-cli.sh    CLI smoke test against the running stack
  build/                     Dockerfiles + build-images.sh for the custom images
```

## Topology

```
                         edge network (plain bridge)
                         published: 127.0.0.1:443
                                   │
                              ┌────┴────┐
                              │  Caddy  │  TLS terminator + only host boundary
                              └────┬────┘  vhosts → pds-{a,b,c}, plc, indexer
  ══════════════════════════════ internal network (internal: true, no egress) ══
     11.10.0.0/24 — public-range dark space (see below)
                                   │
         ┌──────────┬─────────────┼───────────────┬──────────────┐
         │          │             │               │              │
      ┌──┴──┐   ┌───┴───┐    ┌────┴────┐      ┌────┴───┐     ┌────┴────┐
      │ plc │   │ pds-a │    │  pds-b  │      │ pds-c  │     │ indexer │
      └──┬──┘   └───┬───┘    └────┬────┘      └────┬───┘     └────┬────┘
      ┌──┴───┐      └────────┬────┴────────────────┘         ┌────┴─────┐
      │plc-db│               │  requestCrawl / firehose      │indexer-db│
      └──────┘          ┌────┴────┐                          └──────────┘
                        │  relay  │  fan-in (one subscribeRepos upstream)
                        └────┬────┘
                        ┌────┴─────┐
                        │jetstream │  /subscribe :8080  ─────► indexer
                        └──────────┘
```

DID ops flow PDS → plc; record commits flow PDS → relay → jetstream → indexer →
indexer-db, and the indexer pushes SSE back out to clients through Caddy's
`indexer.test` vhost. The relay exists because jetstream consumes exactly one
`subscribeRepos` stream; with three PDSes it is the required fan-in hop. This is
a higher-fidelity path than Opake's own production setup (which runs no relay and
rides Bluesky's hosted jetstream) — a hermetic run can host nothing external, so
it owns the whole pipeline.

### Networks & hermeticity

Two networks, and the split is load-bearing:

- **`internal`** — `internal: true`, **no external egress**, subnet
  `11.10.0.0/24`. Carries ALL inter-service traffic. The subnet is deliberately
  in PUBLIC (non-reserved) space — `11.0.0.0/8` is US-DoD dark space, never
  routed on the internet — because the indigo relay's SSRF guard
  (`util/ssrf.PublicOnlyControl`) rejects RFC1918/loopback IPs at dial time, with
  no flag to disable it (`--allow-insecure-hosts` only gates the `ws://` scheme).
  Public-range container IPs pass `IsPublicIPAddress()`, so the **stock relay
  works unmodified**. **FOOTGUN:** the host installs a route for this subnet to
  the docker bridge — only ever pick a range the host will never legitimately
  contact (NEVER `8.8.0.0/16`, etc). Safe only because egress is blocked.
- **`edge`** — a plain bridge for host-published ports. **Only Caddy attaches
  here**, so Caddy is the single boundary crossing. Every other service is
  internal-only and therefore provably egress-blocked (verified: relay,
  jetstream, indexer, PDSes all fail to reach the outside world).

Host-facing surface: Caddy on `127.0.0.1:443` (TLS, routing every `*.test` vhost
by SNI). Everything else is reached internally or via `docker compose exec`.

## Lifecycle

```sh
just dev-env-up      # build images (if needed) → compose up --wait → bootstrap
just dev-env-down    # compose down (keeps volumes)
just dev-env-reset   # compose down -v (WIPES volumes) → dev-env-up
just dev-env-logs [service]
```

The underlying steps, if you want them by hand:

```sh
./build/build-images.sh                                   # custom images (once / on change)
docker compose up -d --wait                               # start; waits on healthchecks
docker compose run --rm bootstrap /bootstrap/bootstrap.sh # provision fixture actors
```

**`reset` blast radius.** `down -v` destroys every data volume — PLC db, all
three PDS repos, relay + jetstream state, indexer db. Bootstrap then recreates
the accounts from scratch, which means **new `did:plc` values**: the PDS mints
fresh signing keys, and nothing guarantees DID stability across a reset (only the
*encryption* keys are stable, since they derive from the fixed mnemonics). Any
artefact that pinned a DID is now stale. The one that bites in practice:

> **Web e2e `.auth/` storageStates go stale after any reset.** `tests/e2e/`
> persists one OAuth session per actor under `.auth/<actor>.json`, and the setup
> project skips re-auth when that file is younger than 6h (mtime TTL, *not* a DID
> check). After a reset the file looks fresh but holds a session for a DID that
> no longer exists, so specs fail on a dead session. Force a fresh login with
> **`E2E_REAUTH=1`** (or delete `tests/e2e/.auth/`) on the first run after a reset.

## Images (all pinned)

| Image | Source |
|-------|--------|
| `opake-devenv-plc:pinned` | did-method-plc, git build context (`packages/server/Dockerfile`) |
| `ghcr.io/bluesky-social/pds:0.4` | pulled |
| `caddy:2.8-alpine`, `postgres:16-alpine` | pulled |
| `opake-devenv-relay:pinned` | indigo `cmd/relay` cloned + built in-image (`build/relay.Dockerfile`, Go ≥ 1.26) |
| `opake-devenv-jetstream:pinned` | jetstream `main` from source, pinned `JS_REF` commit (`build/jetstream.Dockerfile`) — the maintained rewrite, no 15s idle self-kill |
| `opake-devenv-indexer:pinned` | `apps/indexer` as a prod `mix release` (`build/indexer.Dockerfile`) |
| `opake-devenv-cli:pinned` | `opake` CLI, for bootstrap (`build/cli.Dockerfile`) |

Relay and jetstream clone their upstream *inside* the build rather than using a
remote git context: indigo's own Dockerfile runs `git describe --tags`, which a
BuildKit remote context can't satisfy (it checks out without `.git`).

## Fixture actors

Six actors, two per PDS, defined in `fixtures/actors.json`. Identities derive
from fixed BIP-39 mnemonics, so each actor always resolves to the same X25519
encryption key and republishes the same `app.opake.publicKey/self` across resets.
DIDs are *not* stable (see reset blast radius) — always address actors by handle
and resolve.

| Name | Handle | PDS |
|------|--------|-----|
| alice | `alice.pds-a.test` | pds-a |
| bob | `bob.pds-a.test` | pds-a |
| carol | `carol.pds-b.test` | pds-b |
| dave | `dave.pds-b.test` | pds-b |
| eve | `eve.pds-c.test` | pds-c |
| frank | `frank.pds-c.test` | pds-c |

All share the account password `opake-devenv-pw` (per-actor `password` field,
env `ACTOR_PASSWORD` as fallback). The 24-word mnemonics are **public test
vectors** — not secrets, never valid on a live PDS, deliberately kept outside
git-crypt so nobody confuses them with real credentials (contrast
`tests/accounts.secret`). They live in full in `actors.json`; not reproduced here.

## Actor namespaces

The six above are the *default population*: checked in, bootstrapped with the
environment, and shared by everything that doesn't ask for otherwise. A test run
can instead ask for a population of its own by setting `E2E_ACTOR_NS`
(`just e2e-web alpha`, `just e2e-federation alpha`), which gets it six actors
mirroring the same roles and the same PDS placement:

| Namespace | Handle | PDS |
|-----------|--------|-----|
| `alpha` | `alice-alpha.pds-a.test` | pds-a |
| `alpha` | `frank-alpha.pds-c.test` | pds-c |

Nothing about a namespace is written down. Its handles are `<role>-<ns>.<pds>.test`
and its mnemonics are BIP-39 words over 32 bytes of `SHA-256("opake-e2e:<ns>:<role>")`,
so the namespace name is the entire registry — provision it twice, against a reset
environment or a live one, and every actor comes back with the same handle, the
same PDS, and the same published encryption key.

Grammar is `[a-z0-9-]{1,12}`, validated before anything touches the network. The
12 is the PDS's doing: it rejects handles over 29 characters, and
`alice-<ns>.pds-a.test` spends 17 of them before the namespace starts.

Provisioning happens on demand, from the test harness (`tests/e2e/pds-admin.ts`),
the first time a namespace is used: it runs the same `bootstrap.sh` recipe below
for the actors that don't resolve yet, so a namespaced actor gets exactly what a
checked-in one gets — a live account, a published `publicKey/self` derived from
its mnemonic, and a seeded cabinet (`frank` excepted, as ever). Actors that
already exist are left alone, records and all.

Namespaces are individually disposable: `just e2e-ns-clean alpha` deletes that
namespace's accounts (and with them their records and blobs) and drops its local
artifacts. It refuses the default population — those six are checked in, and
`just dev-env-reset` remains the way to clear them. Deleting a namespace touches
no other namespace and no default actor, which is what makes concurrent runs
against one environment safe to clean up after independently.

## Bootstrap

`bootstrap.sh` runs the opake CLI **inside** the internal network, reaching the
PDSes over plain http — so the PDSes never need host ports and stay
egress-blocked. Per actor:

1. mint an invite code (PDS admin API) → `createAccount` (public XRPC),
2. seed a legacy session + account config from the returned tokens. The CLI
   reads its PDS URL from account config, so it stays on `http://pds-x:3000`
   rather than resolving the DID-doc `https` endpoint,
3. `opake recover` imports the fixed mnemonic and publishes
   `app.opake.publicKey/self`,
4. seed the cabinet root: one tiny `opake upload`. A recovered cabinet has no
   root directory until something is written — `ls`/`mkdir`/web-upload all fail
   with "no root directory" until then (only a document upload materialises the
   root; `mkdir` makes an orphan child). The web UI does *not* genesis-create the
   root on first write (product gap, tracked in findings), so the web
   doc-upload spec needs the root to already exist. The placeholder is benign;
   specs create their own uniquely-named files.

Chosen over `opake account login` because legacy login generates a *fresh random*
identity behind an interactive 3-word confirmation; the seed-then-recover shape
imports a known mnemonic with no key mismatch prompt, so the whole thing is
non-interactive. `LIMIT=alice` restricts it to one actor. Assumes a clean
(post-reset) network.

## Adding an actor

1. Add an entry to `fixtures/actors.json` — `name`, `handle`
   (`<name>.<pds>.test`), `pds` (the compose service, e.g. `pds-c`), `mnemonic`,
   optional `password`.
2. Generate the mnemonic with `fixtures/generate_mnemonics.py` (BIP-39 24-word,
   using the product's own wordlist at `crates/opake-crypto/src/bip39_english.txt`).
   Note it regenerates the *whole* set — rerun only when deliberately rotating
   all fixtures, then copy the new phrase for your one actor rather than clobbering
   `actors.json`.
3. `just dev-env-reset` (or `docker compose run --rm bootstrap
   /bootstrap/bootstrap.sh` on a fresh stack) provisions it. `LIMIT=<name>` on a
   live stack does just the new actor.

Web e2e partitions Playwright workers by actor (`workerIndex` → fixture), so
adding actors widens the parallelism budget there.

## Adding a PDS

1. **Compose service** — copy a `pds-x` block: set `PDS_HOSTNAME: pds-d.test`,
   give it a unique `PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX` and its own data
   volume. It inherits the shared `x-pds-common` / `x-pds-env` anchors.
2. **Handle domain** — add `.pds-d.test` to `PDS_SERVICE_HANDLE_DOMAINS` in
   `x-pds-env`. Handles must be a single label under a PDS subdomain
   (`alice.pds-d.test`); a bare `.test` rejects the multi-label prefix.
3. **Caddy vhost** — add a `pds-d.test, *.pds-d.test { … reverse_proxy
   http://pds-d:3000 }` block, and the internal-network `aliases` entry on the
   caddy service (`pds-d.test`). The wildcard carries per-handle
   `/.well-known/atproto-did` resolution plus OAuth pages and XRPC.
4. **Cert SAN** — add `DNS:pds-d.test` and `DNS:*.pds-d.test` to `certs/san.ext`
   and regenerate the leaf cert against the dev CA. One cert covers every vhost.
5. **Bootstrap** — add actors on the new PDS to `actors.json`; the bootstrap
   loop is data-driven and needs no code change.

## Config that matters

- **CLI → local indexer:** `OPAKE_INDEXER_URL` (or `OPAKE_CLI_INDEXER_URL`) =
  `http://indexer:6100` inside the net. Without it the CLI targets production
  `indexer.opake.app`.
- **CLI / clients → local PLC:** `OPAKE_PLC_DIRECTORY=http://plc:2582` natively.
  In browser WASM `std::env::var` is inert, so the base URL threads through
  runtime config (`VITE_PLC_DIRECTORY_URL=https://plc.test`).
- **CLI → PDS:** bootstrap seeds `pds_url = http://pds-x:3000`, so the bootstrap
  flow itself needs no TLS. On the https path (resolving a PDS from a DID-doc
  `serviceEndpoint`, i.e. cross-PDS record fetches) the CLI trusts the dev CA via
  `SSL_CERT_FILE=/certs/ca.crt` — its rustls stack honours it through
  `rustls-platform-verifier` → `rustls-native-certs`, no code change.
- **Indexer → PDS over TLS:** the indexer's Req/Mint client (OTP
  `:public_key.cacerts_get`, the OS trust store) must trust the dev CA to fetch
  records/keys over `https://pds-x.test` (auth key fetch + backfill). Injected
  config-only by `build/indexer-entrypoint.sh`, which adds the CA to the OS trust
  store at boot; the app's own entrypoint/source is untouched.
- **Indexer → local PLC:** `PLC_DIRECTORY_URL=http://plc:2582` (replaces the
  hardcoded `https://plc.directory` in `auth/key_fetcher.ex` + `backfill.ex`).
- **PDS admin:** `PDS_ADMIN_PASSWORD` mints invites.
- **Outbound neutralised:** every PDS `*_URL` (appview, mod service) points at
  `localhost:3000` and there is no SMTP — belt-and-braces on top of the egress
  block; the PDS logs invite/verification mails instead of sending.

## Consumers

Who talks to this stack:

- **Web e2e** (`tests/playwright.config.ts`, specs under `tests/e2e/`). The
  browser resolves every `*.test` host to Caddy via
  `--host-resolver-rules=MAP *.test 127.0.0.1:443` and trusts the dev CA via
  `ignoreHTTPSErrors`. A browser-side route blockade (`fixtures.ts`) fails any
  non-local request, so a resolver escape to `plc.directory`/`bsky.network`
  fails the test loudly. The app runs in Vite `--mode devenv`
  (`apps/web/.env.devenv`).
  **GOTCHA — env precedence:** Vite gives ambient process env *precedence over*
  `.env.[mode]` files. The repo `.envrc` exports `VITE_INDEXER_URL` /
  `OPAKE_INDEXER_URL` for host-side dev, which would silently shadow the devenv
  values and point the app at a non-hermetic indexer. The Playwright config
  therefore reads `.env.devenv` itself and lifts it into `webServer.env` so the
  mode file wins regardless of the spawning shell. **Anyone wiring a new consumer
  must guard against the same leak.**
- **CLI verification** (`bootstrap/verify-cli.sh`) — `docker compose run --rm
  bootstrap /bootstrap/verify-cli.sh`. Logs in as a fixture actor, uploads,
  waits out the pds → relay → jetstream → indexer pipeline, downloads, compares,
  and resolves a cross-PDS actor by DID.
- **Indexer** — consumes the dev-env firehose via `JETSTREAM_URL`
  (`ws://jetstream:8080/subscribe`) and serves its API/SSE on `:6100`
  (`indexer.test` through Caddy for browsers). `CORS_ORIGIN` allows the host web
  server's cross-origin calls.

## Known gotchas

- **PLC image entrypoint reads `DB_CREDS_JSON`, not `DATABASE_URL`** (crashes
  "Unexpected token u in JSON" otherwise).
- **jetstream `/subscribe` is on `--addr :8080`; health is `/healthz` on
  `--debug-addr :8085`.** New jetstream emits `{"kind":"commit",...}` frames the
  indexer parses (verified end-to-end).
- **Indexer cold-start migration race:** on a fresh DB the first boot may crash
  once (consumer starts before the `cursor` table exists), then the restart
  policy reboots it and it converges. Harmless; the healthcheck only passes once
  it's actually serving.
- **`internal: true` breaks host-published ports** — that's why the two-network
  split exists.

## Known gaps

- **No handle→DID resolution for foreign actors from the HOST.** Resolving
  `alice.pds-a.test` tries `https://alice.pds-a.test/.well-known/atproto-did` and
  then falls back to the external Bluesky API. In the browser this works: the
  Playwright `--host-resolver-rules` map `*.test` to Caddy, whose wildcard vhosts
  serve per-handle well-known (the membership specs add members by handle this
  way). A host-side Node/CLI process has no such resolver mapping, so the
  well-known probe fails and the external fallback dies at the blockade —
  host-side cross-PDS lookups therefore resolve **by DID** (DID-doc via the local
  PLC → the publicKey record from the actor's own PDS); `verify-cli.sh` does
  exactly this.
- **No workspace-delete primitive.** Test junk (workspaces, directory entries)
  accumulates until the next `reset` — there is no way to prune a single
  workspace. Heavy accumulation noticeably slows web boot (the client rebuilds
  its projection from full snapshots on cold start), so long-lived stacks that
  never reset degrade; `reset` is the only cure.
