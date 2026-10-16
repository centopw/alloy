# Alloy

**A modern, cross-platform Matrix chat client.**

> **Status:** Pre-alpha (planning & foundations)

Alloy is a privacy-first, extensible Matrix client written in Rust with a Tauri 2 desktop shell and a Svelte 5 web UI. It emphasizes encryption, customization, and deep user control through JSON packages and sandboxed WASM plugins.

## Features (Planned)

- **Desktop & Mobile:** Native apps for macOS, Windows, Linux, iOS, and Android
- **End-to-End Encryption:** E2EE via vodozemac; message verification & backup
- **Deep Customization:**
  - Tier-A: JSON packages (themes, keybindings, slash commands)
  - Tier-B: Sandboxed WASM plugins (message renderers, custom panels, commands)
  - Tier-C: Community marketplace with signed plugins & themes
- **Sliding Sync:** Efficient room list and timeline synchronization (MSC3575, with MSC4186 proxy fallback)
- **Modern UI:** Virtualized timeline, adaptive shell, light/dark/AMOLED themes
- **Media Handling:** Authenticated downloads, thumbnail generation
- **Privacy:** Zero telemetry by default; opt-in analytics only with consent

## Status

**P0 — Foundations** (active)
- [x] Workspace bootstrap
- [ ] Core: matrix-sdk login + room list + timeline (in progress)
- [ ] UI: Type generation + virtualized timeline (in progress)
- [ ] Storage: keychain + encrypted database (planned)
- [ ] Tauri mobile perf gate (planned)
- [ ] CI skeleton with security checks (planned)

See [docs/PLAN.md](docs/PLAN.md) for the full roadmap and architecture details.

## Quick Start

### Prerequisites

- **Rust:** 1.90+ (with rustfmt and clippy)
- **Node.js & pnpm:** 22+ and 9+ respectively
- **Just:** Command runner (see setup below)
- **Docker:** Optional, for local Synapse testing

### Setup

1. **Clone & enter the repo:**
   ```bash
   git clone https://github.com/centopw/alloy.git
   cd alloy
   ```

2. **Install tools:**
   ```bash
   # macOS
   brew install just cargo-deny
   # Or install globally via cargo
   cargo install just cargo-deny --locked
   ```

3. **Install dependencies & run checks:**
   ```bash
   just ui-check
   ```

4. **Start a local Synapse test homeserver (optional):**
   ```bash
   just synapse-up
   # Run against it, then
   just synapse-down
   ```

5. **Run the full CI pipeline locally:**
   ```bash
   just ci
   ```

This runs:
- `cargo fmt --check` (Rust formatting)
- `cargo clippy -D warnings --workspace` (Linting)
- `cargo test` (Unit & integration tests)
- `cargo deny check` (Dependency & license audit)
- `cargo audit` (Vulnerability check)
- `pnpm install && pnpm check && pnpm build` (UI type-check & build)

## Architecture

```
┌─────────────────────────────┐
│  apps/alloy (Tauri 2 shell) │
│  Svelte 5 + TypeScript UI   │
│  • Adaptive shell/timeline  │
│  • Token engine → CSS vars  │
│  • 12 extension slots       │
└──────────────┬──────────────┘
       ⬌ IPC (Tauri / WASM)
┌──────────────────────────────────┐
│  Rust Core (crates/*) ────────┐  │
│  • alloy-protocol: DTOs       │  │
│  • alloy-core: matrix-sdk-ui  │  │
│  • alloy-storage: keychain+DB │  │
│  • alloy-plugin: WASM host    │  │
│  • alloy-theme: tokens        │  │
│  • alloy-ffi: NSE/FCM         │  │
└──────────────────────────────────┘
       ⬌ sliding sync, E2EE, media
   ┌───────────────────────────┐
   │  Matrix Homeserver        │
   └───────────────────────────┘
```

For details, see [docs/PLAN.md](docs/PLAN.md) and the architecture docs in [docs/plan/](docs/plan/).

## Project Recipes

```bash
just --list              # Show all recipes
just fmt                 # Format Rust code
just lint                # Lint (fmt --check + clippy)
just test                # Run all tests
just deny                # Check licenses & banned deps
just audit               # Check for known vulnerabilities
just ui-check            # Type-check UI
just ui-build            # Build UI bundles
just synapse-up          # Start local Synapse in Docker
just synapse-down        # Stop Synapse
just ci                  # Full CI pipeline (all checks)
```

## Contributing

We welcome contributions! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development setup
- Git & PR conventions (Conventional Commits)
- Code style & quality expectations
- Testing requirements
- Security vulnerability reporting

## Code of Conduct

By participating, you agree to abide by the [Code of Conduct](CODE_OF_CONDUCT.md), which adopts the [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct.html).

## License

Alloy is licensed under the **Apache License 2.0**. See [LICENSE](LICENSE) for the full text.

## Security

For security vulnerability reports, please follow [SECURITY.md](SECURITY.md) and use **GitHub Security Advisories** (preferred) or email hiep@hce.vn with subject `[SECURITY]`.

---

**More documentation:**
- [Master Plan](docs/PLAN.md)
- [UI Stack](docs/plan/01-ui-stack.md)
- [Core Architecture](docs/plan/02-core-architecture.md)
- [Extensibility](docs/plan/03-extensibility.md)
- [Security & Delivery](docs/plan/04-security-delivery.md)
- [UX Design](docs/plan/05-ux-design.md)
- [CI Pipeline](docs/CI.md)
