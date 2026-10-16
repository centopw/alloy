# Alloy task runner recipes (justfile)
# See CONTRIBUTING.md and docs/CI.md for documentation.

set shell := ["bash", "-cu"]

# UI workspace directory (apps/alloy by default; apps/alloy/ui configurable)
UI_DIR := env_var_or_default("UI_DIR", "apps/alloy")
SYNAPSE_COMPOSE := "tests/synapse/docker-compose.yml"

default:
    @just --list

# Format all Rust crates in the workspace
fmt:
    cargo fmt --all

# Run Rust formatting check and Clippy with zero warnings allowed
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings

# Run all cargo unit and integration tests across the workspace
test:
    cargo test --workspace

# Run license, advisory, bans, and source checks using cargo-deny
deny:
    cargo deny check

# Run vulnerability audit using cargo-audit
audit:
    cargo audit

# Type-check frontend UI via SvelteKit / svelte-check
ui-check:
    pnpm -C {{UI_DIR}} install
    pnpm -C {{UI_DIR}} check

# Build frontend UI production assets via Vite
ui-build:
    pnpm -C {{UI_DIR}} build

# Start local Synapse test homeserver in Docker
synapse-up:
    docker compose -f {{SYNAPSE_COMPOSE}} up -d

# Stop local Synapse test homeserver in Docker
synapse-down:
    docker compose -f {{SYNAPSE_COMPOSE}} down

# Run complete local CI validation suite
ci:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings
    cargo test --workspace
    cargo deny check
    cargo audit
    pnpm -C {{UI_DIR}} install
    pnpm -C {{UI_DIR}} check
    pnpm -C {{UI_DIR}} build

# Enable repo git hooks (Conventional Commits check on commit-msg and pre-push)
hooks:
    git config core.hooksPath .githooks
