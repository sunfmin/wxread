#!/usr/bin/env bash
# Tests ./adr-numbers.sh, which the gate holds docs/adr to: issues built in
# parallel each took the next free number on their own branch and both landed
# (issue #66), so a number named two decisions.
set -euo pipefail
cd "$(dirname "$0")"
here=$PWD
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

# adrs <dir> <file>...: a directory of ADRs holding these files.
adrs() {
  local dir=$work/$1
  shift
  mkdir -p "$dir"
  for f in "$@"; do echo "# $f" >"$dir/$f"; done
}

# expect <name> <status> <stderr substring> <dir>
expect() {
  local name=$1 want=$2 says=$3 status=0
  (cd "$work" && "$here/adr-numbers.sh" "$4") 2>"$work/err" || status=$?
  if [ "$status" -ne "$want" ] || ! case $(cat "$work/err") in *"$says"*) true ;; *) false ;; esac; then
    echo "FAIL $name: exit $status, wanted $want; stderr: $(cat "$work/err"); wanted it to say: ${says:-anything}" >&2
    failures=$((failures + 1))
  else
    echo "ok   $name"
  fi
}

adrs unique 0001-a.md 0002-b.md 0003-c.md
expect "every number its own" 0 "" unique

adrs pair 0001-a.md 0002-b.md 0002-c.md 0003-d.md
expect "two files sharing a number" 1 \
  "gate: pair/0002-b.md and pair/0002-c.md share the number 0002. Give the one that landed last the next free number, 0004, and update every reference to it (\"ADR 0002\", docs/adr/0002-…) to name the decision it meant." \
  pair

adrs pairs 0001-a.md 0001-b.md 0002-c.md 0002-d.md 0002-e.md
expect "every shared number named" 1 "pairs/0002-c.md, pairs/0002-d.md and pairs/0002-e.md share the number 0002" pairs
expect "the first shared number named too" 1 "pairs/0001-a.md and pairs/0001-b.md share the number 0001" pairs

[ "$failures" -eq 0 ] || {
  echo "adr-numbers-test: $failures failed" >&2
  exit 1
}
