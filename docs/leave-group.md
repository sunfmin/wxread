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
prototype: wxread does not ship it. Each screen is described below by the text
recognized on it; the screenshots themselves are not kept (they are the owner's
chat list).

WeChat 4.x draws its window itself (Qt): Accessibility sees its menus and
nothing inside the window. A menu is pressed through Accessibility; everything
inside the window is found by macOS's text recognition (Vision,
`VNRecognizeTextRequest`, accurate, `zh-Hans` and `en-US`) on a screenshot of the
window (`screencapture -x -o -l <window id>`), in the window's points. Recognized
text is compared with spaces and quotes removed and full-width parentheses made
ASCII: Vision reads `wxread 退群测试` as `wxread退群测试` and `（园）` as `(园)`.
WeChat on this Mac shows English; each label below is given in English and in
Chinese.

## Before anything: which group

wxread answers the group (`wxread group <id>`): its display name, its own name,
and its members. Then every contact is read (`wxread contacts`, page by page:
`--query` matches a group's own name, not the display name a group with none is
shown by), and the display name must belong to this one group alone. Two groups
answered with the same display name are refused before the window is touched:

```json
{"step":"refused","why":"more than one group is shown as 孙凤君、秦俊斌 @qinjunbin: the window cannot tell them apart","groups":["17812931124@chatroom","19010950345@chatroom"]}
```

WeChat's search opens the first chat it finds by a name, and nothing in the
window says which of two same-named groups it is (the member count can be the
same too: these two have 3 each), so there is nothing to tell them apart by.

The opened chat is then the group asked for when **all three** hold:

1. its title reads `<display name> (<members>)`, the count being the number of
   members wxread answers;
2. its chat info's **Group Name** (群聊名称) reads the group's own name, when it
   has one;
3. the confirmation quotes the display name, on as many lines as it wraps to.

Any of them failing stops the path; at the confirmation, it is cancelled.

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
a paste, in its window or any other; Return opens the first. The chat is told by
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
