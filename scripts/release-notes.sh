#!/usr/bin/env bash
# Prints the GitHub release notes for a version tag:
#   highlights from CHANGELOG.md, the commits since the previous version (git-cliff, grouped, with
#   links), a compare link, and the install notes. Used by the release workflow; run it by hand to
#   preview or to refresh an old release:  scripts/release-notes.sh v0.1.6
# Needs: git (with tags), git-cliff.
set -euo pipefail
tag="${1:?usage: release-notes.sh vX.Y.Z}"
ver="${tag#v}"
repo="${GITHUB_REPOSITORY:-centopw/alloy}"
root="$(git rev-parse --show-toplevel)"

# Previous version tag (descending semver order, only vX.Y.Z tags); empty for the first release.
prev="$(git tag --list 'v[0-9]*' --sort=-v:refname | grep -x -A1 -- "$tag" | sed -n 2p || true)"

highlights="$(awk -v v="## $ver " 'index($0, v) == 1 { on = 1; next } /^## / { on = 0 } on' "$root/CHANGELOG.md" | sed -e '/./,$!d')"
if [ -n "$highlights" ]; then
  printf '## Highlights\n\n%s\n\n' "$highlights"
fi

# The first release has no predecessor: start from the root commit.
base="${prev:-$(git rev-list --max-parents=0 "$tag" | tail -n1)}"
commits="$(git cliff --config "$root/cliff.toml" --strip header "$base..$tag" 2>/dev/null | sed '1,/^$/d')"
if [ -n "$commits" ]; then
  printf "## What's changed\n\n%s\n\n" "$commits"
fi

if [ -n "$prev" ]; then
  printf '**Full changelog:** https://github.com/%s/compare/%s...%s\n\n' "$repo" "$prev" "$tag"
else
  printf '**Full changelog:** https://github.com/%s/commits/%s\n\n' "$repo" "$tag"
fi

printf -- '---\n\n## Install\n\n'
cat "$root/.github/release-notes.md"
