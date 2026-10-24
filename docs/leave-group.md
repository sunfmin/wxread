# The path to leave a group through WeChat's window

How a group is left in WeChat for Mac 4.1.15 (build 4066647863) by doing what a
person does in its window: a key, a menu, a screenshot, text recognized in it,
a click. The decision to take this path is docs/adr/0015; this is the path itself,
as `docs/leave-group/wxwin.swift` runs it:

```sh
swiftc -O docs/leave-group/wxwin.swift -o wxwin
WXREAD_KEYS="$(cat <keys>)" ./wxwin run <n>@chatroom --out <dir>   # up to the confirmation, cancelled
WXREAD_KEYS="$(cat <keys>)" ./wxwin run <n>@chatroom --confirm     # OK, only on "wxread 退群测试"
```

It answers one JSON line per step and saves every screenshot in `--out`. It is a
prototype: wxread does not ship it. `wxread group leave <id> --yes` is the path
built into wxread (crates/leave decides it, crates/cli/src/window.rs drives the
window); where building it found the path otherwise than the prototype did, the
steps below say so, and [Built into wxread](#built-into-wxread) records the runs. Each screen is described below by the text
recognized on it; the screenshots themselves are not kept (they are the owner's
chat list).

WeChat 4.x draws its window itself (Qt): Accessibility sees its menus and
nothing inside the window. A menu is pressed through Accessibility; everything
inside the window is found by macOS's text recognition (Vision,
`VNRecognizeTextRequest`, accurate, `zh-Hans` and `en-US`) on a screenshot of the
window (`screencapture -x -o -l <window id>`), in the window's points. Recognized
text is compared with spaces and quotes removed and full-width punctuation,
letters and digits made ASCII (`【】` made `[]`): Vision reads `wxread 退群测试`
as `wxread退群测试`, `（园）` as `(园)`, and `朱富贵会员群A02（杭州区域）` as
`朱富贵会员群A02(杭州区域)`.
WeChat on this Mac shows English; each label below is given in English and in
Chinese.

## Before anything: which group

