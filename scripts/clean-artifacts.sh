#!/usr/bin/env bash
# Remove regenerable build leftovers (per-agent cargo targets, docker build cache, android build output).
# Never touches source, docs, the running Synapse container or tests/synapse/synapse_data.
#   scripts/clean-artifacts.sh              clean /tmp/alloy-target-* and docker build cache
#   scripts/clean-artifacts.sh --keep NAME  keep /tmp/alloy-target-NAME (repeatable)
#   scripts/clean-artifacts.sh --deep       also remove ./target, ui/.svelte-kit, ui/build, android build dirs
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
deep=0
keep=()
while [ $# -gt 0 ]; do
  case "$1" in
    --deep) deep=1 ;;
    --keep) shift; keep+=("/tmp/alloy-target-$1") ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

before=$(df -k / | awk 'NR==2 {print $4}')

for d in /tmp/alloy-target-*; do
  [ -d "$d" ] || continue
  skip=0
  for k in ${keep[@]+"${keep[@]}"}; do [ "$d" = "$k" ] && skip=1; done
  [ "$skip" = 1 ] && continue
  echo "rm $d"
  rm -rf "$d"
done

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  docker builder prune -f >/dev/null
  docker image prune -f >/dev/null
fi

if [ "$deep" = 1 ]; then
  rm -rf "$root/target" "$root/apps/alloy/ui/.svelte-kit" "$root/apps/alloy/ui/build" \
    "$root/apps/alloy/src-tauri/gen/android/app/build" "$root/apps/alloy/src-tauri/gen/android/.gradle"
fi

after=$(df -k / | awk 'NR==2 {print $4}')
echo "freed $(( (after - before) / 1024 )) MiB"
