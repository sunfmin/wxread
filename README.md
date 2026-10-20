# wxread

An agent CLI that reads WeChat for Mac 4.x directly: chats, contacts, every
message kind, files and voice. Each command prints one JSON object, and every
failure names its fix. `wxread --help` is the manual.

```sh
wxread doctor              # what it found, and the fix of everything missing
wxread --help              # the guide
```

macOS only, Apache-2.0 ([ADR 0001](docs/adr/0001-macos-only-and-apache-2.md)).
The formats are as [wechat-cli](https://github.com/huohuoer/wechat-cli) and
wechat-decrypt (both Apache-2.0) found them.

## Developing

`./gate.sh` decides whether a change may land, run where the change is made: `cargo fmt
--check`, `./verify.sh` (Flux over the pure crates, which must report `0
trusted; 0 ignored`), `cargo clippy -D warnings` and `cargo nextest run`. It
runs on rustup's `nightly-2026-09-25` and needs the checker Cargo.toml pins:

- cargo-flux, built from the Flux revision pinned there:
  `git clone https://github.com/flux-rs/flux && cd flux && git checkout <rev> && cargo xtask install`
- fixpoint, built at the commit pinned there:
  https://github.com/ucsd-progsys/liquid-fixpoint#how-to-build-and-install
- z3 at the version pinned there: https://github.com/Z3Prover/z3/releases
- cargo-nextest: `cargo install cargo-nextest --locked`

`./verify.sh` names the one to install when one is missing or another build.
Terms are in [GLOSSARY.md](GLOSSARY.md), decisions in [docs/adr](docs/adr).
