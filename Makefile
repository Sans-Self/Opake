# Opake — build targets
#
# Frontend targets source nvm to ensure the correct Node version (.nvmrc).

SHELL := /bin/bash

NVM = source "$${NVM_DIR:-$$HOME/.nvm}/nvm.sh" && nvm use --silent

WASM_CRATE = crates/opake-wasm
WASM_OUT   = web/src/wasm/opake-wasm

.PHONY: build wasm wasm-dev install web-build

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
