#!/usr/bin/env bash
# Release gate: refuses to publish unless the tag is safe to release. Run by the `verify` job of
# release.yml before anything is built or uploaded; run it by hand to check a tag first:
#   scripts/verify-release.sh v0.2.2 [sha]
#
# A tag passes only when ALL of these hold:
#   1. it is a plain semver tag (vX.Y.Z, optionally with a -prerelease suffix);
#   2. it equals the version in apps/alloy/src-tauri/Cargo.toml, the single source of the app version
#      (tauri.conf.json must not define its own copy, and ui/package.json must not disagree);
#   3. CHANGELOG.md has a "## X.Y.Z" section;
#   4. the tagged commit is on main;
#   5. the CI workflow finished green for that exact commit (waits while it is still running).
# Needs: git, jq, cargo, and gh (authenticated; GH_TOKEN in CI) for check 5.
set -euo pipefail

tag="${1:?usage: verify-release.sh vX.Y.Z [sha]}"
ver="${tag#v}"
repo="${GITHUB_REPOSITORY:-centopw/alloy}"
root="$(git rev-parse --show-toplevel)"
# Minutes to wait for a CI run that is still going. 0 (the default, used by the tag job in Actions) fails
# at once; re-run the job when CI is green. scripts/release.sh waits before it tags, so a tag it pushes
# always finds a finished run.
wait_minutes="${CI_WAIT_MINUTES:-0}"

fail() { echo "release gate: $*" >&2; exit 1; }

# 1. Tag shape.
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || fail "'$tag' is not a vX.Y.Z tag"
# The Actions API matches full commit ids only.
sha="$(git rev-parse --verify "${2:-$tag}^{commit}" 2>/dev/null)" || fail "cannot resolve ${2:-$tag} to a commit; is the tag pushed?"

# 2. One source of truth for the version.
cargo_ver="$(cd "$root" && cargo metadata --no-deps --format-version 1 --offline 2>/dev/null \
  | jq -r '.packages[] | select(.name == "alloy-app") | .version')"
[ -n "$cargo_ver" ] || fail "cannot read the alloy-app version from Cargo metadata"
[ "$cargo_ver" = "$ver" ] || fail "tag is $ver but apps/alloy/src-tauri/Cargo.toml says $cargo_ver"
jq -e 'has("version") | not' "$root/apps/alloy/src-tauri/tauri.conf.json" >/dev/null \
  || fail "tauri.conf.json defines \"version\"; remove it so Cargo.toml stays the only source"
ui_ver="$(jq -r '.version // empty' "$root/apps/alloy/ui/package.json")"
[ -z "$ui_ver" ] || [ "$ui_ver" = "$ver" ] || fail "apps/alloy/ui/package.json says $ui_ver, tag is $ver"

# 3. Release notes exist.
grep -q "^## ${ver//./\\.} " "$root/CHANGELOG.md" || fail "CHANGELOG.md has no '## $ver' section"

# 4. Only commits that are on main can be released.
git fetch --quiet origin main
git merge-base --is-ancestor "$sha" origin/main || fail "$sha is not on main"

# 5. Green CI on exactly this commit.
deadline=$((SECONDS + wait_minutes * 60))
while :; do
  runs="$(gh api "repos/$repo/actions/workflows/ci.yml/runs?head_sha=$sha&event=push&per_page=20" \
    --jq '[.workflow_runs[] | {status, conclusion}]')"
  if jq -e 'any(.[]; .conclusion == "success")' <<<"$runs" >/dev/null; then
    echo "release gate: $tag ($sha) passed: version $ver, CHANGELOG, on main, CI green"
    exit 0
  fi
  if jq -e 'length == 0' <<<"$runs" >/dev/null; then
    fail "no CI run for $sha; CI skips docs-only commits, so tag a commit that changed code"
  fi
  if jq -e 'all(.[]; .status == "completed")' <<<"$runs" >/dev/null; then
    fail "CI did not succeed for $sha ($(jq -r '[.[].conclusion] | join(", ")' <<<"$runs"))"
  fi
  [ "$SECONDS" -lt "$deadline" ] || fail "CI for $sha has not finished (waited $wait_minutes min); re-run this job once it is green"
  echo "release gate: CI for $sha is still running, checking again in 30 s"
  sleep 30
done
