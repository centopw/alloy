#!/usr/bin/env bash
# Validates the subject line against Conventional Commits 1.0.
# Usage: check-commit-msg.sh <file-with-message>   (or "-" for stdin)
set -euo pipefail
src="${1:--}"
subject="$(if [ "$src" = "-" ]; then head -n1; else head -n1 "$src"; fi)"
case "$subject" in Merge\ *|Revert\ *|fixup!\ *|squash!\ *) exit 0 ;; esac
types='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
re="^(${types})(\([a-z0-9][a-z0-9._/-]*\))?!?: [^A-Z ].{0,71}[^.]$"
if ! [[ "$subject" =~ $re ]]; then
  cat >&2 <<MSG
Invalid commit subject: "$subject"
Expected: <type>(<scope>)?!: <lowercase imperative summary>   (max 72 chars, no trailing period)
Types: ${types//|/, }
MSG
  exit 1
fi
