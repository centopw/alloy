# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Alloy, please report it responsibly by
opening a **private GitHub Security Advisory** in this repository rather than creating
a public issue. GitHub advisories allow us to coordinate a fix before disclosure.

1. Navigate to **Security > Advisories > Report a vulnerability**
2. Include:
   - Affected component/version
   - Steps to reproduce (if possible)
   - Potential impact
   - Suggested fix (optional)

Alternatively, you may reach out to **hiep@hce.vn** with the subject line
**[SECURITY]** (email will be confirmed for security mail routing as project evolves).

**Response SLA:** We aim to acknowledge receipt within 48 hours and provide a status
update within 5 business days.

## Supported Versions

| Version | Status | Support Ends |
|---------|--------|--------------|
| **1.0+** | LTS (planned post-release) | TBD |
| **0.2–0.5** | Active development | End of phase |
| **< 0.2** | Pre-alpha (in development) | N/A |

**Security updates** will be issued for all supported versions. Older pre-alpha
versions will not receive backported fixes; upgrade to the latest main branch.

## Security Scope

### Architecture and threat model

Alloy connects personal Telegram and Facebook Messenger accounts directly from the device
([ADR 0016](docs/decisions/0016-on-device-connectors.md)). There is no Alloy server and no
intermediate homeserver or bridge:

- **Transport.** Telegram traffic uses MTProto's transport encryption (grammers, in-process).
  Messenger traffic uses TLS to Meta's web endpoints, made by the separate Go sidecar
  `sidecars/alloy-meta`, which the app supervises and talks to over NDJSON on stdio.
- **Local data.** Accounts, chats and messages live in a local SQLite database (`alloy-store`) in
  the app data directory. Connector secrets (session bytes, tokens, cookies) are sealed with
  XChaCha20-Poly1305, bound to the account id, under a key held by `alloy-storage`'s `SecretStore`
  (the OS keyring on desktop, an owner-only file on Android). The message database itself is not
  yet encrypted; full-database encryption is the M5 hardening decision (#134).
- **End-to-end encryption.** Alloy adds no E2EE layer of its own. Messenger's end-to-end
  encrypted chats are handled by the sidecar; Telegram cloud chats are not end-to-end encrypted by
  the network.

### In Scope
- Connector implementations (`alloy-telegram`, `alloy-sidecar`, `sidecars/alloy-meta`) and the
  NDJSON boundary between the app and the sidecar
- Local storage (`alloy-store`): secret sealing, migrations, data left behind after logout
- Cryptographic key handling (keychains, zeroization)
- Plugin sandbox enforcement (Wasmtime capability model, WASM fuel)
- Supply chain (Rust and Go dependency versions, license compliance)
- Update mechanism (code signing, delta verification)
- Network security (TLS, certificate pinning—future)

### Out of Scope (Network Responsibility)
- Account restrictions or locks imposed by Telegram or Meta on unofficial clients (a documented,
  accepted risk; see the README)
- Server-side bugs, account compromises or encryption at Telegram or Meta
- Malware or another user with access to an unlocked device and its app data directory
- Social engineering attacks against users

## Security Baseline

Alloy enforces the following architectural constraints per [docs/plan/04-security-delivery.md](docs/plan/04-security-delivery.md):

1. **No unsafe Rust code**: `unsafe_code = "forbid"` for the whole workspace, with no exception
   crate.
2. **Zero telemetry by default.** Opt-in analytics only via explicit user consent;
   no collection of message content, contact lists, or encryption keys.
3. **Mandatory supply-chain checks:**
   - `cargo-audit` blocks known CVEs in CI
   - `cargo-deny` enforces license and version policies
   - SBOM generated per release
4. **No downloading unsigned code** (plugins require Minisign/Ed25519 signature;
   marketplace enforces signed-only delivery).
5. **Platform keychains** for secrets at rest (macOS Keychain, Windows Credential Manager, Linux
   Secret Service; an owner-only file on Android).
6. **Memory-safe secret handling** (zeroize + secrecy crates for crypto material).

## Plugin Sandbox

Plugins run in **Wasmtime with capabilities-based security:**
- **Fuel**: 500M units per plugin (prevents infinite loops).
- **Memory**: 32 MB cap per plugin.
- **Declarations**: Each plugin declares required capabilities (e.g., "read_room_list",
  "fetch_url"); user grants/denies at install.
- **Android:** Full Tier-B WASM plugin support enabled.

## Code Signing & Notarization

### macOS (Desktop)
- Code signed with Apple Developer ID certificate for direct distribution
- Notarized via Apple's servers (via Tauri + xcrun notarytool) for gatekeeper bypass

### Windows
- Authenticode-signed EXE/MSI with DigiCert or similar for direct distribution

### Linux (Desktop)
- GPG-signed tarballs or AppImage bundles for direct distribution
- Flatpak/F-Droid submission planned (post-v1)

### All Platforms
- **Tauri updater** with Ed25519-signed manifests (no Sparkle outside macOS)
- Delta compression enabled
- Release builds only on git tags

## CI/CD Security

- **GitHub Actions secrets** store keys (`APPLE_CERTIFICATE_BASE64`, `WINDOWS_CERT_PASSWORD`, etc.)
- **No secrets in PR logs** (output always masked)
- **Pinned action versions** (by major tag minimum)
- **Code review gate** before merging main
- **Artifact attestation** (future: per CycloneDX SBOM)

## Known Limitations & Future Work

- [ ] Device certificate pinning (post-MVP)
- [ ] Local message database encryption (SQLCipher decision in M5, #134)
- [ ] SafetyNet (Android) pinning (post-v1)
- [ ] Formal security audit (planned post-MVP before expanding distribution channels)

## Contact

- **Security Issues:** Private GitHub Advisory (preferred) or hiep@hce.vn
- **General Questions:** open a discussion in this repository
- **Maintainers:** see CONTRIBUTING.md
