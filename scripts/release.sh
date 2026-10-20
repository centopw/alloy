#!/usr/bin/env bash
# Cuts a release in one command:  scripts/release.sh 0.2.2   (or: just release 0.2.2)
#
# Edits exactly one file by hand per release: write the "## X.Y.Z (pre-release)" section in CHANGELOG.md
# first and commit it. This script then bumps the only version source (apps/alloy/src-tauri/Cargo.toml)
# and Cargo.lock, pushes the bump, waits for CI to pass on that commit, and only then pushes the tag.
# The tag starts release.yml, whose first job (scripts/verify-release.sh) re-checks all of this on the
# server, so a tag pushed by any other route is held to the same rules.
set -euo pipefail

ver="${1:?usage: release.sh X.Y.Z}"
ver="${ver#v}"
[[ "$ver" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "'$ver' is not X.Y.Z" >&2; exit 1; }
root="$(git rev-parse --show-toplevel)"
cd "$root"
manifest=apps/alloy/src-tauri/Cargo.toml

die() { echo "release: $*" >&2; exit 1; }

[ "$(git branch --show-current)" = main ] || die "run this on main"
git fetch --quiet origin main --tags
[ -z "$(git status --porcelain --untracked-files=no)" ] || die "the working tree has uncommitted changes"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main is not in sync with origin/main"
git rev-parse -q --verify "refs/tags/v$ver" >/dev/null && die "tag v$ver already exists"
grep -q "^## ${ver//./\\.} " CHANGELOG.md || die "write the '## $ver (pre-release)' section in CHANGELOG.md and commit it first"

current="$(cargo metadata --no-deps --format-version 1 --offline | jq -r '.packages[] | select(.name == "alloy-app") | .version')"
[ "$(printf '%s\n%s\n' "$current" "$ver" | sort -V | tail -n1)" = "$ver" ] && [ "$current" != "$ver" ] \
  || die "$ver is not newer than the current version $current"

# The only version edit: the [package] version line of the app manifest.
awk -v v="$ver" '!done && /^version = "/ { print "version = \"" v "\""; done = 1; next } { print }' "$manifest" > "$manifest.tmp"
mv "$manifest.tmp" "$manifest"
cargo update -p alloy-app --offline

git add "$manifest" Cargo.lock
git commit -q -m "chore(release): bump to $ver"
git push -q origin main
sha="$(git rev-parse HEAD)"

echo "release: waiting for CI on $sha (the gate checks it again before building)"
CI_WAIT_MINUTES=60 scripts/verify-release.sh "v$ver" "$sha" || { echo "release: not tagged; fix the problem and run again" >&2; exit 1; }

git tag "v$ver" "$sha"
git push -q origin "v$ver"
echo "release: pushed v$ver; watch it with: gh run watch -R centopw/alloy"
