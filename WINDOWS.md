# wxread on Windows

This port adds Windows support to the wxread CLI. It reads **WeChat for
Windows 4.x** directly from `xwechat_files`, using the same SQLCipher 4
decryption and the same database schema as the macOS build. Like the original,
it prints one JSON object per command, writes no decrypted bytes, and never
changes WeChat's files.

The upstream project was macOS-only; this change adds Windows without changing
the macOS path.

## What works

| Area | Status |
| --- | --- |
| Data-directory discovery | `%USERPROFILE%\xwechat_files`, falling back to `%USERPROFILE%\Documents\xwechat_files` |
| `doctor` | Finds accounts, databases, and checks every supplied key |
| `chats`, `contacts`, `contact`, `group` | Reads `session`, `contact`, `bizchat` databases |
| `messages`, `search` | Reads `message_*`, `biz_message_*`, and the FTS index |
| `articles`, `favorites`, `moments` | Reads the Windows 4.x layout (`biz_message`, `favorite`, `sns`) |
| `file get`, `avatar`, `voice` | Writes only the file named by `--out` |
| Key capture (`wxread keys`) | Not ported: supply keys instead |
| `resign` | macOS only |
| `group leave` | macOS only (drives WeChat for Mac's window) |

Windows WeChat stores each account at
`%USERPROFILE%\xwechat_files\<wxid>_<suffix>\db_storage`, which is the same
`db_storage` layout the reader already understands. `%USERPROFILE%` is used
unless `HOME` is set (Git Bash/MSYS2), and `--data-dir`/`WXREAD_DATA_DIR`
always win.

## Build

Requirements:

- Rust 1.99+ with the **GNU** Windows host toolchain
  (`stable-x86_64-pc-windows-gnu`), not MSVC.
- A MinGW-w64 GCC for the bundled SQLite C build, for example MSYS2's
  `mingw-w64-x86_64-gcc`.
- `git` on `PATH` if you build from a checkout (the version stamp uses it).

In PowerShell, with MSYS2 installed at `C:\msys64`:

```powershell
$env:Path = "C:\msys64\mingw64\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
rustup default stable-x86_64-pc-windows-gnu
cargo build --release
.\target\release\wxread.exe --version
```

The resulting `target\release\wxread.exe` needs the MinGW runtime DLLs
(`libgcc_s_seh-1.dll`, `libwinpthread-1.dll`) on `PATH` or beside it. To make a
standalone folder, copy those two DLLs from `C:\msys64\mingw64\bin`.

If the machine restricts writes to `%TEMP%`, point the build at a writable
directory:

```powershell
$env:TMP = "$PWD\.tmp"; $env:TEMP = $env:TMP
```

## Keys

This build does not attach to WeChat's process. Extract the account's keys with
a Windows WCDB/SQLCipher key tool for WeChat 4.x (for example
[wcdb-key-tool](https://github.com/TANGandXUE/wcdb-key-tool)), then supply its
`all_keys.json`:

```powershell
.\wxread.exe --keys .\all_keys.json doctor
.\wxread.exe --keys .\all_keys.json chats
```

The file is one JSON object of each database's path under `db_storage` to its
key, exactly the shape the macOS build produces:

```json
{"message/message_0.db": {"enc_key": "<64 hex>", "salt": "<32 hex>"}}
```

The salt may be omitted. `--keys` beats `WXREAD_KEYS`; `WXREAD_DATA_DIR` can
name the data directory instead of `--data-dir`.

Keys are read into memory, used to decrypt pages into memory, and never
written back. The only file wxread writes is the `--out` path of `file get`,
`avatar`, or `voice`.

## DSH skill

A ready-made DSH skill for reading WeChat lives at
[`skills/wechat-read-windows/SKILL.md`](skills/wechat-read-windows/SKILL.md).
It walks an agent through locating the executable and keys, picking the active
account, listing chats, reading/paging messages, searching, and what to do when
keys are stale.

Install it for the current Windows user:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.dsh\skills\wechat-read" | Out-Null
Copy-Item .\skills\wechat-read-windows\SKILL.md "$env:USERPROFILE\.dsh\skills\wechat-read\SKILL.md"
```

DSH discovers `%USERPROFILE%\.dsh\skills\<name>\SKILL.md` automatically; the
skill's name is `wechat-read`.

## Tests

The macOS CLI suite is compiled only on Unix (`#![cfg(unix)]`). The Windows
port has its own smoke tests, which build a synthetic encrypted account and run
the real binary:

```powershell
cargo test --test windows
```

The pure crates' unit tests, the SQLCipher tests, and the Windows target all
run with `cargo test`.

## How the port differs

- `crates/cli/src/world.rs`: a Windows home (`HOME`, else `USERPROFILE`, else
  `HOMEDRIVE`+`HOMEPATH`) and a `default_data_dir` that probes the Windows
  layouts. The Unix `getpwnam` path is macOS-only.
- `crates/doctor/src/lib.rs`: platform-specific data-directory constant,
  WeChat name, key guidance, and `Full Disk Access` replaced by a Windows
  permission hint.
- `crates/cli/src/doctor.rs`: `permissions` is `{}` off macOS, since the
  Accessibility/Screen Recording grants are macOS concepts.
- `crates/cli/src/capture.rs` and `resign.rs`: the macOS capture/re-sign paths
  are compiled out; the commands explain the macOS-only behavior instead.
- `crates/cli/src/help-windows.txt`, `short-windows.txt`, and the
  `help/*-windows.txt` files: Windows help that names `%USERPROFILE%` and key
  extraction.
- The Flux annotations are kept, and the Windows build uses the same pinned
  `flux-rs` git dependency as upstream. A plain `cargo build` erases the
  annotations; `./verify.sh` still checks the same crates on macOS. Building on
  a network that blocks GitHub needs a Cargo mirror for that one git
  dependency, as upstream already does.
- `crates/cli/tests/synthetic/mod.rs`: JSON-escapes the Windows path in the
  `_db_dir` note of a keys file.
