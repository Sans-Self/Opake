# Opake — build targets
#
# Replaces Makefile. Each language's toolchain handles its own caching;
# just orchestrates the dependency order.

# Recipes need the nix devshell (rustc, NIX_LDFLAGS for the linker, mix, bun).
# Interactive shells get it via the direnv hook, but bare shells (CI, agents)
# don't — route every recipe through direnv so the env is loaded either way.
# No-ops cheaply when already inside; nix-direnv caches the evaluation.
set shell := ["direnv", "exec", ".", "bash", "-cu"]

wasm_crate := "crates/opake-wasm"
wasm_out := "packages/opake-sdk/wasm"
registry := env("REGISTRY", "zot.sans-self.org")
tag := `git rev-parse --short HEAD`

# Indexer Postgres (docker compose). Override via env for other setups.
db_container := env("OPAKE_DB_CONTAINER", "opake-db-1")
db_name := env("OPAKE_DB_NAME", "opake_indexer_dev")

# ---------------------------------------------------------------------------
# Rust
# ---------------------------------------------------------------------------

# Build all Rust crates
build:
    cargo build --workspace

# Check all Rust crates (faster than build)
check:
    cargo check --workspace

# Run Rust tests
rust-test:
    cargo test --workspace

# Check Rust formatting
fmt:
    cargo fmt --check

# Format Rust code
fmt-write:
    cargo fmt --all

# Run clippy
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# ---------------------------------------------------------------------------
# WASM
# ---------------------------------------------------------------------------

# Build WASM (release)
wasm:
    wasm-pack build {{ wasm_crate }} --target web --out-dir ../../{{ wasm_out }} --out-name opake

# Build WASM (debug, faster iteration)
wasm-dev:
    wasm-pack build {{ wasm_crate }} --target web --dev --out-dir ../../{{ wasm_out }} --out-name opake

# Regenerate TypeScript bindings for cross-boundary DTOs into
# packages/opake-sdk/src/generated/. Runs the ts-rs export tests in
# opake-wasm with the `ts-bindings` feature on. Diff the generated dir
# against git to detect drift between Rust DTOs and the SDK's view.
ts-bindings:
    cargo test -p opake-wasm --features ts-bindings --test ts_bindings

# ---------------------------------------------------------------------------
# SDK (@opake/sdk)
# ---------------------------------------------------------------------------

# Build all packages (WASM + core + daemon + react)
sdk-build: wasm
    cd packages/opake-sdk && bun run build
    cd packages/opake-daemon && bun run build
    cd packages/opake-react && bun run build

# Run all package tests. opake-daemon has no test files yet and bun
# errors on an empty match — add it back with its first test.
# opake-react runs under vitest (jsdom), so it needs its own script,
# not bun's test runner.
sdk-test:
    cd packages/opake-sdk && bun test
    cd packages/opake-react && bun run test

# Generate API docs
sdk-docs:
    cd packages/opake-sdk && bun run typedoc

# Build + test all packages
sdk: sdk-build sdk-test

# ---------------------------------------------------------------------------
# Web frontend
# ---------------------------------------------------------------------------

# Install frontend dependencies
install-web-deps:
    cd apps/web && bun install

# Start frontend dev server
web-dev:
    cd apps/web && bun run dev

# Production build (SDK + Vite)
web-build: sdk-build
    cd apps/web && bun run build

# Run frontend tests
web-test:
    cd apps/web && bun run test

# Lint frontend
web-lint:
    cd apps/web && bun run lint

# Typecheck frontend
web-typecheck:
    cd apps/web && bun run tsc --noEmit

# Check that spec citations (paths, bug__ tests, commit hashes) still resolve
spec-lint:
    python3 scripts/spec_lint.py

# ---------------------------------------------------------------------------
# Elixir indexer
# ---------------------------------------------------------------------------

# Start indexer dev server
indexer:
    cd apps/indexer && mix phx.server

# Run indexer tests
indexer-test:
    cd apps/indexer && mix test

# Build indexer release
indexer-release:
    cd apps/indexer && MIX_ENV=prod mix release

# Set the indexer cursor to "now" so the next start skips replaying Jetstream from its earliest retained frame. Stop the running indexer first — it holds an in-memory cursor that overwrites manual writes on the next persist.
indexer-cursor-now:
    @echo "Stop the indexer first — a running process overwrites manual cursor writes on its next persist."
    docker exec {{ db_container }} psql -U postgres -d {{ db_name }} -c "INSERT INTO cursor (id, time_us, updated_at) VALUES (1, (EXTRACT(EPOCH FROM NOW()) * 1000000)::bigint, NOW()) ON CONFLICT (id) DO UPDATE SET time_us = EXCLUDED.time_us, updated_at = EXCLUDED.updated_at;"

# ---------------------------------------------------------------------------
# E2E tests
#
# Two tiers gate on the hermetic dev-env (dev-env/) being up + bootstrapped
# (`just dev-env-up`); both fail fast with a clear message otherwise:
#   just e2e-web         — Playwright browser suite (tests/e2e), real OAuth
#                          against the local PDSes, one worker per fixture actor
#   just e2e-federation  — CLI federation tier (tests/federation), cross-PDS
#                          membership + keyring-delete outcomes via the indexer
# The default `just validate` runs neither: it stays hermetic-without-docker
# (unit + fake-pds CLI tier). Run the dev-env tiers explicitly when touching
# federation, membership, or the indexer pipeline.
# ---------------------------------------------------------------------------

