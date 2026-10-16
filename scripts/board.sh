#!/usr/bin/env bash
# Project board helper: keeps GitHub issues + Project #2 (centopw) in sync.
# Usage:
#   board.sh status <issue#> <Backlog|Ready|"In progress"|"In review"|Done>
#   board.sh set    <issue#> <Priority|Size> <value>      e.g. set 12 Size L
#   board.sh new    --title T --milestone M [--labels a,b] [--priority P0|P1|P2] [--size XS|S|M|L|XL]
#                   [--status S] [--body-file F | --body TEXT]
#   board.sh show   [issue#]        list board items (or one issue)
#   board.sh claim  <issue#>        status -> In progress and comment "claimed"
#   board.sh done   <issue#> [sha]  status -> Done, close issue, comment with sha
# Requires: gh (scopes: repo, project), jq.
set -euo pipefail
REPO="${ALLOY_REPO:-centopw/alloy}"; OWNER="${ALLOY_PROJECT_OWNER:-centopw}"; PN="${ALLOY_PROJECT_NUMBER:-2}"

CACHE="${ALLOY_BOARD_CACHE:-${TMPDIR:-/tmp}/alloy-board-cache-$PN.json}"

meta() { # project id + field/option ids, cached (delete the cache file to refresh)
  if [ ! -s "$CACHE" ]; then
    local pid fj
    pid=$(gh project view "$PN" --owner "$OWNER" --format json | jq -r .id)
    fj=$(gh project field-list "$PN" --owner "$OWNER" --format json)
    jq -n --arg pid "$pid" --argjson f "$fj" '{project_id:$pid, fields:$f.fields}' >"$CACHE"
  fi
  cat "$CACHE"
}

item_id_for_issue() { # echoes project item id (adds issue to project if absent); one GraphQL query
  local num="$1" pid id
  pid=$(meta | jq -r .project_id)
  id=$(gh api graphql -f query='query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){projectItems(first:20){nodes{id project{id}}}}}}' \
        -f o="${REPO%%/*}" -f r="${REPO##*/}" -F n="$num" 2>/dev/null \
        | jq -r --arg pid "$pid" '.data.repository.issue.projectItems.nodes[]|select(.project.id==$pid)|.id' | head -n1 || true)
  if [ -z "$id" ]; then
    id=$(gh project item-add "$PN" --owner "$OWNER" --url "https://github.com/$REPO/issues/$num" --format json | jq -r .id)
  fi
  echo "$id"
}

set_field() { # item_id field_name option_name
  local item="$1" field="$2" value="$3" m pid fid oid
  m=$(meta); pid=$(jq -r .project_id <<<"$m")
  fid=$(jq -r --arg f "$field" '.fields[]|select(.name==$f)|.id' <<<"$m")
  oid=$(jq -r --arg f "$field" --arg v "$value" '.fields[]|select(.name==$f)|.options[]?|select(.name==$v)|.id' <<<"$m")
  [ -n "$fid" ] && [ -n "$oid" ] || { echo "unknown field/value: $field=$value" >&2; exit 2; }
  gh project item-edit --id "$item" --project-id "$pid" --field-id "$fid" --single-select-option-id "$oid" >/dev/null
}

cmd="${1:-}"; shift || true
case "$cmd" in
  status) num="$1"; val="$2"; set_field "$(item_id_for_issue "$num")" Status "$val"; echo "#$num -> $val" ;;
  set)    num="$1"; set_field "$(item_id_for_issue "$num")" "$2" "$3"; echo "#$num $2=$3" ;;
  claim)  num="$1"; set_field "$(item_id_for_issue "$num")" Status "In progress"
          gh issue comment "$num" -R "$REPO" --body "Claimed by ${ALLOY_AGENT:-$(git config user.name)} (status: In progress)." >/dev/null; echo "claimed #$num" ;;
  done)   num="$1"; sha="${2:-}"; set_field "$(item_id_for_issue "$num")" Status Done
          gh issue comment "$num" -R "$REPO" --body "Done${sha:+ in $sha}." >/dev/null
          gh issue close "$num" -R "$REPO" --reason completed >/dev/null 2>&1 || true; echo "done #$num" ;;
  new)
    title=""; ms=""; labels=""; prio="P1"; size="M"; status="Backlog"; body=""
    while [ $# -gt 0 ]; do case "$1" in
      --title) title="$2"; shift 2;; --milestone) ms="$2"; shift 2;; --labels) labels="$2"; shift 2;;
      --priority) prio="$2"; shift 2;; --size) size="$2"; shift 2;; --status) status="$2"; shift 2;;
      --body) body="$2"; shift 2;; --body-file) body="$(cat "$2")"; shift 2;; *) echo "bad arg $1" >&2; exit 2;; esac; done
    [ -n "$title" ] || { echo "--title required" >&2; exit 2; }
    args=(issue create -R "$REPO" --title "$title" --body "${body:-Tracking item.}")
    [ -n "$ms" ] && args+=(--milestone "$ms")
    IFS=',' read -ra L <<<"$labels"; for l in "${L[@]:-}"; do [ -n "$l" ] && args+=(--label "$l"); done
    url=$(gh "${args[@]}"); num="${url##*/}"; item=$(item_id_for_issue "$num")
    set_field "$item" Status "$status"; set_field "$item" Priority "$prio"; set_field "$item" Size "$size"
    echo "$url" ;;
  show)
    if [ $# -ge 1 ]; then gh issue view "$1" -R "$REPO"
    else gh project item-list "$PN" --owner "$OWNER" --format json -L 500 | jq -r '.items[]|[.content.number,.status,.priority,.size,.title]|@tsv' | sort -n; fi ;;
  *) sed -n '2,12p' "$0"; exit 2 ;;
esac
