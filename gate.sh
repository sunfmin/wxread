#!/usr/bin/env bash
# The definition of "may land": the same locally, in CI and for afkconvoy.
set -euo pipefail
cd "$(dirname "$0")"

# It runs on rustup's nightly, the one cargo-flux is built for,
# whatever the shell that started it has on its PATH: a Homebrew `cargo` ahead
# of rustup's breaks when Homebrew moves a library under it. The release
# builds on stable, where Flux's attributes expand to nothing.
PATH="$HOME/.cargo/bin:$PATH"
RUSTUP_TOOLCHAIN=nightly-2026-09-25
export PATH RUSTUP_TOOLCHAIN

cargo fmt --all --check

# Flux runs first, alone: beside the tests it shares the cores, and its time
# measures them. A solver query past twenty seconds fails naming its function
# (`-Ffixpoint-timeout`), it checks on past an error so one run names every
# slow function (`-Fcatch-bugs`), and it reports the slowest (`-Ftimings`).
# Every function of every pure crate is checked: Flux's summary of each one
# must say `0 trusted; 0 ignored`. A dependency is summarised too, and is not
# held to it: a pure crate's summary is the one printed just before cargo
# says that crate's artifact is done.
FLUXFLAGS="-Ftimings -Ffixpoint-timeout=20 -Fcatch-bugs -Flog-dir=$PWD/target/flux-log"
export FLUXFLAGS
flux_log=$(mktemp)
trap 'rm -f "$flux_log"' EXIT
./verify.sh --message-format=json-render-diagnostics >"$flux_log" 2>&1 && flux_status=0 || flux_status=$?
pure=$(sed -n 's/^PURE="\(.*\)"$/\1/p' verify.sh)
summaries=$(awk '
    /^\{/ {
        if (/"reason":"compiler-artifact"/) {
            match($0, /"name":"[^"]*"/)
            print substr($0, RSTART + 8, RLENGTH - 9) "\t" last
            last = ""
        }
        next
    }
    { print > "/dev/stderr" }
    /^summary\./ { last = $0 }' "$flux_log")
[ "$flux_status" -eq 0 ] || exit "$flux_status"
for crate in $pure; do
  summary=$(printf '%s\n' "$summaries" | awk -F'\t' -v c="${crate//-/_}" -v d="$crate" '$1 == c || $1 == d { s = $2 } END { print s }')
  case "$summary" in
  *"; 0 trusted; 0 ignored."*) ;;
  *)
    echo "gate: the pure crate $crate must not exempt anything from Flux, and Flux reports: ${summary:-no summary of it}" >&2
    exit 1
    ;;
  esac
done

cargo clippy --workspace --all-targets --quiet -- -D warnings

# nextest holds each test to the limit .config/nextest.toml gives.
cargo nextest run --workspace --cargo-quiet --status-level fail --final-status-level fail --show-progress none
