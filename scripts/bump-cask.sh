#!/usr/bin/env bash
# Bump the Alloy cask (centopw/homebrew-tap, token "alloy-client") to a released version.
#   scripts/bump-cask.sh <version> [SHA256SUMS] [--dry-run]
# <version> is a tag (`v0.4.18`) or a bare version (`0.4.18`). Without SHA256SUMS the file is
# downloaded from the tag's GitHub release, which needs GH_TOKEN (set in the release workflow).
# With TAP_TOKEN unset the script prints a skip notice and exits 0, so a release without the
# secret stays green; with --dry-run it prints the rendered cask and touches nothing.
set -euo pipefail

usage() { echo "usage: $0 <version> [SHA256SUMS] [--dry-run]" >&2; exit 2; }

dry_run=false
version=""
sums=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=true ;;
    -h|--help) usage ;;
    -*)
      echo "unknown option: $arg" >&2
      usage
      ;;
    *)
      if [ -z "$version" ]; then version="$arg"; elif [ -z "$sums" ]; then sums="$arg"; else usage; fi
      ;;
  esac
done
[ -n "$version" ] || usage
version="${version#v}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not a stable version: $version" >&2
  exit 2
fi

token="${TAP_TOKEN:-}"
if [ -z "$token" ] && [ "$dry_run" = false ]; then
  echo "TAP_TOKEN is not set: skipping the Homebrew cask update." >&2
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$repo_root/packaging/homebrew/alloy-client.rb.in"
tap="${ALLOY_TAP:-centopw/homebrew-tap}"
cask_path="Casks/alloy-client.rb"
[ -f "$template" ] || { echo "missing cask template: $template" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [ -z "$sums" ]; then
  gh release download "v$version" -R "${GITHUB_REPOSITORY:-centopw/alloy}" -p SHA256SUMS -O "$work/SHA256SUMS"
  sums="$work/SHA256SUMS"
fi
[ -f "$sums" ] || { echo "missing checksum file: $sums" >&2; exit 1; }

# sha256sum writes "hash  name" ("*name" with --binary); accept an optional ./ before the name.
sha_for() {
  awk -v name="$1" '{
    f = $2
    sub(/^\*/, "", f)
    sub(/^\.\//, "", f)
    if (f == name) { print $1; exit }
  }' "$sums"
}

arm_name="Alloy_${version}_aarch64.dmg"
intel_name="Alloy_${version}_x64.dmg"
arm_sha="$(sha_for "$arm_name")"
intel_sha="$(sha_for "$intel_name")"
for pair in "$arm_name:${arm_sha:-missing}" "$intel_name:${intel_sha:-missing}"; do
  name="${pair%:*}"
  sha="${pair##*:}"
  if ! [[ "$sha" =~ ^[0-9a-f]{64}$ ]]; then
    echo "no SHA256 for $name in $sums" >&2
    exit 1
  fi
done

rendered="$(<"$template")"
rendered="${rendered//@@VERSION@@/$version}"
rendered="${rendered//@@SHA256_ARM64@@/$arm_sha}"
rendered="${rendered//@@SHA256_X64@@/$intel_sha}"
if [[ "$rendered" == *"@@"* ]]; then
  echo "cask template still has placeholders" >&2
  exit 1
fi

if [ "$dry_run" = true ]; then
  printf '%s\n' "$rendered"
  exit 0
fi

# The token goes in a request header, never in a URL or a remote, so it cannot leak into logs.
auth="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$token" | base64 | tr -d '\n')"
git -c http.extraHeader="$auth" clone --depth=1 --quiet "https://github.com/$tap.git" "$work/tap"

if [ -f "$work/tap/$cask_path" ] && [ "$(<"$work/tap/$cask_path")" = "$rendered" ]; then
  echo "$tap/$cask_path is already at $version."
  exit 0
fi

mkdir -p "$(dirname "$work/tap/$cask_path")"
printf '%s\n' "$rendered" > "$work/tap/$cask_path"
git -C "$work/tap" \
  -c user.name="Alloy Release" -c user.email="alloy-release@users.noreply.github.com" \
  add "$cask_path"
git -C "$work/tap" \
  -c user.name="Alloy Release" -c user.email="alloy-release@users.noreply.github.com" \
  commit --quiet -m "alloy-client $version"
git -C "$work/tap" -c http.extraHeader="$auth" push --quiet origin HEAD:main
echo "Updated $tap/$cask_path to $version."
