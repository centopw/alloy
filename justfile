# Alloy task runner recipes (justfile)
# See CONTRIBUTING.md and docs/CI.md for documentation.

set shell := ["bash", "-cu"]

# UI workspace directory (apps/alloy by default; apps/alloy/ui configurable)
UI_DIR := env_var_or_default("UI_DIR", "apps/alloy/ui")
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

# Generate changelog from Conventional Commits via git-cliff
changelog *args:
    @command -v git-cliff >/dev/null 2>&1 || (echo "Error: git-cliff is not installed. Install via: brew install git-cliff or cargo install git-cliff" && exit 1)
    git-cliff {{args}}

# Build unsigned desktop bundle locally
nightly-local:
    pnpm -C {{UI_DIR}} install
    pnpm -C {{UI_DIR}} build
    pnpm -C apps/alloy tauri build

# Build Android debug APK (requires JAVA_HOME (JDK 17), ANDROID_HOME, and target e.g. aarch64)
android-debug target="aarch64":
    @test -n "$$JAVA_HOME" || (echo "Error: JAVA_HOME is not set. Android build requires JDK 17." && exit 1)
    @test -n "$$ANDROID_HOME" || (echo "Error: ANDROID_HOME is not set. Android build requires Android SDK/NDK." && exit 1)
    rustup target add {{target}}-linux-android 2>/dev/null || true
    pnpm -C {{UI_DIR}} install
    pnpm -C {{UI_DIR}} build
    pnpm -C apps/alloy tauri android build --apk --debug --target {{target}}

# Run timeline scroll performance benchmark script via headless Chrome
perf-ui:
    pnpm -C apps/alloy/ui build
    node apps/alloy/ui/scripts/measure-scroll-perf.mjs

# Measure cold start and idle RSS of the release desktop app on macOS (method: docs/plan/08-delivery-p1.md section 2.1)
perf-desktop binary="target/release/alloy-app" runs="5":
    node scripts/perf-desktop.mjs {{binary}} --runs {{runs}}
