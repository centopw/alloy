#!/usr/bin/env bash
# Validates a full commit message:
#   1. Subject follows Conventional Commits 1.0 (lowercase imperative, <=72 chars, no trailing period).
#   2. feat/fix/perf/refactor commits MUST reference a tracking issue in the body/footer
#      (Refs #N | Closes #N | Fixes #N | Resolves #N | Part of #N) - see docs/TRACKING.md.
# Usage: check-commit-msg.sh <file-with-message>   (or "-" for stdin)
set -euo pipefail
src="${1:--}"
msg="$(if [ "$src" = "-" ]; then cat; else cat "$src"; fi)"
subject="$(printf '%s\n' "$msg" | head -n1)"
case "$subject" in Merge\ *|Revert\ *|fixup!\ *|squash!\ *) exit 0 ;; esac
types='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
re="^(${types})(\([a-z0-9][a-z0-9._/,-]*\))?!?: [^A-Z ].{0,71}[^.]$"
if ! [[ "$subject" =~ $re ]]; then
  cat >&2 <<MSG
Invalid commit subject: "$subject"
Expected: <type>(<scope>)?!: <lowercase imperative summary>   (max 72 chars, no trailing period)
Types: ${types//|/, }
MSG
  exit 1
fi
type="${subject%%[(:!]*}"
case "$type" in feat|fix|perf|refactor)
  if ! printf '%s\n' "$msg" | tail -n +2 | grep -Eiq '(refs|closes|fixes|resolves|part of):? +#[0-9]+'; then
    echo "Commit type '$type' must reference an issue, e.g. 'Refs #12' or 'Closes #12' in the body. See docs/TRACKING.md." >&2
    exit 1
  fi ;;
esac
