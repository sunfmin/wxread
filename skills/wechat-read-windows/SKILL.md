---
name: wechat-read
description: Read the user's own WeChat for Windows 4.x data — chats, messages of every kind, contacts, favorites, Moments and media — through the wxread CLI and a database-keys file, then search or summarize what was read. Use when the user asks to 读取微信消息/聊天记录, 查看微信群消息, search or export WeChat messages, summarize a WeChat chat or group, list WeChat chats, or mentions wxread or WeChat on Windows.
whenToUse: The user wants their own WeChat for Windows 4.x chats or messages listed, read, searched or summarized. Not for WeChat for Mac, not for sending messages, and not for leaving groups.
---

# Read WeChat on Windows with wxread

`wxread` reads WeChat for Windows 4.x directly from
`%USERPROFILE%\xwechat_files`. It decrypts each database in memory and writes
nothing except an explicit media `--out` path. Never send messages, leave
groups, re-sign WeChat, or modify its databases.

## 1. Locate the executable and the keys

Run in PowerShell from the workspace:

```powershell
$WXREAD = if ($env:WXREAD_EXE) { $env:WXREAD_EXE } else {
  @(
    ".\target\release\wxread.exe",
    ".\src\wxread-main\target\release\wxread.exe",
    ".\dist\wxread-0.1.6-windows-x86_64\wxread.exe"
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $WXREAD) {
  $WXREAD = (Get-ChildItem -Recurse -Filter wxread.exe -ErrorAction SilentlyContinue |
    Select-Object -First 1).FullName
}
if (-not $WXREAD) { throw "wxread.exe not found: build it or set WXREAD_EXE" }
```

Keys are a JSON file in `all_keys.json` shape (`{ "<db path>": {"enc_key": ..., "salt": ...} }`).
Find one, or use `WXREAD_KEYS` (the JSON itself, not a path):

```powershell
$KEYS = $env:WXREAD_KEYS_FILE
if (-not $KEYS) {
  foreach ($p in @(".\tools\wxread-keys.json", ".\all_keys.json")) {
    if (Test-Path $p) { $KEYS = (Resolve-Path $p).Path; break }
  }
}
$keyArgs = @(); if ($KEYS) { $keyArgs = @("--keys", $KEYS) }
```

`--keys` beats `WXREAD_KEYS`; `--data-dir` (the parent `xwechat_files`, not an
account directory) beats `WXREAD_DATA_DIR`. Never print, commit, or paste the
keys file; it is a secret.

## 2. Check WeChat is readable

```powershell
$doc = (& $WXREAD @keyArgs doctor | ConvertFrom-Json)
$doc.data_dir
$doc.accounts | Format-Table id, db_storage, databases, keyed
$doc.findings
```

Good state: the account has `keyed == databases`, and `findings` has nothing
about it. With several accounts, choose the one that is fully keyed and whose
`db_storage` was written most recently:

```powershell
$account = $doc.accounts |
  Where-Object { $_.databases -gt 0 -and $_.keyed -eq $_.databases } |
  Sort-Object { (Get-Item $_.db_storage).LastWriteTime } -Descending |
  Select-Object -First 1
if (-not $account) { throw "No fully-keyed account: extract keys first (section 7)" }
$acctArgs = @("--account", $account.id)
```

An old, unused account may appear with unkeyed databases. That is not the
current account; do not treat its finding as a failure of the current one.

## 3. List the chats

```powershell
$chats = (& $WXREAD @keyArgs chats @acctArgs --limit 50 | ConvertFrom-Json)
$chats.account
$chats.chats | Format-Table id, display_name, kind, last_message_at, unread
$chats.next
```

`kind` is `direct`, `group` or `folder`. A folder (`brandsessionholder`,
`brandservicesessionholder`) holds official-account articles; use
`wxread articles` for it, not `messages`.

Find a chat by name:

```powershell
& $WXREAD @keyArgs chats @acctArgs --query "关键词" --limit 20
```

## 4. Read one chat's messages

```powershell
$msgs = (& $WXREAD @keyArgs messages @acctArgs --chat "<chat-id>" --limit 30 |
  ConvertFrom-Json)
$msgs.chat
$msgs.messages | Format-Table at, kind, @{n="sender";e={$_.sender.display_name}}
$msgs.next
```

