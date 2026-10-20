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
A contact's name as WeChat shows it: the account's remark, else their name in the group it is shown in, else their own name. A group with no remark or name of its own is shown by its members' display names, the account's left out, in the order WeChat lists them, joined by `、`.
_Avoid_: name, label, shown name

**Official account**:
A contact that publishes (公众号), its id `gh_…`: what it sends the account is kept apart from the chats, in the account's `message/biz_message_<n>.db`.
_Avoid_: public account, subscription, biz account

**Article**:
One piece an official account published, with its title, digest, url and cover; one push of an official account's holds one or several.
_Avoid_: post, news, biz message

**Moment**:
One post to Moments (朋友圈): its author, when, its text, location, link and media, and the likes and comments on it, as WeChat on this Mac last loaded it into `sns/sns.db`; WeChat caches its media under `cache/<YYYY-MM>/Sns`, each by its Moment's and its own id.
_Avoid_: post, feed, timeline item, SNS

**Chat**:
A conversation the account has: direct, with one contact, or in one group.
_Avoid_: session, conversation, thread

**Folder**:
An entry of WeChat's chat list that holds others and has no messages of its own: the groups folded away (折叠的群聊, `@placeholder_foldgroup`), what subscription accounts sent (订阅号消息, `brandsessionholder`), the service accounts (服务号, `brandservicesessionholder`); `wxread chats` lists each as kind `folder`, by WeChat's name, with the command that reads what it holds.
_Avoid_: placeholder, holder, group folder

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

**Call**:
A voice or video call in a chat, as the bubble WeChat leaves of it: whether it connected and for how long, or was cancelled, declined or missed.
_Avoid_: VoIP, phone call

**Sender**:
The contact who said a message, by id, and shown as WeChat shows them: the account's remark for them, else their name in the group, else their own name. A recall's is who recalled it; WeChat's other notices have none.
_Avoid_: author, from, user

**Notice**:
A message WeChat itself writes into a chat: a recall, a join or leave, a rename, and the like.
_Avoid_: system message, event

**Full-text index**:
WeChat's own index of the chats' messages (`message/message_fts.db`): for each message it has indexed, the text WeChat searches it by; the newest messages are not in it yet.
_Avoid_: FTS, search index, fts db

**Media**:
A file a message or a Moment carries rather than holds: an image, a video, a voice message, a file; one of them a medium.
_Avoid_: attachment, resource

**Re-signing**:
Signing the WeChat app again, ad-hoc, with its entitlements and `get-task-allow` added, so root may read the running WeChat's memory; what `wxread resign --yes` does, undone by reinstalling WeChat.
_Avoid_: patching, cracking, unsigning
**Transcription**:
The text WeChat heard in a voice message, made only when the account chose to convert it to text in WeChat (转文字); wxread never transcribes.
_Avoid_: transcript, speech-to-text, caption

**Thumbnail**:
The small copy WeChat keeps of an image or a video beside its file, often when it holds no file: an image's `_t.dat`, a video's `_thumb.jpg`.
_Avoid_: preview, thumb

**Missing**:
A media message's or a favorite's file, or an avatar, that WeChat on this Mac does not hold (it never downloaded it, or cleared it): an answer, not an error, with the thumbnail when there is one and what makes WeChat fetch it.
_Avoid_: not found, absent, deleted

**Favorite**:
Something the account saved in WeChat's Favorites (收藏), by its own id: a text, a link, an image, a video, a file, a voice message, a place, a chat history, a note and the like, with where it was saved from and the tags the account gave it.
_Avoid_: bookmark, collection, saved item, star

**Item**:
One part of a favorite, as WeChat keeps it in its data list: a chat history's messages, a note's paragraphs and images, or the one image or file a favorite is; an item that carries a file is fetched by its file id, `fav:<favorite>:<item>`.
_Avoid_: data item, element, entry

**Image key**:
The key an account's V2 images are sealed under, derived from the code in WeChat's kvcomm files' names and the account's id, and told by the image it opens; unlike a database's key, it needs no memory read.
_Avoid_: image password, dat key

**Avatar**:
The image WeChat shows for a contact or a group, as its head image database holds it.
_Avoid_: profile picture, head image, icon

**Finding**:
One thing `wxread doctor` found wrong, and the line that fixes it.
_Avoid_: issue, warning, check
