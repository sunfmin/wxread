#!/usr/bin/env bash
# Tests ./flux-summary.sh on logs cargo prints when it checks crates in
# parallel: one crate's summary can come between another's summary and its
# artifact line (issue #22), as in this order, taken from a fresh
# `./verify.sh --message-format=json-render-diagnostics`:
#
#   summary. 13 functions processed: 13 checked; 0 trusted; 0 ignored. ...   (wxread-crack's)
#   summary. 7 functions processed: 7 checked; 0 trusted; 0 ignored. ...     (wxread-account's)
#   {"reason":"compiler-artifact", ... "name":"wxread_crack" ...}
#   {"reason":"compiler-artifact", ... "name":"wxread_account" ...}
#
# Each crate's own summary is in the output cargo keeps for its unit, which it
# replays when the unit is fresh; the tests write those files as cargo does.
set -euo pipefail
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

# unit <crate> <summary>: writes the unit cargo built for <crate>, its output
# holding <summary> as Flux prints it (bold, then grey), and prints the
# artifact line cargo says when it is done.
unit() {
  local name=${1//-/_} hash
  hash=$(printf '%s' "$1" | cksum | cut -d' ' -f1)
  local dir="$work/target/flux/build/$1/$hash"
  mkdir -p "$dir/out" "$dir/fingerprint"
  : >"$dir/out/lib$name-$hash.rmeta"
  if [ -n "$2" ]; then
    printf '{"$message_type":"unused_extern","lint_level":"force-warn","unused_extern_names":["flux_rs"]}\n\033[1msummary.\033[0m %s\033[0m\n' "$2" >"$dir/fingerprint/output-lib-$name"
  fi
  printf '{"reason":"compiler-artifact","package_id":"path+file://%s/crates/%s#%s@0.1.0","target":{"kind":["lib"],"crate_types":["lib"],"name":"%s","src_path":"%s/crates/%s/src/lib.rs","edition":"2024","doc":true,"doctest":true,"test":true},"features":[],"filenames":["%s"],"executable":null,"fresh":false}\n' \
    "$work" "${1#wxread-}" "$1" "$name" "$work" "${1#wxread-}" "$dir/out/lib$name-$hash.rmeta"
}

clean() { printf '%s functions processed: %s checked; 0 trusted; 0 ignored. 0 constraints solved. Finished in 35.93ms' "$1" "$1"; }

# expect <name> <status> <stderr substring> <log> <crate>...
expect() {
  local name=$1 want=$2 says=$3 log=$4 status=0
  shift 4
  ./flux-summary.sh "$log" "$@" 2>"$work/err" || status=$?
  if [ "$status" -ne "$want" ] || ! case $(cat "$work/err") in *"$says"*) true ;; *) false ;; esac; then
    echo "FAIL $name: exit $status, wanted $want; stderr: $(cat "$work/err"); wanted it to say: ${says:-anything}" >&2
    failures=$((failures + 1))
  else
    echo "ok   $name"
  fi
}

# The order above, every crate clean: both pass.
crack=$(unit wxread-crack "$(clean 13)")
account=$(unit wxread-account "$(clean 7)")
{
  echo "    Checking wxread-crack v0.1.0"
  echo "    Checking wxread-account v0.1.0"
  echo "summary. $(clean 13)"
  echo "summary. $(clean 7)"
  echo "$crack"
  echo "$account"
} >"$work/interleaved.log"
expect "interleaved summaries, both clean" 0 "" "$work/interleaved.log" wxread-crack wxread-account

# The same order, wxread-crack trusting a function: the clean summary printed
# just before its artifact line is wxread-account's, and does not pass it.
crack=$(unit wxread-crack "13 functions processed: 12 checked; 1 trusted; 0 ignored. 0 constraints solved. Finished in 34.61ms")
expect "interleaved summaries, one trusting" 1 \
  "gate: the pure crate wxread-crack must not exempt anything from Flux, and Flux reports: summary. 13 functions processed: 12 checked; 1 trusted; 0 ignored." \
  "$work/interleaved.log" wxread-crack wxread-account

# A crate ignoring a function fails.
account=$(unit wxread-account "7 functions processed: 6 checked; 0 trusted; 1 ignored. 0 constraints solved. Finished in 35.93ms")
expect "one ignoring" 1 "gate: the pure crate wxread-account must not exempt anything from Flux, and Flux reports: summary. 7 functions processed: 6 checked; 0 trusted; 1 ignored." \
  "$work/interleaved.log" wxread-account

# A crate Flux printed no summary for fails, though the log holds others.
account=$(unit wxread-account "")
expect "no summary in its output" 1 "gate: the pure crate wxread-account must not exempt anything from Flux, and Flux reports: no summary of it" \
  "$work/interleaved.log" wxread-account

# A crate cargo never reported fails.
expect "no artifact" 1 "gate: the pure crate wxread-keys must not exempt anything from Flux, and Flux reports: no summary of it" \
  "$work/interleaved.log" wxread-keys

[ "$failures" -eq 0 ] || {
  echo "flux-summary-test: $failures failed" >&2
  exit 1
}
