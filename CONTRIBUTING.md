# Contributing to Alloy

Thank you for your interest in contributing to Alloy! Alloy is an open-source, cross-platform client for personal Telegram and Facebook Messenger accounts, connected directly from the device, built with a Rust core, Tauri 2 desktop shell, and Svelte 5 frontend. The architecture is recorded in [ADR 0016](docs/decisions/0016-on-device-connectors.md).

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
4. **Go:** the version in `sidecars/alloy-meta/go.mod` (1.26+). It builds the Messenger sidecar, which every app build bundles.
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

3. **Messenger sidecar:** nothing to do on a machine with Go. `apps/alloy/src-tauri/build.rs`
   builds `apps/alloy/src-tauri/binaries/alloy-meta-<triple>` on demand the first time `alloy-app`
   is built, and again only when the sidecar sources change; without Go the build stops with one
   message naming both ways forward (install Go 1.26+, or run `just sidecar` elsewhere and copy the
   binary in). To force a rebuild, or to produce the binary without cargo:
   ```bash
   just sidecar         # host triple; `just sidecar-android` for the arm64 Android library
   ```
   CI still builds the sidecar explicitly before clippy and tests, so a Go-less runner never gets
   that far.

4. **Verify your environment with the CI recipe:**
   ```bash
   just ci
   ```

### Telegram application credentials

`api_id`/`api_hash` identify Alloy the *application*, the same role they play in a mautrix bridge
config. They are a runtime setting, not a build input: no credential is compiled into a release, so
an installed binary cannot be mined for one, and the pair can be replaced without shipping a new
version.

- **In the app:** Settings -> Networks -> Telegram application. The pair is sealed in the OS
  keyring under `alloy.telegram.app.v1` and reused from then on; the `api_hash` is never returned
  or logged, and `GetTelegramApp` reports only the `api_id` and where the pair came from.
- **Registering one yourself:** <https://my.telegram.org/apps>. Telegram's API terms ask any client
  that is not the official app to carry a name prefixed "Unofficial", so the registered entry is
  titled "Unofficial Alloy"; if that prefix is not acceptable, the fix is to rename the client
  itself (display name, bundle name and docs) so the two titles agree — the title is what the terms
  constrain, not the phone-number prompt.
- **Headless development (no UI):** export `ALLOY_TELEGRAM_API_ID` and `ALLOY_TELEGRAM_API_HASH`
  before running the engine. This is an override only: the sealed runtime pair wins when both are
  present, and `GetTelegramApp` reports the environment as its source.
- **Without a pair:** the build still succeeds and `ListNetworks` lists Telegram with
  `ready: false` plus a setup block naming the two fields and the registration URL; `BeginLogin`
  then fails with an actionable `not_supported`. `just telegram-app` prints this recipe.

### Network settings

Every network-specific knob is a runtime setting, never a build input. No endpoint, proxy,
platform selector or default value is compiled into the engine or the UI: a network declares what
it accepts in `crates/alloy-engine/src/factory.rs` (`network_setting_specs`), `ListNetworks`
reports that declaration with the value in effect, and `SetNetworkSetting` /
`ClearNetworkSetting` change it. Messenger's `proxy` and `platform` are the first non-Telegram
examples.

- **Adding a knob** means adding a `NetworkSettingSpec`: no protocol change, no new command, no
  caller to update. Adding a network means adding its declaration; a setting the declaration does
  not list is refused by the engine.
- **Storage:** non-secret values live in `<data_dir>/network_settings.json`, written atomically
  (temporary file, fsync, rename). A `secret` setting is sealed in the OS keyring and never
  written to that file or returned by `ListNetworks`.
- **Delivery:** the value in effect is read when the next connector is created, so a Messenger
  sidecar is launched with `ALLOY_META_PROXY` (omitted when unset) and `ALLOY_META_PLATFORM`. A
  running account keeps the settings it started with; changing one does not restart it.
- **Never** hard-code a network's endpoint, proxy, credential or client platform in Rust, the UI or
  a build script. If a value belongs to the network rather than to the person, it is still a
  declaration with a default, not a literal in a call site.

---

## Git & PR Conventions

### Branch Naming

Use descriptive branch names with appropriate prefixes:
- `feature/<short-description>`: New features
- `fix/<short-description>`: Bug fixes
- `docs/<short-description>`: Documentation changes
- `refactor/<short-description>`: Code refactoring without behavior changes
- `ci/<short-description>`: CI/CD configuration updates

### Tracking
Every change maps to an issue on the project board; see [docs/TRACKING.md](docs/TRACKING.md). `feat|fix|perf|refactor` commits must contain `Refs #N` or `Closes #N`.

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
6. **Connector tests:** `cargo test -p alloy-connector --features testing`, `cargo test -p alloy-engine --features testing`, `cargo test -p alloy-sidecar --features fake-sidecar`
7. **Sidecar checks:** `( cd sidecars/alloy-meta && go vet ./... && go test ./... )`
8. **UI check & build:** `just ui-check` and `just ui-build`
9. **Full pre-flight run:**
   ```bash
   just ci
   ```

### Rules & Architectural Guidelines

- **Forbid unsafe code:** `unsafe_code = "forbid"` is set for the whole workspace in `Cargo.toml`. There is no exception crate.
- **Apache-2.0 License:** Alloy's Rust and UI code is Apache-2.0. Do not introduce dependencies incompatible with Apache-2.0 (verified by `cargo-deny`). The Messenger sidecar `sidecars/alloy-meta` is AGPL-3.0 because it builds on mautrix-meta; it ships as a separate executable speaking NDJSON over stdio and must never be linked into the Rust code.
- **Local data:** messages and accounts live in the local SQLite store `crates/alloy-store`. Its migrations are append-only and checksummed: never edit an applied migration, add a new one.
- **Zero telemetry by default:** No tracking, metrics, or telemetry code may collect user communications or metadata without explicit opt-in.

---

## Reporting Issues

- **Security vulnerabilities:** Do **NOT** open public issues. Follow [SECURITY.md](SECURITY.md) to report privately via GitHub Security Advisories.
- **Bug reports & Feature requests:** Use GitHub Issues, detailing steps to reproduce, environment info, and expected behavior.
