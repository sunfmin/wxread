# Decisions in pure crates checked by Flux, effects in one binary

wxread is one cargo workspace: pure crates (`wxread-keys`, `wxread-doctor`, and one for each part of the domain to come) that do no I/O, and one effect crate, the `wxread` binary, that reads the world, asks them what to make of it, and prints the answer. Every function of every pure crate is checked by Flux refinement types, which must report `0 trusted; 0 ignored`: a bound or an order the type states (a count of keyed databases never above the databases there are, a hex digit below sixteen, no arithmetic that overflows) is refused by the checker, not left to a reviewer. Flux, fixpoint and z3 are pinned in Cargo.toml and `verify.sh` refuses any other build of them, since another checker can pass or fail the same tree differently.

## Consequences

- `./gate.sh` runs on the pinned nightly Flux is built for; the release builds on stable, where Flux's attributes expand to nothing.
- What Flux cannot state is tested with proptest; what the binary does is tested by running it.
- The gate runs where the change is made, before it lands; CI does not run it, since a runner would have to build the pinned checker (fixpoint is Haskell) from source.
