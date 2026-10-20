#!/usr/bin/env bash
# Holds every ADR to a number of its own (issue #66).
#
#   ./adr-numbers.sh [<dir>]      <dir> is docs/adr unless named
#
# Issues built in parallel each take the next free number on their own branch,
# and both land; the gate, which every branch runs before it lands, catches
# the second. It names each number two or more files share and the fix, and
# exits 1.
set -euo pipefail
dir=${1:-docs/adr}
numbers=$(find "$dir" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-*.md' -exec basename {} \; | cut -c1-4 | sort)
next=$(printf '%04d' $((10#$(tail -n1 <<<"$numbers") + 1)))
status=0
for n in $(uniq -d <<<"$numbers"); do
  files=$(find "$dir" -maxdepth 1 -name "$n-*.md" -exec basename {} \; | sort | sed "s|^|$dir/|")
  last=$(tail -n1 <<<"$files")
  rest=$(sed '$d' <<<"$files" | paste -sd, - | sed 's/,/, /g')
  echo "gate: $rest and $last share the number $n. Give the one that landed last the next free number, $next, and update every reference to it (\"ADR $n\", docs/adr/$n-…) to name the decision it meant." >&2
  status=1
done
exit "$status"
