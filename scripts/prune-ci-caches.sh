#!/usr/bin/env bash
# Keeps the GitHub Actions cache under the repository's 10 GB budget.
#
# Why this exists: the repository runs a 900-crate debug matrix (three OSes + Android) plus four
# release-profile caches from the nightly job. Every dependency change writes a *new* key for each
# of those, and GitHub only evicts entries when the 10 GB budget is exhausted - by then it may
# evict the release caches, and the next tag build recompiles Tauri, matrix-sdk and E2E for ~25
# minutes from scratch (measured: macOS x64 `Finished release profile in 25m 01s`).
#
# Rules, in order of confidence:
#   1. Caches written by a tag run (ref refs/tags/*) can only ever be read by later runs of the
#      same tag. They are dead weight after the release; delete them.
#   2. When several entries share the same key family (the key with its trailing content hashes
#      removed), keep the newest and delete the rest. The older entries are superseded by cargo's
#      own fingerprinting and only consume budget.
#   3. Everything else is kept, including the newest entry of every family, so the next CI run
#      still restores its cache.
#
# Usage:
#   scripts/prune-ci-caches.sh            # dry run: list what would be deleted
#   scripts/prune-ci-caches.sh --apply    # delete
#   GITHUB_REPOSITORY=owner/name scripts/prune-ci-caches.sh --apply
#
# Needs: gh (authenticated with a token that may delete Actions caches) and python3.
set -euo pipefail

repo="${GITHUB_REPOSITORY:-centopw/alloy}"
apply=0
[ "${1:-}" = "--apply" ] && apply=1

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
gh cache list --repo "$repo" --limit 100 \
  --json id,key,ref,sizeInBytes,lastAccessedAt > "$tmp"

python3 - "$tmp" "$apply" "$repo" <<'PY'
import json, re, sys

caches = json.load(open(sys.argv[1]))
apply = sys.argv[2] == "1"

# Drop the trailing content-hash segments rust-cache/node append to a family key:
#   v0-rust-dev-ubuntu-latest-Linux-x64-93b0a40d-c511bf42  -> ...-Linux-x64
#   node-cache-Linux-x64-pnpm-<64 hex>                     -> node-cache-Linux-x64-pnpm
#   setup-java-Linux-x64-gradle-<64 hex>                   -> setup-java-Linux-x64-gradle
def family(key: str) -> str:
    parts = key.split("-")
    while parts and re.fullmatch(r"[0-9a-f]{8,64}", parts[-1]):
        parts.pop()
    return "-".join(parts)

caches.sort(key=lambda c: c["id"])  # GitHub ids increase over time
newest: dict[str, int] = {}
for c in caches:
    newest[family(c["key"])] = c["id"]

delete = []
for c in caches:
    ref = c.get("ref") or ""
    reason = None
    if ref.startswith("refs/tags/"):
        reason = "tag-scoped cache is unreadable outside that tag"
    elif newest[family(c["key"])] != c["id"]:
        reason = "superseded by a newer cache of the same family"
    if reason:
        delete.append((c, reason))

keep_bytes = sum(c["sizeInBytes"] for c in caches if all(d[0]["id"] != c["id"] for d in delete))
free_bytes = sum(c[0]["sizeInBytes"] for c in delete)
print(f"caches: {len(caches)}, kept: {len(caches) - len(delete)} "
      f"({keep_bytes / 2**20:.0f} MiB), delete: {len(delete)} ({free_bytes / 2**20:.0f} MiB)")
for c, reason in sorted(delete, key=lambda d: -d[0]["sizeInBytes"]):
    print(f"  {c['sizeInBytes'] / 2**20:7.0f} MiB  {c['key']:<70}  {reason}")
if not apply:
    print("\ndry run; pass --apply to delete")
else:
    for c, _ in delete:
        print(f"deleting {c['id']} …", flush=True)
        import subprocess
        subprocess.run(["gh", "cache", "delete", str(c["id"]), "--repo", sys.argv[3]],
                       check=True)
    print(f"deleted {len(delete)} cache(s), freed {free_bytes / 2**20:.0f} MiB")
PY
