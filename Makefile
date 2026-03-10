# Opake — build targets
#
# Frontend targets source nvm to ensure the correct Node version (.nvmrc).

SHELL := /bin/bash

NVM = source "$${NVM_DIR:-$$HOME/.nvm}/nvm.sh" && nvm use --silent

WASM_CRATE = crates/opake-wasm
WASM_OUT   = web/src/wasm/opake-wasm

.PHONY: build wasm wasm-dev install-web-devs web-build setup \
       validate lint test rust-test fmt clippy web-lint web-typecheck web-test \
       appview appview-test appview-release \
       images push-images

## Build all Rust crates
build:
	cargo build

## Build WASM (release)
wasm:
	wasm-pack build $(WASM_CRATE) --target web --out-dir ../../$(WASM_OUT) --out-name opake

## Build WASM (debug, faster iteration)
wasm-dev:
	wasm-pack build $(WASM_CRATE) --target web --dev --out-dir ../../$(WASM_OUT) --out-name opake

## Install frontend dependencies
install-web-devs:
	cd web && $(NVM) && bun install

## Production build (WASM + tsc + Vite)
web-build: wasm
	cd web && $(NVM) && bun run build

## Set up dev environment (git hooks, dependencies)
setup: install-web-devs
	ln -sf ../../tools/pre-commit.sh .git/hooks/pre-commit

# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

REGISTRY ?= zot.sans-self.org
TAG      ?= $(shell git rev-parse --short HEAD)

## Run all checks (CI equivalent)
validate: fmt clippy test web-lint web-typecheck wasm web-test web-build

## Check Rust formatting
fmt:
	cargo fmt --check

## Run clippy
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

## Run all tests (Rust + web + appview)
test: rust-test web-test appview-test

## Run Rust tests
rust-test:
	cargo test --workspace

## Lint frontend
web-lint: install-web-devs
	cd web && $(NVM) && bun run lint

## Typecheck frontend
web-typecheck: install-web-devs
	cd web && $(NVM) && bun run tsc --noEmit

## Run frontend tests
web-test: install-web-devs
	cd web && $(NVM) && bun run test

## Run all lints (Rust + web)
lint: fmt clippy web-lint web-typecheck

# ---------------------------------------------------------------------------
# Elixir appview
# ---------------------------------------------------------------------------

## Run appview tests
appview-test:
	cd appview && mix test

## Start appview dev server
appview:
	cd appview && mix phx.server

## Build appview release
appview-release:
	cd appview && MIX_ENV=prod mix release

# ---------------------------------------------------------------------------
# Container images
# ---------------------------------------------------------------------------

## Build container images (appview + web)
images:
	docker build -f Containerfile.appview \
		-t $(REGISTRY)/opake/appview:$(TAG) \
		-t $(REGISTRY)/opake/appview:latest .
	docker build -f Containerfile.web \
		-t $(REGISTRY)/opake/web:$(TAG) \
		-t $(REGISTRY)/opake/web:latest .

## Push container images to registry
push-images: images
	docker push $(REGISTRY)/opake/appview:$(TAG)
	docker push $(REGISTRY)/opake/appview:latest
	docker push $(REGISTRY)/opake/web:$(TAG)
	docker push $(REGISTRY)/opake/web:latest
