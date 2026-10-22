# 0018: Windows reads WeChat 4.x with supplied keys

Date: 2026-10-21
Status: Accepted

## Context

The upstream CLI is macOS-only because it reads WeChat for Mac 4.x's
`xwechat_files` and captures keys from the running Mac process (ADR 0001).
WeChat for Windows 4.x keeps the same account layout under
`%USERPROFILE%\xwechat_files`: each account directory holds a `db_storage`
with the same SQLCipher 4 databases and WAL. Its schema is the same family the
pure crates already read, and decryption is already done in memory
(ADR 0003).

Key capture is the part that does not transfer. The Mac build hooks
CommonCrypto during a page decrypt; Windows WeChat 4.1+ no longer keeps the raw
32-byte key in process memory, only a WCDB passphrase object from which each
database's key is derived. Reproducing that capture needs version-specific
reverse engineering and would make the Windows build depend on a moving
internal.

## Decision

Build the same CLI for Windows and read the Windows `xwechat_files`:

- `%USERPROFILE%\xwechat_files` is the default data directory, with
  `%USERPROFILE%\Documents\xwechat_files` as the fallback some installers use;
  `HOME` is honoured first for Git Bash/MSYS2, and `--data-dir` /
  `WXREAD_DATA_DIR` always win.
- Keys are supplied with `--keys <file>` (the `all_keys.json` shape) or
  `WXREAD_KEYS`. `doctor` verifies each key against each database's first-page
  HMAC, exactly as on macOS.
- Decryption, WAL replay, schema reading, paging, media writing and the
  read-only guarantees are the existing crates, unchanged. No decrypted byte
  is written; only `--out` is written.
- `wxread keys`, `resign` and `group leave` stay macOS-only and answer with a
  fix that names the Windows alternative. `permissions` is `{}` because
  Accessibility and Screen Recording are macOS grants.

The Windows build uses stable Rust and the same pinned Flux dependency as
macOS. A plain `cargo build` erases the annotations; `./verify.sh` still runs
the checker over the same crates on macOS. No Windows-only dependency
substitution is introduced, so verification is unchanged.

## Consequences

- A Windows user needs an external Windows WCDB/SQLCipher key tool, then
  `--keys` on every command. `WINDOWS.md` names one and the file shape.
- No group leaving, re-signing, or key capture on Windows. Reading is the
  supported surface; it is also the part that never changes WeChat's files.
- The Windows port has its own smoke tests (`cargo test --test windows`) built
  on a synthetic encrypted account; the macOS CLI suite remains
  `#![cfg(unix)]`.
- If WeChat changes the Windows database schema or key derivation, the reader
  either reports the missing columns as a finding or an unknown message kind;
  it does not write to the database. Supporting a new schema is a new change,
  not a new build target.
