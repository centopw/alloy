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

### In Scope
- Matrix protocol implementation (matrix-sdk + alloy-core)
- Cryptographic key handling (keychains, zeroization)
- Local data encryption (SQLite via matrix-sdk-sqlite)
- Plugin sandbox enforcement (Wasmtime capability model, WASM fuel)
- Supply chain (dependency versions, license compliance)
- Update mechanism (code signing, delta verification)
- Network security (TLS, certificate pinning—future)

### Out of Scope (Homeserver Responsibility)
- Homeserver authentication bugs or account compromises
- Homeserver-side encryption/decryption
- Network eavesdropping (use a trusted network or VPN)
- Social engineering attacks against users

## Security Baseline

Alloy enforces the following architectural constraints per [docs/plan/04-security-delivery.md](docs/plan/04-security-delivery.md):

1. **No unsafe Rust code** at workspace level (`#![forbid(unsafe_code)]`), except
   platform-specific FFI in thin, audited wrapper crates.
2. **Zero telemetry by default.** Opt-in analytics only via explicit user consent;
   no collection of message content, contact lists, or encryption keys.
3. **Mandatory supply-chain checks:**
   - `cargo-audit` blocks known CVEs in CI
   - `cargo-deny` enforces license and version policies
   - SBOM generated per release
4. **No downloading unsigned code** (plugins require Minisign/Ed25519 signature;
   marketplace enforces signed-only delivery).
5. **Platform keychains** for secrets at rest (macOS/Linux/Windows Credential Manager;
   iOS Keychain; Android Keystore).
6. **Memory-safe secret handling** (zeroize + secrecy crates for crypto material).

## Plugin Sandbox

Plugins run in **Wasmtime with capabilities-based security:**
- **Fuel**: 500M units per plugin (prevents infinite loops).
- **Memory**: 32 MB cap per plugin.
- **Declarations**: Each plugin declares required capabilities (e.g., "read_room_list",
  "fetch_url"); user grants/denies at install.
- **iOS**: No WASM plugins in v1 (Tier-A JSON packages only); see D7.
- **Disabled on iOS builds** per product decision; Android enables full Tier-B.

## Code Signing & Notarization

### macOS & iOS
- Code signed with Apple Developer ID certificate
- Notarized via Apple's servers (via Tauri + xcrun notarytool)
- App Store submission requires notarization

### Windows
- Authenticode-signed EXE/MSI with DigiCert or similar
- MSIX optional for Microsoft Store; direct download uses signed manifests

### Linux
- GPG-signed tarballs or Flatpak/AppImage bundles
- Flathub provides AppImage signing

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
- [ ] Local storage encryption (SQLCipher investigation ongoing—see PLAN.md D2 conflict note)
- [ ] AppAttest (iOS) / SafetyNet (Android) pinning (post-v1)
- [ ] Formal security audit (planned pre-App Store/Play launch)

## Contact

- **Security Issues:** Private GitHub Advisory (preferred) or hiep@hce.vn
- **General Questions:** open a discussion in this repository
- **Maintainers:** see CONTRIBUTING.md
