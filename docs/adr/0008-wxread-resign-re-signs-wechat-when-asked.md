# `wxread resign` re-signs WeChat with get-task-allow, only when told to

`sudo wxread keys` reads the keys out of the running WeChat's memory (docs/adr/0005), and macOS lets even root take the `task_for_pid` port of a process only when its signature allows it. A WeChat installed from Tencent is signed with the hardened runtime and without `com.apple.security.get-task-allow`, so `keys` is refused. `wxread resign` closes that gap: it re-signs the WeChat app ad-hoc (`codesign --force --sign - --entitlements <them> <app>`) with the entitlements it is signed with now plus `get-task-allow`, and nothing else.

## How

- **It changes the app only with `--yes`.** Without it, `wxread resign` asks codesign what the app is signed with and answers what it would do: the entitlements it keeps, the one it adds, and the line that does it. With `--yes` it re-signs, reads the signature again, and answers what changed. wxread prompts for nothing, so the go-ahead is a flag, not a question.
- **It keeps every entitlement WeChat has.** The entitlements are read with `codesign -d --entitlements - --xml`, and the plist signed with is that one with `<key>com.apple.security.get-task-allow</key><true/>` added to its top dict (or a `false` one turned `true`). Dropping the others, as a bare recipe does, takes away what WeChat relies on (its sandbox, its app group, JIT). The decision is the pure crate `wxread-resign`, checked by Flux (docs/adr/0002); the binary only runs codesign. What it answers it changed is what codesign reports afterwards, not what it meant to do.
- **Running it twice is running it once.** An app that carries `get-task-allow` already is not signed again.
- **The app is WeChat where it is installed**, `/Applications/WeChat.app` else `/Applications/Weixin.app`, or the app `--app` names (a copy the user made, say). It is re-signed in place: a second copy would be a second WeChat for the user to keep apart, and reinstalling undoes the one in place.
- **It elevates nothing.** An app macOS will not let the user change is refused naming `sudo wxread resign --yes`, and App Management for the app running wxread; codesign missing is refused naming `xcode-select --install`.
- **What it cannot do for the user it names**, in the answer's `next` and in `--help`: quit WeChat and open it again (a running WeChat keeps the signature it started with), grant the app running wxread Full Disk Access, then `sudo wxread keys`; and that re-signing can stop WeChat updating itself, undone by reinstalling it from Tencent.

## Why re-sign WeChat, and not ask for SIP to be disabled

Turning System Integrity Protection off would let root read WeChat as it is, but it weakens the whole machine, for every process, until it is turned back on, and it takes a reboot into recovery each way. Re-signing is scoped to the one app whose memory is needed, changes nothing WeChat does, and is undone by reinstalling WeChat. So wxread re-signs WeChat, and never asks for SIP to be turned off.

## Consequences

- `resign` is the one command that changes anything of WeChat's (its app's signature, never its data), and only with `--yes`, run by the person whose Mac it is.
- After a WeChat update (which a reinstall is), WeChat is Tencent's signed build again, and `wxread resign` is run again.
- Its tests run the real codesign on a synthetic app signed the way Tencent signs WeChat; no WeChat is touched by them. That a re-signed WeChat then lets `sudo wxread keys` read it is checked on a real Mac by hand, not by the gate.
