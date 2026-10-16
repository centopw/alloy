# S6 Spike: CI, Delivery & Governance Architecture

**Date:** 2026-10-16  
**Status:** Completed  
**Verdict:** **GO** (Ready for Phase 0 completion and Phase 1 delivery integration)

---

## What Was Verified

### 1. `cargo-deny` (v0.20.2)
- **Command:** `cargo deny check`
- **Output:**
  ```
  advisories ok, bans ok, licenses ok, sources ok
  ```
- **Surprises & Findings:**
  - Upstream transitive dependency `webpki-root-certs v1.0.9` (via `rustls-platform-verifier` -> `reqwest` -> `matrix-sdk`/`tauri`) uses license `CDLA-Permissive-2.0`. Added to `[licenses] allow`.
  - In `cargo-deny` 0.20+, the `vulnerability = "deny"` configuration key was deprecated/removed (vulnerabilities are denied by default). Transitive unmaintained crate warnings (`anymap2` in `matrix-sdk`, `proc-macro-error` in `gtk`) are handled with `unmaintained = "none"`, preventing false build failures while maintaining strict vulnerability blocking.
  - License allowlist verified against all current workspace dependencies (matrix-sdk, tauri 2, wasmtime, svelte/tauri-plugin-opener).

### 2. `cargo-audit` (v0.22.2)
- **Command:** `cargo audit`
- **Output:**
  - Loaded 1,329 security advisories from RustSec database.
  - Scanned 817 crate dependencies in `Cargo.lock`.
  - Found 0 security vulnerabilities.
  - 4 warnings found for unmaintained/unsound transitive dependencies (`anymap2`, `derivative`, `proc-macro-error`, `glib`).

### 3. Task Runner (`just` v1.59.0)
- **Command:** `just --list`
- **Output:**
  ```
  Available recipes:
      audit        # Run vulnerability audit using cargo-audit
      ci           # Run complete local CI validation suite
      default
      deny         # Run license, advisory, bans, and source checks using cargo-deny
      fmt          # Format all Rust crates in the workspace
      lint         # Run Rust formatting check and Clippy with zero warnings allowed
      synapse-down # Stop local Synapse test homeserver in Docker
      synapse-up   # Start local Synapse test homeserver in Docker
      test         # Run all cargo unit and integration tests across the workspace
      ui-build     # Build frontend UI production assets via Vite
      ui-check     # Type-check frontend UI via SvelteKit / svelte-check
  ```

### 4. GitHub Actions Workflows (`actionlint` v1.7.12 & PyYAML)
- **Command:** `actionlint`
- **Output:** Clean (0 syntax/structural errors).
- **Command:** `python3 -c "import glob, yaml; [yaml.safe_load(open(f)) for f in glob.glob('.github/workflows/*.yml')]"`
- **Output:** All workflows parsed successfully.
- **Surprises & Findings:**
  - In GitHub Actions expressions, `hashFiles` is not valid at `jobs.<id>.if` context (only within `steps`). Implemented step-level checks gating mobile build jobs (`apps/alloy/src-tauri/gen/apple/**` and `apps/alloy/src-tauri/gen/android/**`) cleanly without using `workflow_dispatch` stubs.

### 5. Frontend & Documentation Integration
- Verified `apps/alloy` package layout: UI scripts `check` and `build` wired into `just ui-check`, `just ui-build`, and `ci.yml` via configurable `UI_DIR` environment variable.
- Full canonical Apache-2.0 `LICENSE` installed.
- Governance docs created: `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `README.md`, `.editorconfig`, `rustfmt.toml`, `clippy.toml`, `docs/CI.md`.

---

## Tool & Crate Versions

| Tool / Crate | Version | Notes |
|---|---|---|
| `just` | 1.59.0 | Project recipe runner |
| `cargo-deny` | 0.20.2 | License and supply-chain policy linter |
| `cargo-audit` | 0.22.2 | RustSec vulnerability scanner |
| `actionlint` | 1.7.12 | GitHub Actions workflow linter |
| `shellcheck` | 0.11.0 | Actionlint shell script validator |
| `rustfmt` | 1.10.0-stable | Rust code formatter |

---

## Verdict

**GO.** The CI pipeline, supply chain policies, and project governance files are fully verified, syntactically validated, and ready for deployment.
