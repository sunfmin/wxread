# A folder of WeChat's chat list is answered as a folder

WeChat for Mac 4.x keeps its chat list in `SessionTable` of `session/session.db`, and beside the chats it keeps there the folders its list shows: `@placeholder_foldgroup` (折叠的群聊, the groups the account folded away), `brandsessionholder` (订阅号消息, what subscription accounts sent) and `brandservicesessionholder` (服务号, the service accounts). None is a contact and none has messages of its own. `wxread chats` answered them as `direct` chats named `""` with 2 members, and `wxread messages --chat` on one then failed telling the agent to pass "an id `wxread chats` lists": the list handed out an id the next command refused, as an agent found on a real account of 521 chats.

A chat's kind is now one of `direct`, `group` and `folder`. A folder is named as WeChat names it, has no members (`null`), and is kept in the list where WeChat sorts it, since the list is WeChat's list. Every chat answers `read_with`, the command that reads what it holds: `wxread messages --chat <id>` for a direct chat or a group; for a folder, `wxread chats` (each folded group is a chat of its own in the list) or `wxread articles` (official accounts' posts). Asking `wxread messages`, `voice` or `search` for a folder's messages is refused naming that command. The folders are one closed set, `wxread_chats::Folder`, the one place their ids, names and commands are kept; a folder is told by its id alone.

## Consequences

- `read_with` is on every chat, not only on folders, so an agent reads one field to know its next command, whatever the kind.
- `--query` finds a folder by WeChat's name for it; `--with` never answers one, since no contact is in a folder.
- An id of WeChat's chat list that is no contact and not one of these folders is still answered as a direct chat; one found on a real account joins the set.
- A chat none of the message databases holds is refused without calling its id one `wxread chats` does not list: it may be one that WeChat on this Mac holds no message of.
