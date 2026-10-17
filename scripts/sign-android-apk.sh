#!/usr/bin/env bash
# Sign a release-built (unsigned) APK so it can be installed.
#   scripts/sign-android-apk.sh <unsigned.apk> <signed.apk>
# With ANDROID_KEYSTORE_BASE64, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS and
# ANDROID_KEY_PASSWORD set it signs with that release key. Without them it signs with a throwaway
# key: the APK installs, but each build has a different signature, so testers must uninstall
# before updating. Never publish such an APK as a stable release.
set -euo pipefail
in="$1"; out="$2"
apksigner=$(find "${ANDROID_HOME:?ANDROID_HOME is required}/build-tools" -name apksigner | sort -V | tail -n 1)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
  echo "$ANDROID_KEYSTORE_BASE64" | base64 -d > "$tmp/release.keystore"
  ks="$tmp/release.keystore"; pass="$ANDROID_KEYSTORE_PASSWORD"; alias="$ANDROID_KEY_ALIAS"; keypass="$ANDROID_KEY_PASSWORD"
else
  echo "No release keystore configured: signing with a throwaway key" >&2
  ks="$tmp/throwaway.keystore"; pass=android; alias=alloy; keypass=android
  keytool -genkeypair -keystore "$ks" -storepass "$pass" -keypass "$keypass" -alias "$alias" \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Alloy Test,O=Alloy,C=US" >/dev/null 2>&1
fi
"$apksigner" sign --ks "$ks" --ks-pass "pass:$pass" --ks-key-alias "$alias" --key-pass "pass:$keypass" --out "$out" "$in"
"$apksigner" verify "$out"
