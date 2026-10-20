# wxread

wxread reads what WeChat for Mac 4.x keeps on the Mac it is logged in on:
its own encrypted databases and files, read where they lie.

## Language

**Data directory**:
The directory WeChat for Mac keeps every account it has logged in on this Mac in (`xwechat_files`).
_Avoid_: container, WeChat folder

**Account**:
One WeChat identity logged in on this Mac, and the directory of its own under the data directory that holds its databases.
_Avoid_: user, profile, login

**Database**:
One of an account's encrypted SQLite files, named by its path under the account's `db_storage`.
_Avoid_: db file, store

**Key**:
The secret that opens one database; each database has its own, and only the running WeChat holds it.
_Avoid_: password, passphrase, DB key

**Contact**:
A WeChat identity the account knows, by its id: a person, a group, or an official account (its id `gh_…`).
_Avoid_: friend, buddy

**Group**:
A contact that is many people in one chat, its id `<n>@chatroom`, with an owner, its members and a notice.
_Avoid_: chatroom, room, group chat

**Member**:
A person in a group, whether or not the account has them as a contact, with the name they set in that group, if any.
_Avoid_: participant, user

**Notice**:
What a group pins for its members (群公告): its text, who posted it and when.
_Avoid_: announcement, bulletin

**Remark**:
What the account calls a contact, set by the account and seen only by it (备注).
_Avoid_: alias, nickname, note

**Display name**:
A contact's name as WeChat shows it: the account's remark, else their name in the group it is shown in, else their own name.
_Avoid_: name, label, shown name

**Official account**:
A contact that publishes (公众号), its id `gh_…`: what it sends the account is kept apart from the chats, in the account's `message/biz_message_<n>.db`.
_Avoid_: public account, subscription, biz account

**Article**:
One piece an official account published, with its title, digest, url and cover; one push of an official account's holds one or several.
_Avoid_: post, news, biz message

**Chat**:
A conversation the account has: direct, with one contact, or in one group.
_Avoid_: session, conversation, thread

**WAL**:
The `-wal` file beside a database: what WeChat wrote last, not yet moved into the database itself, read with it.
_Avoid_: journal, log

**Cursor**:
The `next` a list answers, passed back as `--cursor` to get the page after; it names where the page ended, so nothing is kept between runs.
_Avoid_: token, offset, page number

**Message**:
One thing said in a chat, by the account or another contact, at one time.
_Avoid_: msg, record, entry

**Kind**:
What a message is (text, image, voice, file, link, and so on), whatever number WeChat stores it under.
_Avoid_: type, msg type

**Sender**:
The contact who said a message, by id, and shown as WeChat shows them: the account's remark for them, else their name in the group, else their own name. WeChat's own notices have none.
_Avoid_: author, from, user

**Notice**:
A message WeChat itself writes into a chat: a recall, a join or leave, a rename, and the like.
_Avoid_: system message, event

**Full-text index**:
WeChat's own index of the chats' messages (`message/message_fts.db`): for each message it has indexed, the text WeChat searches it by; the newest messages are not in it yet.
_Avoid_: FTS, search index, fts db

**Media**:
A file a message carries rather than holds: an image, a video, a voice message, a file.
_Avoid_: attachment, resource

**Finding**:
One thing `wxread doctor` found wrong, and the line that fixes it.
_Avoid_: issue, warning, check
