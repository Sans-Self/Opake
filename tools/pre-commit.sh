#!/bin/sh
set -e

# Skip on branches without Rust code
if [ ! -f Cargo.toml ]; then
    exit 0
fi

# Rust checks (only if Rust files are staged)
if git diff --cached --name-only | grep -q '\.rs$'; then
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo check --target wasm32-unknown-unknown -p opake-core
fi

# Web frontend checks (only staged files under apps/web/src/)
if git diff --cached --name-only | grep -q '^apps/web/src/'; then
    STAGED_WEB=$(git diff --cached --name-only --diff-filter=d | grep '^apps/web/src/' | sed 's|^apps/web/||')
    if [ -n "$STAGED_WEB" ]; then
        (cd apps/web && bun run format:check && bunx eslint $STAGED_WEB)
    fi
fi
