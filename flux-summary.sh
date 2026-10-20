#!/usr/bin/env bash
# Holds each pure crate to Flux's summary of it: `0 trusted; 0 ignored`.
#
#   ./flux-summary.sh <log> <crate>...
#
# <log> is what `./verify.sh --message-format=json-render-diagnostics` printed,
# stdout and stderr together. A pure crate's summary is the one printed just
# before cargo says that crate's artifact is done. It names the first crate
# whose summary is missing or exempts anything, and exits 1.
set -euo pipefail
log=$1
shift
summaries=$(awk '
    /^\{/ {
        if (/"reason":"compiler-artifact"/) {
            match($0, /"name":"[^"]*"/)
            print substr($0, RSTART + 8, RLENGTH - 9) "\t" last
            last = ""
        }
        next
    }
    /^summary\./ { last = $0 }' "$log")
for crate in "$@"; do
  summary=$(printf '%s\n' "$summaries" | awk -F'\t' -v c="${crate//-/_}" -v d="$crate" '$1 == c || $1 == d { s = $2 } END { print s }')
  case "$summary" in
  *"; 0 trusted; 0 ignored."*) ;;
  *)
    echo "gate: the pure crate $crate must not exempt anything from Flux, and Flux reports: ${summary:-no summary of it}" >&2
    exit 1
    ;;
  esac
done
