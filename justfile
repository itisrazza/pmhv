set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Install the web UI's dependencies
web-deps:
    npm --prefix web ci

# Run pmhvd and the Vite dev server (with /api proxied to pmhvd)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    cargo run -p pmhvd &
    npm --prefix web run dev

# Build the web UI into web/dist
build-web:
    npm --prefix web run build

# Build the web UI, then the workspace with the UI embedded
build: build-web
    cargo build --workspace --release

# Regenerate web/openapi.json and the web API client from the Rust types
generate:
    cargo run -q -p pmhvd -- openapi > web/openapi.json
    npm --prefix web run generate

# Format Rust and web sources
fmt:
    cargo fmt --all
    npm --prefix web run fmt

# Check formatting without changing anything
fmt-check:
    cargo fmt --all --check
    npm --prefix web run fmt:check

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    npm --prefix web run lint

check:
    cargo check --workspace --all-targets
    npm --prefix web run typecheck

test:
    cargo test --workspace

# Licence metadata (REUSE), for tracked and staged files only so node_modules is skipped
licences:
    git ls-files -z --cached --others --exclude-standard | xargs -0 reuse lint-file
