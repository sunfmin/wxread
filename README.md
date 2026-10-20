# wxread

An agent CLI that reads WeChat for Mac 4.x directly: chats, contacts, every
message kind, files and voice. Each command prints one JSON object, and every
failure names its fix. `wxread --help` is the manual.

```sh
wxread doctor                               # what it found, and the fix of everything missing
wxread --keys <file> chats                  # the account's chats, newest first
wxread --keys <file> articles               # what official accounts sent, newest first
wxread --keys <file> contacts --query amy   # find a contact, and its id
wxread --keys <file> group <n>@chatroom     # a group's notice and members
wxread --keys <file> messages --chat <id>   # a chat's messages, each as its kind
wxread --keys <file> voice <id> --chat <id> --out v.wav
                                            # a voice message that plays
wxread --help                               # the guide
```

## Install

```sh
brew install sunfmin/tap/wxread
```

`wxread --version` prints the release you have. The keys of WeChat's
databases are the one thing the installer cannot get: `wxread keys --help`
says how.

**For an AI agent**: a skill that sends an agent to `wxread --help` installs
with:

```sh
npx skills add sunfmin/wxread
```

macOS only, Apache-2.0 ([ADR 0001](docs/adr/0001-macos-only-and-apache-2.md)).
The formats are as [wechat-cli](https://github.com/huohuoer/wechat-cli) and
wechat-decrypt (both Apache-2.0) found them. Voice messages are decoded by
silk-decoder-rs (MIT), vendored in [crates/silk](crates/silk)
([ADR 0005](docs/adr/0005-voice-is-decoded-in-process.md)).

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
Each pure crate is listed in `verify.sh`'s `PURE`: a new one is added there,
or Flux never checks it. Terms are in [GLOSSARY.md](GLOSSARY.md), decisions
in [docs/adr](docs/adr).

## Cutting a release

Set `version` in `Cargo.toml`, merge, then push the tag `v<version>`. The
[release workflow](.github/workflows/release.yml) builds both macOS binaries
on stable Rust, makes this repository's release `v<version>` of the two
archives, `SHA256SUMS` and the formula generated from them, and pushes the
formula to [`sunfmin/homebrew-tap`](https://github.com/sunfmin/homebrew-tap)
as `Formula/wxread.rb`. The release needs nothing but the workflow's own
token; the tap needs the repository secret `HOMEBREW_TAP_TOKEN`, a token that
may write the tap's contents, and its step fails without it, naming the line
below that does it by hand.

By hand it is the same steps, on a Mac, with the tag's commit checked out and
from a shell where `gh` and `git` may write this repository and the tap:

```sh
packaging/release.sh check "$(git describe --tags --exact-match)"   # the commit checked out is v<version>
packaging/release.sh build      # both targets, on rustup's stable toolchain
packaging/release.sh package    # dist/wxread-<version>-macos-<arch>.tar.gz, each checked; SHA256SUMS; wxread.rb
packaging/release.sh publish    # release, then tap: the files to the release v<version>, the formula to the tap
```

`publish` is two steps, each of which runs alone too, and again after a
failure: `release` alone makes or refills the GitHub release, and

```sh
packaging/release.sh release    # dist/ to this repository's release v<version>
packaging/release.sh tap        # dist/wxread.rb to the tap: what CI does with HOMEBREW_TAP_TOKEN
```

`build` uses rustup's stable toolchain whatever `cargo` is first on `PATH`,
and names the line that installs it when it, or the Intel target, is
missing. `package` refuses a binary of the wrong arch, or one whose
`--version` is not the release: build at the tag, with no changes.
