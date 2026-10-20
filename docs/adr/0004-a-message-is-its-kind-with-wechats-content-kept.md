# A message is answered as its kind, with WeChat's content and type kept beside it

`wxread messages` answers each message as one of a closed set of kinds (text, link, quote, location, card, forward, recall, nudge, join, leave, renamed, transfer, red_packet, image, video, file, sticker, voice, system, unknown), its kind's fields in an object named by the kind, and beside them, always, the content as WeChat keeps it (a group's without its `<sender>:\n`, decompressed) and WeChat's type and subtype. A message whose type wxread does not know, or whose XML does not read, is `unknown`: never dropped and never an error, since an agent can still read its content and type, and a run that failed on one odd message would hide the whole chat. The kinds and their XML are as bizkernel's wechat-intake and wechat-cli found them; reading them is the pure crate `wxread-messages`, tested on content alone.

Messages are listed newest first, as chats are: by when each was sent, then by WeChat's order among those of one second (`sort_seq`), then by database and row, so a `next` names one place and each message is on one page however many share a second.

## Consequences

- A new kind is a new name in the set and in `wxread messages --help`, whose list the tests hold to the set; until then its messages are `unknown` with their content.
- Media is named, not fetched: each media kind says what finds its file, and fetching it is a command of its own.
- A recall is a kind, not a flag on the message it recalled: WeChat rewrites that message's row into the recall's notice, so the original is no longer there to flag.
- A recall's sender is who recalled it. WeChat for Mac keeps no id of them: its notice names the account as "你" ("You"), anyone else by the name it showed them by, in quotes. That name is matched against each member's remark, name in the group and own name; it is the sender only when exactly one member goes by it, and none otherwise, never a guess. In a direct chat it is the other. The message recalled is named by its id only when the notice holds it (`<newmsgid>`), since WeChat rewrote its row into the recall.