# Fail fast unless the dev-env indexer is serving through Caddy on :443.
_dev-env-check:
    @curl -fsS -k --resolve indexer.test:443:127.0.0.1 https://indexer.test/api/health >/dev/null 2>&1 \
      || { echo "dev-env is not up — run 'just dev-env-up' first"; exit 1; }

# Run CLI e2e tests (requires a running PDS)
e2e-cli:
    cd tests && bun test tests/cli/

# Web e2e (Playwright) against the dev-env. Runs use an isolated namespace by
# default because membership scenarios mutate fixture accounts. Pass another
# namespace (`just e2e-web alpha`) to run separately.
e2e-web ns="e2e-web": _dev-env-check
    cd tests && E2E_ACTOR_NS={{ ns }} bunx playwright test --project=e2e

# Rebuild the dev-env CLI image iff the working-tree CLI sources changed since
# it was baked. The federation tier runs the compiled binary baked into that
# image, so without this gate a stale image silently certifies an old binary
# against current source. `ensure` compares a src-hash label to the working
# tree and rebuilds on mismatch (docker layer cache makes the no-op case cheap).
_dev-env-cli-fresh:
    @dev-env/build/build-cli.sh ensure

# CLI federation tier against the dev-env. It uses an isolated namespace by
# default; pass the same namespace as `e2e-web` when a run intentionally shares
# fixtures across the native and browser tiers.
e2e-federation ns="e2e-fed": _dev-env-check _dev-env-cli-fresh
    cd tests && OPAKE_TEST_ENV=devenv E2E_ACTOR_NS={{ ns }} bunx vitest run tests/federation/

# Harness meta-tier: tests of the e2e harness itself (snapshot liveness,
# namespace isolation). Each spec drives whole Playwright runs as child
# processes, so it is deliberately outside the product suite. Slow by nature.
e2e-harness: _dev-env-check
    cd tests && bunx playwright test --project=harness

# Delete a namespace's actors from the dev-env (accounts, records, blobs) and
# its local artifacts. Refuses the default population: those six are checked in,
# and `just dev-env-reset` is the only sanctioned way to clear them.
e2e-ns-clean ns:
    cd tests && bun run e2e/ns-clean.ts {{ ns }}

# Gate run: both reactive tiers from a pristine baseline. Resets the dev-env
# (wipes all fixture state) and forces fresh logins, so a red is attributable
# to the change under test — never to accumulated stack state (stale tokens,
# fixture buildup, workspace-count degradation). Use before commit/archive
# decisions. Running the tiers on an AGED stack instead is a soak run: new
# failures there are accumulation findings, not gate noise — file them.
e2e-gate: dev-env-reset
    cd tests && E2E_REAUTH=1 E2E_ACTOR_NS=e2e-gate bunx playwright test --project=e2e
    just e2e-federation e2e-gate

# Run all e2e tests
e2e: e2e-cli

# ---------------------------------------------------------------------------
# CI / validation
# ---------------------------------------------------------------------------

# Run all checks (CI equivalent)
validate: fmt clippy rust-test sdk web-lint web-typecheck web-build indexer-test spec-lint

# Run all tests (Rust + SDK + web + indexer)
test: rust-test sdk-test web-test indexer-test

# Run all lints (Rust + web)
lint: fmt clippy web-lint web-typecheck

# ---------------------------------------------------------------------------
# Container images
# ---------------------------------------------------------------------------

# Build container images (indexer + web)
images:
    docker build -f Containerfile.indexer \
        -t {{ registry }}/opake/indexer:{{ tag }} \
        -t {{ registry }}/opake/indexer:latest .
    docker build -f Containerfile.web \
        -t {{ registry }}/opake/web:{{ tag }} \
        -t {{ registry }}/opake/web:latest .

# Push container images to registry
push-images: images
    docker push {{ registry }}/opake/indexer:{{ tag }}
    docker push {{ registry }}/opake/indexer:latest
    docker push {{ registry }}/opake/web:{{ tag }}
    docker push {{ registry }}/opake/web:latest

# ---------------------------------------------------------------------------
# Hermetic dev environment (dev-env/) — local PLC + 3 PDSes + relay +
# jetstream + indexer. See dev-env/README.md.
# ---------------------------------------------------------------------------

# Build the dev-env's custom images (relay, jetstream, indexer, cli)
dev-env-build:
    cd dev-env && ./build/build-images.sh

# Start the stack (healthcheck-gated) and bootstrap the fixture actors
dev-env-up: dev-env-build
    # TLS material is generated locally, never committed: a published CA key
    # would be one anyone could mint trusted certificates against.
    [ -f dev-env/certs/ca.key ] || dev-env/certs/regen.sh
    cd dev-env && docker compose up -d --wait
    cd dev-env && docker compose run --rm bootstrap /bootstrap/bootstrap.sh

# Stop the stack, keep data
dev-env-down:
    cd dev-env && docker compose down

# Pristine reset: wipe all volumes (incl. jetstream cursor), restart, re-bootstrap
dev-env-reset:
    cd dev-env && docker compose down -v
    just dev-env-up

# Tail dev-env logs (optionally one service)
dev-env-logs service="":
    cd dev-env && docker compose logs -f {{ service }}

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

# Set up dev environment (git hooks, dependencies)
setup: install-web-deps
    ln -sf ../../tools/pre-commit.sh .git/hooks/pre-commit
