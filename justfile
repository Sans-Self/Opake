# Opake — build targets
#
# Replaces Makefile. Each language's toolchain handles its own caching;
# just orchestrates the dependency order.

wasm_crate := "crates/opake-wasm"
wasm_out := "packages/opake-sdk/wasm"
registry := env("REGISTRY", "zot.sans-self.org")
tag := `git rev-parse --short HEAD`

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

# ---------------------------------------------------------------------------
# Elixir appview
# ---------------------------------------------------------------------------

# Start appview dev server
appview:
    cd apps/appview && mix phx.server

# Run appview tests
appview-test:
    cd apps/appview && mix test

# Build appview release
appview-release:
    cd apps/appview && MIX_ENV=prod mix release

# ---------------------------------------------------------------------------
# E2E tests
# ---------------------------------------------------------------------------

# Run CLI e2e tests (requires a running PDS)
e2e-cli:
    cd tests && bun test tests/cli/

# Run web e2e tests (requires running web + appview)
e2e-web:
    cd tests && bun test tests/web/

# Run all e2e tests
e2e: e2e-cli e2e-web

# ---------------------------------------------------------------------------
# CI / validation
# ---------------------------------------------------------------------------

# Run all checks (CI equivalent)
validate: fmt clippy rust-test sdk web-lint web-typecheck web-build appview-test

# Run all tests (Rust + SDK + web + appview)
test: rust-test sdk-test web-test appview-test

# Run all lints (Rust + web)
lint: fmt clippy web-lint web-typecheck

# ---------------------------------------------------------------------------
# Container images
# ---------------------------------------------------------------------------

# Build container images (appview + web)
images:
    docker build -f Containerfile.appview \
        -t {{ registry }}/opake/appview:{{ tag }} \
        -t {{ registry }}/opake/appview:latest .
    docker build -f Containerfile.web \
        -t {{ registry }}/opake/web:{{ tag }} \
        -t {{ registry }}/opake/web:latest .

# Push container images to registry
push-images: images
    docker push {{ registry }}/opake/appview:{{ tag }}
    docker push {{ registry }}/opake/appview:latest
    docker push {{ registry }}/opake/web:{{ tag }}
    docker push {{ registry }}/opake/web:latest

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

# Set up dev environment (git hooks, dependencies)
setup: install-web-deps
    ln -sf ../../tools/pre-commit.sh .git/hooks/pre-commit
