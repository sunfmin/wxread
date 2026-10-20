# Screens of WeChat's window, as their text

Each file is one look at WeChat for Mac 4.1.15's window, kept as what
`wxread group leave` reads of it: the window's size (`window`), one line per
text macOS recognizes in its screenshot, where it is, in the window's points
from its top left, and each window of WeChat's over it (`overlay`: the
matches a search lists, a confirmation) with its frame and its own text in
the same points.

- `recorded-*` are the four screens of the run that left the test group
  `wxread 退群测试` (57609122265@chatroom) on 2026-10-20, as they were read.
- `search.jsonl` is the matches a search listed in another run, under a
  Group Chats heading; the others are written from these and from what
  docs/leave-group.md records (a paste that did not land, a name wrapped
  onto two lines, a group of the same name the account has left, the chat
  list level with the confirmation). The `-other` screens are the ones a
  run must stop on.
- `search-truncated.jsonl` is the matches a search listed for
  朱富贵会员群A02（杭州区域） (46264918477@chatroom) on 2026-10-20, as the run
  that stopped on it read them: the name's full-width parentheses read as
  ASCII, and the chat listed by it cut short, its ellipsis read as `.`.
- `search-dash.jsonl` and `chat-dash.jsonl` are the search and the chat
  of 智能通信定位圈—广域物联②群 (5308573596@chatroom) as read on
  2026-10-20: its `—` read as `一` (and `广` as `厂` in the search box), a
  text cursor `|` after the name. The chat's messages and senders are
  written for the test, in the places the real ones were read.
- `chat-truncated.jsonl`, `chat-ads.jsonl` and `chat-images.jsonl` are
  written for the tests: a title with its parentheses read half full-width,
  a chat of ads that another group is fed too, and a chat of images
  only, told by its senders' names and times. `info-chat.jsonl` is
  `info.jsonl` with the chat's message beside it; `info-other.jsonl` is
  the chat info under another group's title.

No screenshot is kept, and none of the owner's chats: the chat list's lines
are left out of every screen.