wxread answers the group (`wxread group <id>`): its display name, its own
name, its members, and its newest messages (`wxread messages --chat <id>`).
The prototype then told the chat it opened by its name: the title had to read
`<display name> (<members>)`, the chat info's **Group Name** the group's own
name, and two groups of the same display name were refused. Built into
wxread, macOS's reading of names proved too unreliable for that (#71, #76),
and the chat is told by what it shows instead
(docs/adr/0017-the-group-opened-is-told-by-its-member-count-and-newest-messages.md):
its title's member count, and its newest messages against those the
databases hold. See [Told by what it shows](#told-by-what-it-shows). The
confirmation must still quote the display name, compared folded.

## The path

Each step: how its target is found, how the next screen is told, what is seen
when it fails.

**0. The person is not using the Mac.** `HIDIdleTime` (IOHIDSystem) of 3 seconds
or more, waited for at most a minute. The window takes the front for about ten
seconds; a person typing meanwhile types into WeChat, and their keys and the
prototype's interleave. Failing: `the Mac was in use for a minute: nothing was
touched`.

**1. WeChat to the front.** Accessibility's `AXFrontmost` set on WeChat, then read
back until it is true. `NSRunningApplication.activate()` from a process in the
background was not always honored, and its `isActive` is never updated in a
process with no run loop, so neither is used. A click on an inactive window only
activates it: before every click the prototype makes WeChat the front again.
Failing: `WeChat did not become the active app`. The app that was in front gets
it back at the end.

**2. Search by the display name.** Escape (closes what was open), then the menu
Edit > Search (⌘F; pressed through Accessibility), then the name put on the
pasteboard and Edit > Paste pressed; the pasteboard is put back after. Typing
does not work: with a Chinese input source, typed letters stay in composition
(`w'x're'a'd`), and synthetic key events carrying Unicode text fill the field
without WeChat searching.

Seen: the search box, outlined green, holding `wxread 退群测试` and a clear button.

The search box is told by the text recognized at the top of the chat list
(y < 40, x < 330): it must contain the name (Vision reads the magnifier before it
and the clear button after it, as `、` or `1`). Failing: `the search box does not
hold the name: Return could send it into a chat`; seen in two of fourteen runs
on real groups, when the paste did not land.

**3. Return opens the first match.** WeChat 4.1.15 shows no list of matches after
a paste, in its window or any other; Return opens the first. (Built into wxread,
this was found otherwise: the matches are listed, in a window of WeChat's own
over the main one, and Return opened WeChat's search of the internet. wxread
clicks a chat that list names; see [Built into wxread](#built-into-wxread).) The chat is told by
its title (y < 40, right of the list): `<display name> (<members>)`.

Seen: the title `wxread 退群测试 (1)`, and right of it the chat bubble, call and `···` buttons.

A minimized group (折叠的群聊) opens inside the Minimized Groups list, the same
way. Failing: `the opened chat is not the group asked for`, with the title read.

**4. The chat info.** The `···` at the title row's right end, 34 points from the
window's right edge and 27 from its top: it has no text, so it is found by
position, and the click is told to have worked by the panel's **Group Name**
(群聊名称) label right of the chat. The `···` toggles the panel, so it is not
clicked when the panel is already open. Failing: `the chat info did not open`.

Seen, top to bottom in a panel right of the chat: Search, the members (孙凤民, Add), Group Name `wxread 退群测试`, Group Notice, Remark, My Alias in Group, Group QR code, Search Chat History, Joining requires approval, Mute Notifications, Pin to Top, Save to Contacts, Display Members' Nicknames, then in red Clear Chat History and **Leave**. A real group adds Minimize Group and Members I Follow.

**5. Leave (退出群聊).** The red text at the panel's foot, recognized at x > window
width − 300. Groups of 1, 106 and 356 members showed it without scrolling (the
member grid stops at four rows and Show All); when it is not seen, the panel is
scrolled (scroll-wheel events over it) up to eight times. No group needed the
scroll, so it is untried. Before the click, the title is read again. Failing: `no
Leave in the chat info`.

**6. The confirmation.** A window of its own over WeChat's (280 × 194 points),
found in the window list; its text is the part of the screenshot inside it.

`You will leave the group chat` (WeChat in Chinese: 退出群聊), the name quoted on
the lines below it down to the **Clear Chat History** box (a 26-character name
wraps to two lines), and the buttons **Cancel** (取消) and **OK** (确定), each
found by its text. The box is left unticked: the chat's messages stay. Failing:
`no confirmation came up`, or `the confirmation's buttons were not read`.

**7. Cancel or OK.** On any group but `wxread 退群测试`, Cancel; the confirmation's
window is then gone and its text no longer recognized. On the test group, with
`--confirm`, OK.

## After: what shows the account has left

Read with wxread from the databases, each second after OK, on `wxread 退群测试`
(`58872695997@chatroom`, a group the owner made, with only the account in it):

| after OK | what wxread answered |
|----------|----------------------|
| 1 s      | the chat's newest message is a notice, `You've left the group chat`, `type` 10000, sent by the account, with no server id |
| 3 s      | `wxread group` answers no members (the account had been the only one); `wxread chats` answers the chat with `members: 0` |
| after    | the chat is still in `wxread chats`, and the group still in `wxread contacts`, with its name; its owner is still the account |

So the leave is told by **the account's id no longer being among the group's
members**, which takes seconds. The notice's text is in WeChat's language, so it
is not what tells. In the window, the title loses its count (`wxread 退群测试`), the
chat info holds only Search Chat History, Mute Notifications, Pin to Top and
**Delete Chat**, and the message box says `Unable to send message in an exited
group`.

Not verified: the account was this group's owner and its only member. Leaving a
group that has others in it, whether the other members are kept in the
account's copy of the list, and whether an owner leaving a group with members
is asked to hand it over first, were not seen.

## The permissions

The app that runs the path (the terminal, or the agent's host) needs both:

- **Accessibility**, to press menus and send keys and clicks. Without it,
  `AXIsProcessTrusted()` answers false, and macOS lists the app under Privacy &
  Security > Accessibility, switched off. The prototype checks it first and
  refuses (`Accessibility is not allowed for this app: keys and clicks would be
  dropped`), since keys and clicks sent without it are dropped without an error.
- **Screen Recording**, to take the window's screenshot. Without it,
  `screencapture -l <window id>` answers `could not create image from window` and
  exits 1, and macOS asks the person whether to allow the app to record the
  screen.

Both were seen by running the prototype from launchd, where no app that holds
them is responsible for it.

## The runs

On 2026-10-20, on this account. Two real groups, both minimized: **A**, 106
members, and **B**, 356 members, with a 26-character name that wraps in the
confirmation. Every run on a real group stopped at the confirmation and pressed
Cancel, or refused before it.

| group | runs that reached the confirmation and cancelled | refused, and why |
|-------|-----------------------------------|------------------|
| A     | 4                                 | 1: the Mac was in use for a minute |
| B     | 5 (in two, the name read did not match, from the bugs fixed below, and the run cancelled) | 4: paste did not land (2), the clear button read as `1` (1), the window not active, so the info did not open (1) |
| `wxread 退群测试` | 2, then 1 confirmed      | — |

The last three runs on real groups, with the prototype as committed: B
cancelled, A cancelled, B cancelled, 23 to 31 seconds each, about ten of them
spent reading every contact before the window is touched. After them, `wxread group` still
answered the account among the members of both A (106) and B (356). Every
refusal touched nothing after the step it names. The bugs those runs found are
fixed in the prototype: the name is read across the lines it wraps to, only
inside the confirmation's own window, and WeChat is made the front by
Accessibility and checked.

## What the path changes besides

- The group's chat is opened, so its unread messages are marked read, as a
  person opening it would.
- The chat is left open, with its chat info.
- The pasteboard holds the name for a fraction of a second, and is put back.

## Built into wxread

What `wxread group leave` does where the prototype's path did not hold, found
on 2026-10-20 on this account, WeChat 4.1.15 in English, on a new test group
`wxread 退群测试` (57609122265@chatroom), with the old one of that name
(58872695997@chatroom) left before. Every run that did not leave stopped
before Leave was clicked, or cancelled the confirmation; the screens of the run
that left are kept, as their text, in crates/leave/screens/recorded-*.

- **The matches are a window of their own.** After the paste, WeChat lists the
  matches in a window over the main one (layer 3, 368 points wide), which a
  screenshot of the main window does not hold. Return opened WeChat's search of
  the internet (a window of its own, its title `Search`), not the chat. The list
  read: `Most used`, the group, `Group Chats`, the group, `Internet search
  results` and searches of the internet; in the next search, both groups of the
  name under `Most used` and no `Group Chats`. So headings are not relied on: a
  chat listed is a line that reads the name, indented past its avatar
  (35 points or more from the list's left edge; the searches of the internet
  start nearer). A name too long for the list is cut short there, its
  ellipsis read as `.`, `...` or `…`: 朱富贵会员群A02（杭州区域）
  (46264918477@chatroom) was listed as `朱富贵会员群A02(杭州区.`, and a run
  stopped on it. So a line that reads a start of the name before an ellipsis
  is listed by the name too, after the lines that read it whole; the chat's
  title, its Group Name and the confirmation still tell whether it is the
  group. wxread clicks the first, and when its chat's title is the name
  alone (a group the account has left) searches again and clicks the next, at
  most four chats in all. Any other title stops the run.
- **WeChat's other windows.** WeChat had a file open in a window of its own
  (layer 0), and with it in front Edit › Search was not in the menus. wxread
  takes the main window by its name (`Weixin`, `WeChat`, `微信`), and puts it in
  front of WeChat's others (Accessibility's `AXRaise` and `AXMain`) after making
  WeChat the active app.
- **The confirmation's own window is not shot alone.** `screencapture -l` of
  it gave all of WeChat's window squeezed into its 280 × 194 points; its text
  was read, as the prototype read it, in the main window's screenshot inside
  its frame. wxread reads a window's own screenshot only when its pixels are
  the same multiple of its points across and down.
- **The leave.** The run that left took 12.8 s from start to answer; the
  databases no longer listed the account 3 s after OK, and the chat's newest
  message was the notice `You've left the group chat`. `wxread group` answered
  `members: []`.

## Told by what it shows

How `wxread group leave` tells the chat it opened is the group, found on
2026-10-20 on this account, WeChat 4.1.15 in English, on 17 groups the
account was in (the run's `--dry-run`, then `--yes` on one).

- **The title.** The member count is the digits in the title's last
  parentheses, read whatever the name before it reads as: `团结友爱AITO问界车友群18（500）`
  for 团结友爱AITO问界车友群⑱, `智能通信定位圈一广域物联②群（500）|` with a
  text cursor after it. Every one of the 17 counts was read right.
- **The messages.** The chat opens at its bottom, its newest messages
  there, each under its sender's name (with Display Members' Nicknames on)
  and the times between them centred. A message is found by a part of its
  text; a card (a link, a channel's video, a mini program) shows its title,
  summary and source, cut short at the bubble's edge with an ellipsis read
  as `.`; emoji are read as anything (`🍁` as `17`, `🇻🇳🇨🇳` as `★圓`), so they
  are folded away, and a line is found with a quarter of it read otherwise.
  An image or a sticker is found by its sender's name or its minute. A
  message read on several lines is passed whole, so a notice posted again
  is not found inside the one after it.
- **The search's list.** The chat opened is the one whose name reads
  closest: `智能通信定位圈一广域物联.` for 智能通信定位圈—广域物联②群 (its `—` read as
  `一`, cut short), `★圓中越防失联交流群` for 🇻🇳🇨🇳中越防失联交流群.
- **The run.** `--dry-run` over the 17 told every one, 1.8 to 5.2 s a
  group, 61.4 s in all, 1.8 s of it reading the databases; reading the 18
  groups without touching the window took 16.3 s before, 2.9 s after. Two
  of the groups, 智能通信定位圈—广域物联①群 and ②群, have 500 members each,
  and were told apart by their messages; 周边游🚩户外旅行交友美食5群 (255)
  and 19群 (325), fed the same ads, by their counts.
- **The leave.** `--yes` left 智能通信定位圈—广域物联②群 (5308573596@chatroom):
  search 1.0 s, identify 1.2 s, its chat info and the confirmation 3.2 s;
  the databases wrote the notice `You've left the group chat` (type 10000,
  sent by the account, no server id) about a second after OK, and no longer
  listed the account among its members 11 s after it; the group then had
  499. That run waited for the membership, 17.6 s in all; since, the
  notice confirms the leave as well, which the code was changed to after it
  (not yet run on a real group).
