#!/bin/sh
set -e

# Skip on branches without Rust code (e.g. crosslink/hub coordination branch)
if [ ! -f Cargo.toml ]; then
    exit 0
fi

# Link issue references in CHANGELOG.md
if [ -f tools/crosslink-issue-renderer/link-changelog.sh ] && [ -f CHANGELOG.md ]; then
    sh tools/crosslink-issue-renderer/link-changelog.sh https://issues.opake.app CHANGELOG.md
    git add CHANGELOG.md
fi

# Rust checks (only if Rust files are staged)
if git diff --cached --name-only | grep -q '\.rs$'; then
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo check --target wasm32-unknown-unknown -p opake-core
fi

# Web frontend checks (only if web/ files are staged)
if git diff --cached --name-only | grep -q '^apps/web/src/'; then
    (cd apps/web && bun run format:check && bun run lint)
fi