Each message has `id`, `at` (UTC), `kind`, `sender {id, display_name}`,
`content` (WeChat's raw payload) and parsed fields for its kind: `text.text`,
`link {title, url, summary, source}`, `file`, `image`, `voice`, `quote`,
`forward`, and so on. Prefer the parsed field; fall back to `content` only
when the kind has no parsed field.

Paging: pass the previous answer's `next` as `--cursor <next>`; `null` means
the end. `--limit` accepts 1–1000.

Times are UTC. Convert for the user:

```powershell
[datetime]::Parse($m.at).ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss")
```

## 5. Search across chats

```powershell
$found = (& $WXREAD @keyArgs search @acctArgs "关键词" --limit 20 | ConvertFrom-Json)
$found.searched          # "wechat_index" when the FTS index answered
$found.messages | Format-Table at, @{n="chat";e={$_.chat.display_name}},
  @{n="sender";e={$_.sender.display_name}}, kind
```

Search reads WeChat's own index; a message not in the index may still be
readable through `messages`. Quote words containing spaces.

## 6. Other read-only commands

```powershell
& $WXREAD @keyArgs @acctArgs contacts --limit 50
& $WXREAD @keyArgs @acctArgs contacts --query "关键词"
& $WXREAD @keyArgs @acctArgs contact "<id>"
& $WXREAD @keyArgs @acctArgs group "<n>@chatroom"
& $WXREAD @keyArgs @acctArgs articles --limit 20
& $WXREAD @keyArgs @acctArgs favorites --limit 20
& $WXREAD @keyArgs @acctArgs moments --limit 20
& $WXREAD @keyArgs @acctArgs file get "<id>" --out "<path>"
& $WXREAD @keyArgs @acctArgs avatar "<id>" --out "<path>"
& $WXREAD @keyArgs @acctArgs voice "<id>" --chat "<chat-id>" --out "<path>.wav"
```

`file get`, `avatar` and `voice` are the only commands that write, and only to
the named `--out`. Ask before writing outside a temporary/workspace path.

## 7. If the keys are missing or stale

`wxread keys` is macOS-only; do not run it here. On Windows, extract the keys
from the running WeChat with a WCDB/SQLCipher key tool while WeChat is open and
logged in. In this workspace, `wcdb_key_tool_windows.py` has been tested.

The `python` on `PATH` may be the Microsoft Store alias; choose a real 64-bit
Python 3.10+ first:

```powershell
$PY = $env:WXREAD_PYTHON
if (-not $PY) {
  $PY = @(
    "$env:USERPROFILE\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe",
    "C:\Python312\python.exe",
    "C:\Python311\python.exe"
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $PY) { $PY = "python" }
& $PY -c "import struct,sys; print(sys.version); assert struct.calcsize('P')*8 == 64"
```

Then, as Administrator:

```powershell
& $PY .\tools\wcdb_key_tool_windows.py extract `
  --db-dir "$env:USERPROFILE\xwechat_files\<account>\db_storage" `
  --output .\tools\wxread-keys.json
```

It prints `N/N salts` and writes `all_keys.json`-shaped JSON. Then rerun
`doctor`; every database of the active account should be keyed. Extract only
for the user's own account and only when they asked to read it. Keys expire
when WeChat recreates its databases (logout/login or some updates); rerun the
extractor and `doctor`.

## 8. Pitfalls

- Pass `--data-dir` the **parent** `xwechat_files`; the account directory is not
  a data directory.
- Read commands take `--account <id>`; `doctor` does not.
- WeChat must be running to extract keys, but reading works from the files
  afterwards.
- Chinese text through a PowerShell console may look garbled even when the JSON
  is valid. Write output to a file with `Out-File -Encoding utf8` and read it,
  or parse it with Python; do not trust the console rendering.
- `messages` for an official account can return `mmreader` XML. Prefer
  `articles` for official-account pushes, and parsed `link`/`text` fields over
  raw `content`.
- Do not use `group leave`, `resign`, or `keys` on Windows: they are macOS-only
  and answer with a fix.

## 9. Reporting

Read only what the user asked for. Summarize instead of dumping a whole chat;
keep message IDs, chat IDs and UTC/local times when quoting. Never include the
keys file's contents in an answer, a commit, or a log. If a message is a link
and the user asks for its content, fetch the link separately and summarize it;
WeChat article links may require environment verification.
