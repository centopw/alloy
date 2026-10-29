#!/usr/bin/env bash
# Splits the alloy-engine integration tests (each needs a running Synapse and takes about a minute) into
# shards so CI can run them in parallel, each shard with its own Synapse.
#
#   scripts/ci-engine-shards.sh 2        prints the cargo arguments for shard 2 (`--test a --test b`)
#   scripts/ci-engine-shards.sh check    fails when a tests/*.rs file belongs to no shard, so a new test cannot be skipped silently
#
# Shard 1 also runs the crate's unit tests (`--lib`).
set -euo pipefail

cd "$(dirname "$0")/.."

# Rough cost in seconds is noted for the heavy ones; keep the shards about even when adding a file.
SHARD_1="parity_integration prefs_integration safety_integration polls_integration"
SHARD_2="search_integration user_directory_integration tags_integration media_location_integration room_admin_integration oidc_integration"
SHARD_3="members_integration notifications_integration room_media_integration rooms_integration threads_integration"
SHARD_4="timeline_integration unread_integration account_integration auth_integration crypto_integration directory_integration engine_tests media_integration"

case "${1:-}" in
  check)
    assigned=" $SHARD_1 $SHARD_2 $SHARD_3 $SHARD_4 "
    missing=0
    for f in crates/alloy-engine/tests/*.rs; do
      name=$(basename "$f" .rs)
      if [[ "$assigned" != *" $name "* ]]; then
        echo "::error::$f is in no shard of scripts/ci-engine-shards.sh" >&2
        missing=1
      fi
    done
    exit $missing
    ;;
  1) tests=$SHARD_1; extra="--lib" ;;
  2) tests=$SHARD_2; extra="" ;;
  3) tests=$SHARD_3; extra="" ;;
  4) tests=$SHARD_4; extra="" ;;
  *) echo "usage: $0 <1-4|check>" >&2; exit 2 ;;
esac

args="$extra"
for t in $tests; do args="$args --test $t"; done
echo "${args# }"
