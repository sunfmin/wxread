# The keys are read from the running WeChat's memory, with the one privilege wxread touches

Every WeChat database has a key of its own, and that key exists in only one place: the memory of the running WeChat. WeChat for Mac 4.x does not derive the key from a passphrase at open time; it holds the raw 32-byte SQLCipher key in its own memory, written as the literal `x'<64 hex of the key><32 hex of the database's salt>'`. So `wxread keys` reads the keys out of that memory, with no third-party tool, and the rest of wxread reads databases with the keys the user stored from it. The formats are as [wechat-cli](https://github.com/huohuoer/wechat-cli) and wechat-decrypt (both Apache-2.0) found them.

## How a key is found and proved

The key search is a pure function over bytes (`wxread-crack`), checked by Flux like every other pure crate (docs/adr/0002): the effect crate hands it the bytes of WeChat's memory and the first page of each database, and it answers which key opens which database. It never trusts a candidate it has not proved:

- **Find.** Every run of 64 or more hex digits in the memory spells a candidate key in its first 64 digits (WeChat's literal is `x'…'`, whose `x` and `'` bound the run).
- **Prove.** A candidate opens a database only if that database's first-page HMAC recomputes from it — exactly the check `wxread-sqlcipher::opens` makes before any page is decrypted, whose SQLCipher 4 layout and HMAC are decided in docs/adr/0003. `keys` uses that one implementation rather than its own, so a key it answers and a key that decrypts a database are proved the same way.

So every key `wxread keys` answers opens its database, and a database no key in memory opens is simply absent from the answer — never guessed. The search is tested with synthetic memory and with synthetic encrypted pages built by `wxread-sqlcipher`'s own `encrypt`; no real key or chat is in the tree.

## How macOS is handled

Reading another process's memory on macOS needs the `task_for_pid` port, which needs two things: root, and a target that allows being read. A WeChat installed from Tencent carries the hardened runtime without the `com.apple.security.get-task-allow` entitlement, so `task_for_pid` fails even as root. wxread does **not** elevate itself, inject into WeChat, or disable SIP. It refuses, naming the cause and the fix: `wxread resign --yes`, which re-signs WeChat ad-hoc with `get-task-allow` added to its existing entitlements (docs/adr/0006), then quit WeChat, open it again, grant the app running wxread Full Disk Access, and run `sudo wxread keys` again. Why wxread re-signs WeChat rather than ask for SIP to be disabled is decided there.

## Why keys touches privilege, and nothing else does

wxread's rule is to answer, never ask, never help itself — prompt for nothing, elevate nothing (docs/adr/0002, the agent-CLI rules). `keys` is the one exception the domain forces: the keys cannot be read any other way, and only root can read them. So `keys` must run under `sudo`, and run without root it refuses with the exact line, `sudo wxread keys` — it still never elevates itself, prompts, or installs. Every other command runs unprivileged on the keys the user stored, so the privileged step happens once and is visible.

Under `sudo`, the process's `HOME` is root's, but WeChat's databases are the invoking user's; `keys` reads them under the home of `SUDO_USER`, so `sudo wxread keys` finds the right account without a flag.

## Consequences

- `keys` requires root and refuses without it; `doctor` and every reader run unprivileged with the stored keys.
- A key is reported only once it is proved against its database, so a wrong candidate is never answered and the output is safe to trust and store.
- The crypto parameters are WeChat for Mac 4.x's; a WeChat that changes them (a new major version, a new cipher) makes this command report nothing, and is a new decision, not a silent fix.
- Re-signing WeChat is the user's to do, by `wxread resign --yes` (docs/adr/0006) — never something wxread does to WeChat on its own.
