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
    # pmhvd starts pmhv-shim from next to itself, so build everything first.
    cargo build --workspace
    just sign
    trap 'kill 0' EXIT
    ./target/debug/pmhvd &
    npm --prefix web run dev

# Build the web UI into web/dist
build-web:
    npm --prefix web run build

# Build the web UI, then the workspace with the UI embedded
build: build-web
    cargo build --workspace --release
    just sign release

# Ad-hoc sign pmhv-shim with the Virtualization.framework entitlement (macOS only). Cargo
# rebuilds drop the signature, so this runs after every build that might use Apple Virtualization.
sign profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$(uname)" == Darwin ]]; then
        codesign --sign - --force --entitlements crates/pmhv-shim/pmhv-shim.entitlements target/{{profile}}/pmhv-shim
    fi

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

# Boot real firmware under QEMU through pmhvd and the shim (needs QEMU)
test-integration:
    cargo build --workspace
    just sign
    cargo test -p pmhvd --test boot -- --ignored

# Run a dummy machine through pmhvd and the shim (no hypervisor needed)
test-dummy:
    cargo build --workspace
    cargo test -p pmhvd --test boot -- --ignored dummy

# Licence metadata (REUSE), for tracked and staged files only so node_modules is skipped
licences:
    git ls-files -z --cached --others --exclude-standard | xargs -0 reuse lint-file
