# Alloy Releasing & Distribution Guide

This document outlines the release process, versioning policies, distribution channels, code signing configurations, and verification procedures for Alloy.

---

## 1. Versioning Strategy

Alloy follows [Semantic Versioning (SemVer 2.0.0)](https://semver.org/) and [Conventional Commits](https://www.conventionalcommits.org/):

- **Pre-1.0 (Phase P1–P4):** `0.MINOR.PATCH`
  - `0.MINOR`: Incremented on significant feature milestones, breaking API changes, or protocol revisions.
  - `PATCH`: Incremented on bug fixes, performance optimizations, and backward-compatible improvements.
- **Post-1.0:** Strict `MAJOR.MINOR.PATCH` SemVer guarantees.

### Changelog Generation via `git-cliff`

Alloy uses [`git-cliff`](https://git-cliff.org/) configured via `cliff.toml` to generate structured changelogs directly from Conventional Commits.

#### Installation
```bash
# macOS (Homebrew)
brew install git-cliff

# Cargo (any platform)
cargo install git-cliff
```

#### Usage via `just`
```bash
# Preview unreleased changes since last tag
just changelog --unreleased

# Generate or update CHANGELOG.md for a release
just changelog -o CHANGELOG.md

# Prepend changes for a new release version
just changelog --tag v0.1.0 -o CHANGELOG.md
```

---

## 2. Release Channels

| Channel | Trigger | Signing | Target Artifacts | Destination |
|---|---|---|---|---|
| **Nightly** | Daily at 02:00 UTC or `workflow_dispatch` | Unsigned desktop; Debug APK | macOS (arm64 + x64), Windows x64, Linux (AppImage + deb), Android debug APK | Rolling GitHub Prerelease (`nightly`) |
| **Stable** | Git tag push `v*` (e.g. `v0.1.0`) | Signed & Notarized | macOS DMG/App (notarized), Windows NSIS/MSI, Linux deb/AppImage, Signed Android APK | GitHub Releases (Draft -> Published) |

### Nightly Version Metadata
Nightly builds append commit metadata to the base version per SemVer 2.0.0:
```
0.1.0-nightly+<commit>.<describe>
# Example: 0.1.0-nightly+75ddfff.75ddfff
```
This is injected into `apps/alloy/src-tauri/tauri.conf.json` by `scripts/embed-nightly-version.mjs` before packaging.

---

## 3. Tauri Updater & Signature Keys

Alloy uses Tauri's built-in updater plugin with minisign cryptography.

### Key Pair Generation
To generate a new updater signing key pair:
```bash
pnpm tauri signer generate -w ~/.tauri/alloy.key
```
This command outputs:
1. **Public Key:** Base64 minisign public key string.
2. **Private Key:** Saved to `~/.tauri/alloy.key` (or output path).

### Public Key Configuration
*(Note: Documented here for reference. Do not manually modify during frozen contract phases).*
The public key is configured in `apps/alloy/src-tauri/tauri.conf.json` under `plugins.updater`:
```json
{
  "plugins": {
    "updater": {
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDYwOEU0RjY5NzU1MjcxRUUKUl...<PUBKEY>...",
      "endpoints": [
        "https://github.com/alloy-chat/alloy/releases/latest/download/latest.json"
      ]
    }
  }
}
```

### Private Key Deployment
The private key and its passphrase are added to GitHub Repository Secrets:
- `TAURI_SIGNING_PRIVATE_KEY` (content of `alloy.key`)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (passphrase set during generation)

---

## 4. Required CI/CD Secrets

The following GitHub Actions secrets must be configured in repository settings:

### Desktop Code Signing & Notarization
| Secret | Description | Target Platform |
|---|---|---|
| `APPLE_CERTIFICATE` | Base64-encoded Developer ID Application `.p12` certificate | macOS |
| `APPLE_CERTIFICATE_PASSWORD` | Passphrase for the `.p12` file | macOS |
| `APPLE_SIGNING_IDENTITY` | Identity string (e.g., `Developer ID Application: Name (TEAMID)`) | macOS |
| `APPLE_ID` | Apple ID account email used for notarization | macOS |
| `APPLE_PASSWORD` | App-specific password generated on appleid.apple.com | macOS |
| `APPLE_TEAM_ID` | 10-character Apple Developer Team ID | macOS |
| `WINDOWS_CERTIFICATE` | Base64-encoded Authenticode code-signing certificate (`.pfx`) | Windows |
| `WINDOWS_CERTIFICATE_PASSWORD` | Passphrase for the Windows `.pfx` certificate | Windows |
| `TAURI_SIGNING_PRIVATE_KEY` | Minisign private key for Tauri updater manifests | Desktop (all) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`| Passphrase for the updater private key | Desktop (all) |

### Android Release Signing
| Secret | Description | Target Platform |
|---|---|---|
| `ANDROID_KEYSTORE_BASE64` | Base64-encoded release `.keystore` / `.jks` file | Android |
| `ANDROID_KEYSTORE_PASSWORD` | Access password for the keystore file | Android |
| `ANDROID_KEY_ALIAS` | Key alias name inside the keystore | Android |
| `ANDROID_KEY_PASSWORD` | Password for the key alias | Android |

---

## 5. Android Keystore Creation & Management

### Creating a Production Keystore
Generate a dedicated PKCS12 keystore using Java's `keytool`:
```bash
keytool -genkey -v \
  -keystore alloy-release.keystore \
  -alias alloy \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -storetype PKCS12
```

### Encoding for GitHub Secrets
Convert the keystore file to base64 for the `ANDROID_KEYSTORE_BASE64` secret:
```bash
# macOS
base64 -i alloy-release.keystore | tr -d '\n' | pbcopy

# Linux
base64 -w 0 alloy-release.keystore
```

---

## 6. Android Sideloading Instructions

Users installing Alloy on Android outside Google Play (via GitHub Releases or Nightlies) must sideload the APK.

### Prerequisites
1. Enable **"Install Unknown Apps"** in Android Settings for your browser or file manager (`Settings` → `Apps` → `Special App Access` → `Install Unknown Apps`).
2. Download `alloy-android.apk` (or `alloy-android-nightly.apk`) and corresponding `SHA256SUMS`.

### Installation via ADB (Developers)
```bash
# Connect device or launch emulator
adb devices

# Install APK (replace existing without losing app data)
adb install -r alloy-android.apk
```

> **Note on Signature Conflicts:**  
> Android forbids installing a release-signed APK over a debug-signed build (or vice-versa) with error `INSTALL_FAILED_UPDATE_INCOMPATIBLE`. To switch build types:
> ```bash
> adb uninstall chat.alloy
> adb install alloy-android.apk
> ```

---

## 7. Checksum & Integrity Verification

Every release publishes `SHA256SUMS` alongside the binary assets. Users and package managers should verify asset integrity prior to installation.

### Linux
```bash
sha256sum -c SHA256SUMS --ignore-missing
```

### macOS
```bash
shasum -a 256 -c SHA256SUMS
```

### Windows (PowerShell)
```powershell
Get-FileHash -Algorithm SHA256 .\Alloy_*_x64-setup.exe
# Compare hash with value listed in SHA256SUMS
```

---

## 8. Release Execution Checklist

When cutting a new stable release (e.g. `v0.1.0`):

1. **Verify Local Quality Gates:**
   ```bash
   just ci
   just test
   just audit
   ```
2. **Review & Update Changelog:**
   ```bash
   just changelog --tag v0.1.0 -o CHANGELOG.md
   ```
3. **Verify Version Alignment:**
   - Ensure version in `Cargo.toml`, `apps/alloy/src-tauri/tauri.conf.json`, and `apps/alloy/ui/package.json` match.
4. **Tag and Push:**
   ```bash
   git tag -s v0.1.0 -m "chore(release): v0.1.0"
   git push origin v0.1.0
   ```
5. **Monitor CI Workflow:**
   - Watch `.github/workflows/release.yml` run to completion.
   - Verify all desktop bundles (macOS DMG, Windows installer, Linux AppImage/deb) and signed Android APK attach to the release draft.
   - Inspect SBOM (`alloy-sbom-*.json`) and `SHA256SUMS`.
6. **Publish Draft Release:**
   - Review release body and publish on GitHub.
