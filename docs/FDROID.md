# Publishing Alloy on F-Droid

Status: **paused; no F-Droid release is planned for now.** The store metadata and a draft build recipe stay in the
repository for a later submission.

[Merge request !53450](https://gitlab.com/fdroid/fdroiddata/-/merge_requests/53450) was opened and then closed by an
F-Droid reviewer because the project did not meet the inclusion policy's requirement to have been actively maintained
for at least three months (or qualify for one of its exceptions). Reopen it or submit a new request only once Alloy
meets that requirement.

Checked: `fdroid lint vn.hce.alloy` (fdroidserver Docker image `registry.gitlab.com/fdroid/docker-executable-fdroidserver`,
with `config/categories.yml` taken from fdroiddata) reported no findings. The category is `Messaging`. `fdroid lint`
exits 0 even when it prints findings, so judge it by its output.

The F-Droid CI built the UI, Rust code and APK, then stopped because the APK had `versionCode=1` instead of the
expected `2003`. That exposed an Android versioning bug in Alloy, fixed separately; it was not a successful F-Droid
release. A later submission still needs the complete build and review to pass.

## What is in the repository

- `fastlane/metadata/android/en-US/`: title, short and full description, icon, three phone screenshots (taken from
  the demo data of the UI, not from a real account) and `changelogs/<versionCode>.txt`. F-Droid reads these from the
  tagged commit. Add a `changelogs/<versionCode>.txt` for each release; the version code is
  `major * 1000000 + minor * 1000 + patch` (0.2.3 is 2003).
- `docs/fdroid/vn.hce.alloy.yml`: the draft build recipe for the `fdroiddata` repository. The application id became
  `vn.hce.alloy` in 0.2.10 (it was `chat.alloy`); the metadata filename must equal the application id, and no listing
  was ever published under either id, so there is no update path to preserve.
- Store builds set `ALLOY_DISTRIBUTION=fdroid` while compiling. The app then skips its release-page update check,
  because F-Droid delivers updates.

## Already compatible

- Apache-2.0, no proprietary dependencies (no Google Play Services, no Firebase), no telemetry.
- Notifications use UnifiedPush, so they need no Google service.
- The Android project is committed (`apps/alloy/src-tauri/gen/android`), and the release build type has no signing
  config, so an unsigned APK can be built from source.

## Open points before submitting

1. **Run the recipe.** Check it with fdroidserver (`fdroid build -v -l vn.hce.alloy` in a `fdroiddata` checkout). A cold
   Rust build is slow, so check it fits the build server's time limit.
2. **Architectures.** Only arm64 is built. Add `armeabi-v7a` and `x86_64` targets if you want older and emulator devices.
3. **Signing.** F-Droid signs with its own key, so the F-Droid app and the signed GitHub APK cannot update each other.
   Say so in the README, or work towards reproducible builds (hard with Rust and the NDK).
4. **Binary scan.** F-Droid rejects prebuilt binaries; `gradle/wrapper/gradle-wrapper.jar` is the likely finding.
   It is normally accepted when it matches the official checksum.
5. **Merge request.** Open it against `gitlab.com/fdroid/fdroiddata` with `metadata/vn.hce.alloy.yml`. Review takes weeks.

A faster first step is IzzyOnDroid, which serves the signed GitHub release APKs and reads the same metadata.
