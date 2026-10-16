# Contributing to Alloy

Thank you for your interest in contributing to Alloy! Alloy is an open-source, cross-platform Matrix client built with a Rust core, Tauri 2 desktop shell, and Svelte 5 frontend.

Please read this document before submitting issues or pull requests.

---

## Code of Conduct

All contributors are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behavior through private GitHub Security Advisories or to the project maintainers.

---

## Development Setup

### Prerequisites

Ensure you have the following installed on your machine:

1. **Rust:** Latest stable toolchain (1.90+ required, as specified in `rust-toolchain.toml`).
   ```bash
   rustup default stable
   rustup component add rustfmt clippy
   ```
2. **Node.js & pnpm:** Node.js 22+ and pnpm 9+.
   ```bash
   corepack enable
   corepack prepare pnpm@latest --activate
   ```
3. **Just:** Command runner used for project recipes.
   ```bash
   # macOS
   brew install just
   # Linux / cargo
   cargo install just
   ```
4. **Docker:** Required for running the local Synapse homeserver test instance.
5. **Security Tools:**
   ```bash
   cargo install cargo-deny --locked
   cargo install cargo-audit --locked
   ```

### Quickstart

1. **Clone the repository:**
   ```bash
   git clone https://github.com/centopw/alloy.git
   cd alloy
   ```

2. **Install frontend dependencies:**
   ```bash
   just ui-check
   ```

3. **Start local Synapse test instance (optional, for matrix-sdk testing):**
   ```bash
   just synapse-up
   ```

4. **Verify your environment with the CI recipe:**
   ```bash
   just ci
   ```

---

## Git & PR Conventions

### Branch Naming

Use descriptive branch names with appropriate prefixes:
- `feature/<short-description>`: New features
- `fix/<short-description>`: Bug fixes
- `docs/<short-description>`: Documentation changes
- `refactor/<short-description>`: Code refactoring without behavior changes
- `ci/<short-description>`: CI/CD configuration updates

### Branch policy (pre-v0.1)

Until the first functioning version ships, maintainers push directly to `main`. Rules still apply:
- One logical change per commit; history stays linear (rebase, no merge commits).
- Every commit subject follows Conventional Commits, enforced by `scripts/check-commit-msg.sh` (local hooks via `just hooks`, and the `commits` CI job).
- Subject: lowercase imperative, <= 72 chars, no trailing period. Never commit failing builds; push after each green logical unit.
- After the first functioning version, `main` becomes PR-only.

### Conventional Commits

Alloy uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for clear change logs:

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

Common types:
- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code (formatting, missing semi-colons, etc.)
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `perf`: A code change that improves performance
- `test`: Adding missing tests or correcting existing tests
- `chore`: Changes to build process, dependency tools, or libraries

Examples:
- `feat(protocol): add room message event DTO`
- `fix(storage): resolve keychain key retrieval race`
- `docs(ci): update release secrets table`

---

## Testing & Quality Expectations

Before submitting a Pull Request, verify that all local checks pass:

1. **Format check:** `just fmt`
2. **Linter check:** `just lint` (runs `cargo fmt --check` and `cargo clippy -D warnings --workspace`)
3. **Tests:** `just test` (runs all cargo unit/integration tests)
4. **Dependency policy:** `just deny` (runs `cargo deny check`)
5. **Vulnerability audit:** `just audit` (runs `cargo audit`)
6. **UI check & build:** `just ui-check` and `just ui-build`
7. **Full pre-flight run:**
   ```bash
   just ci
   ```

### Rules & Architectural Guidelines

- **Forbid unsafe code:** `#![forbid(unsafe_code)]` is enforced across all Rust crates. Platform-specific FFI wrappers requiring `unsafe` must be strictly isolated, reviewed, and justified.
- **Apache-2.0 License:** All files are licensed under Apache-2.0. Do not introduce dependencies incompatible with Apache-2.0 (verified by `cargo-deny`).
- **Zero telemetry by default:** No tracking, metrics, or telemetry code may collect user communications or metadata without explicit opt-in.

---

## Reporting Issues

- **Security vulnerabilities:** Do **NOT** open public issues. Follow [SECURITY.md](SECURITY.md) to report privately via GitHub Security Advisories.
- **Bug reports & Feature requests:** Use GitHub Issues, detailing steps to reproduce, environment info, and expected behavior.
