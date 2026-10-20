# The group a chat opened is told by its member count and newest messages, not by its name read off the screen

0015 had a leave check the chat it opened by its name: the search box had to hold the display name, the title had to read it with the member count, the chat info's Group Name the group's own name, each exactly once made comparable. macOS's text recognition reads names wrong in ever new ways (full-width parentheses read as ASCII and a long name cut short in the search's list, #71; `—` read as `一` and a text cursor `|` read after the name, #76), and each misreading was found only by a leave stopping on it. So which group is open is no longer told by its name.

## How

- **The name is only searched for.** The chats the search lists are compared with the display name loosely (`fold`: full-width made ASCII, every dash and `一` made `-`, a circled number made its digits, a cursor dropped; a name cut short compared with as much of the name as it holds), and the closest is opened, a quarter of its characters at most read otherwise.
- **The chat is the group when what it shows is what the databases hold of it.** Its title's member count is the group's, and most of the newest messages `wxread messages --chat <id>` holds (up to 8, newest first) are found in the chat, in order, newest at the bottom: a message by a part of its text, one with no text (an image, a sticker) by its sender's name or its minute. At least two must be found, or the one it has. Messages older than the oldest found are above what the chat shows, so not counted against it. When neither decides, nothing is clicked.
- **A chat that is not the group is passed.** Every step looks at the window again until its screen comes, for a few seconds at most; a chat not told to be the group by then is passed for the next chat the search lists, up to four.
- **The confirmation still quotes the name**, compared folded: by then the chat was told to be the group, and the confirmation only has to be its.
- **--dry-run** searches for and opens every group's chat, tells each, and clicks nothing more, so a batch is checked in one run before any is left.

## Considered

- **Fixing each misreading as it is found.** Each new one stops a run, and is found only by trying to leave.
- **The member count alone.** Groups fed the same ads have the same messages, and groups of a name have counts alike (智能通信定位圈—广域物联①群 and ②群 have 500 each); the count tells the first apart, the messages the second, so both must hold.

## Consequences

- A group whose display name another of the account's groups shares is no longer refused before the window is touched: both are listed by the search, and the chat opened is told by what it shows.
- A group whose chat shows none of the newest messages WeChat on this Mac holds (none held, or a chat of images with members' names not shown) cannot be left by wxread: it stops and says what it read.
- Every database is read once a run, before the window is touched; only the membership after OK is read again, every 200 ms.
- The window is held for the whole run: the Mac is waited for to be left alone once (1 s, at most 30 s), and the front given back after the last group.
