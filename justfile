# Opake — build targets
#
# Replaces Makefile. Each language's toolchain handles its own caching;
# just orchestrates the dependency order.

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

# Run all package tests
sdk-test:
    cd packages/opake-sdk && bun test
    cd packages/opake-daemon && bun test
    cd packages/opake-react && bun test

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
# ---------------------------------------------------------------------------

# Run CLI e2e tests (requires a running PDS)
e2e-cli:
    cd tests && bun test tests/cli/

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
# Setup
# ---------------------------------------------------------------------------

# Set up dev environment (git hooks, dependencies)
setup: install-web-deps
    ln -sf ../../tools/pre-commit.sh .git/hooks/pre-commit
