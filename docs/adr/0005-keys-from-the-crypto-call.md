# The keys are captured from WeChat's crypto calls with Frida, and proved against each database

Every WeChat database has a key of its own, and `wxread keys` answers it so the rest of wxread can read the databases with the keys the user stored. The question is where the key comes from.

## Why not from memory

wxread first read the keys out of the running WeChat's memory: WeChat for Mac was understood to hold each key as the SQLCipher literal `x'<64 hex key><32 hex salt>'`, so a scan for long hex runs found them. On WeChat for Mac 4.1.x this no longer holds. The key is not resident in the process's memory in any form a scan can find — not as that hex literal, not as the raw 32 bytes, in neither readable-and-writable nor read-only regions, whether WeChat is idle or actively reading chats. What *is* resident is proof the scan works and the databases are open: the per-database salt (raw), and decrypted pages (`SQLite format 3` headers) in the page cache. The 32-byte key appears only for the instant WeChat hands it to CommonCrypto to decrypt a page; CommonCrypto keeps the expanded key schedule, not the master key. So a passive scan cannot find it, and the earlier approach reported nothing on 4.1.x.

## How the key is captured

The one moment the raw key exists is the crypto call, so that is where wxread catches it. `wxread keys` attaches **Frida** to the running WeChat and hooks the three CommonCrypto entry points SQLCipher (through WCDB) drives page crypto through — `CCCrypt`, `CCCryptorCreate`, `CCCryptorCreateWithMode` — and for every AES-256 call (algorithm AES, key length 32) reads the 32-byte key argument. Each distinct key seen is a candidate. The hook and its driver are `crates/cli/src/capture/`.

- **It runs Frida through uv.** wxread does not vendor Frida or bind it in-process; it builds one command line, `uv run --with 'frida<17' python <driver>`, and uv supplies a pinned Frida (`crates/cli/src/capture.rs`, the one place that command is built, per the agent-CLI rules). Frida below 17 is pinned because the CommonCrypto hook fires there; the hook is written to load on 16 and 17 both. uv missing is refused naming `brew install uv`; `wxread doctor` reports whether uv is found.
- **A key is seen only while its database is decrypted.** So the capture runs for a window (`--seconds`, 25 by default) during which the person browses WeChat — opens chats and scrolls, searches, opens contacts, favorites and files — and a database nothing touches yields no key. This is stated in `wxread keys --help` and is why `keys` can be run again: `doctor` names the databases still unkeyed, and a second capture, browsing the parts that hold them, gets them.

## How a key is proved

Capturing is the effect layer's; proving is the pure crate `wxread-crack` (`verify`), checked by Flux like every other pure crate (docs/adr/0002). It never trusts a captured value it has not proved: a candidate is answered for a database only if that database's first-page HMAC recomputes from it — exactly the check `wxread-sqlcipher::opens` makes before any page is decrypted (its SQLCipher 4 layout and HMAC are docs/adr/0003). `keys` uses that one implementation, so a key it answers and a key that decrypts a database are proved the same way. A database no captured key opens is absent from the answer, never guessed; captured candidates that open nothing are the `NOTHING_CAPTURED` line, telling the person to browse and try again. The proof is tested with synthetic encrypted pages built by `wxread-sqlcipher`'s own `encrypt`; no real key or chat is in the tree.

## How macOS and get-task-allow are handled

Frida attaches with the `task_for_pid` port, which needs root and a target that allows being read. A WeChat installed from Tencent carries the hardened runtime without `com.apple.security.get-task-allow`, so the attach is refused even as root. wxread does **not** elevate itself, inject into WeChat, or disable SIP. It refuses, naming the cause and the fix: re-sign a **copy** of WeChat (docs/adr/0008) and capture from that, so the installed `/Applications/WeChat.app` is never modified and keeps its genuine signature. `keys --help` and `resign --help` give the copy recipe (clone, `wxread resign --app <copy> --yes`, run the copy, `sudo wxread keys`). Why wxread re-signs WeChat rather than ask for SIP to be disabled is decided in docs/adr/0008.

## Why keys touches privilege and a third-party tool, and nothing else does

wxread's rule is to answer, never ask, never help itself, and to keep few dependencies, reaching other systems through their own command line (docs/adr/0002, the agent-CLI rules). `keys` is the one command that departs from this, and the domain forces it: the key cannot be obtained any other way on 4.1.x, only root may attach to WeChat, and the attach is Frida's job, not something to reimplement. So `keys` runs under `sudo` (and refuses without root, naming `sudo wxread keys`), and runs Frida through uv (refusing without it, naming its install) — it still never elevates itself, prompts, or installs. Every other command runs unprivileged, with no Frida and no uv, on the keys the user stored. The privileged, third-party step happens once and is visible.

Under `sudo`, the process's `HOME` is root's, but WeChat's databases are the invoking user's; `keys` reads them under the home of `SUDO_USER`.

## Consequences

- `keys` requires root and uv, and refuses without either; `doctor` and every reader run unprivileged with the stored keys, and `doctor` reports whether uv is present.
- A key is reported only once it is proved against its database, so a wrong candidate is never answered and the output is safe to store.
- A database WeChat did not decrypt during the window is not in the answer; browsing it and running `keys` again captures it. This is expected, not a failure.
- The CommonCrypto hook and the `frida<17` pin are WeChat for Mac 4.1.x's; a WeChat that changes how it drives crypto is a new decision, not a silent fix.
- Re-signing WeChat is the user's to do, on a copy, by `wxread resign --app <copy> --yes` (docs/adr/0008) — never something wxread does to WeChat on its own.
