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
Another WeChat identity the account knows: a person, a group, or an official account.
_Avoid_: friend, buddy

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

**Media**:
A file a message carries rather than holds: an image, a video, a voice message, a file.
_Avoid_: attachment, resource

**Finding**:
One thing `wxread doctor` found wrong, and the line that fixes it.
_Avoid_: issue, warning, check
