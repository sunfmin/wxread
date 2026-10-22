# macOS only, under Apache-2.0

Status: superseded in part by [ADR 0018](0018-windows-reads-with-supplied-keys.md),
which adds reading WeChat for Windows 4.x with supplied keys; the macOS-only
key capture, re-sign and group-leave decisions remain.

wxread reads WeChat for Mac 4.x's own databases and files, which exist only on a Mac where WeChat is logged in, so it is built and released for macOS alone (arm64 and x86_64, through Homebrew): the one departure from the five platforms (macOS, Linux and Windows) every other agent CLI here is released for. A Linux or Windows build would have nothing to read, and WeChat for Windows keeps its data in another layout under other keys. It is licensed Apache-2.0, the license of wechat-cli and wechat-decrypt, whose findings about the formats it builds on and credits.

## Consequences

- No Scoop manifest and no Linux or Windows job in the release; the tests run on macOS too.
- Supporting WeChat for Windows would be a new decision, not a new build target.
