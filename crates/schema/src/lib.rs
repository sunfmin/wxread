//! The tables of WeChat 4.x's databases wxread reads, declared as WeChat
//! declares them (as WeChat for Mac 4.1 writes them, and bizkernel's
//! wechat-intake read them), each beside the database under `db_storage`
//! that holds it. wxread's queries name only these tables and columns, and
//! the tests' synthetic accounts are made from these declarations, so the
//! two cannot drift apart.

/// The contacts, groups among them: `contact/contact.db`.
pub const CONTACT_DB: &str = "contact/contact.db";

/// Every contact the account knows, a group among them, one row each:
/// `username` its id (a wxid, or a group's `<n>@chatroom`), `remark` what
/// the account calls it, `nick_name` what it calls itself.
pub const CONTACT: &str = "CREATE TABLE contact(id INTEGER PRIMARY KEY, username TEXT, local_type INTEGER, alias TEXT, encrypt_username TEXT, flag INTEGER, delete_flag INTEGER, verify_flag INTEGER, remark TEXT, remark_quan_pin TEXT, remark_pin_yin_initial TEXT, nick_name TEXT, pin_yin_initial TEXT, quan_pin TEXT, big_head_url TEXT, small_head_url TEXT, head_img_md5 TEXT, chat_room_notify INTEGER, is_in_chat_room INTEGER, description TEXT, extra_buffer BLOB, chat_room_type INTEGER)";

/// The account's chats: `session/session.db`.
pub const SESSION_DB: &str = "session/session.db";

/// One row a chat, by its contact's `username`: `unread_count` its
/// messages not yet read, `last_timestamp` when its last message was and
/// `sort_timestamp` where WeChat lists it, both in seconds since the epoch.
pub const SESSION: &str = "CREATE TABLE SessionTable(username TEXT PRIMARY KEY, type INTEGER, unread_count INTEGER, unread_first_msg_srv_id INTEGER, unread_first_pat_msg_local_id INTEGER, unread_first_pat_msg_sort_seq INTEGER, is_hidden INTEGER, summary TEXT, draft TEXT, status INTEGER, last_timestamp INTEGER, sort_timestamp INTEGER, last_clear_unread_timestamp INTEGER, last_msg_locald_id INTEGER, last_msg_type INTEGER, last_msg_sub_type INTEGER, last_msg_sender TEXT, last_sender_display_name TEXT, last_msg_ext_type INTEGER)";

/// In `contact/contact.db`: each group's members, `room_id` the group's
/// `contact.id` and `member_id` each member's.
pub const CHATROOM_MEMBER: &str = "CREATE TABLE chatroom_member(room_id INTEGER, member_id INTEGER, CONSTRAINT room_member UNIQUE(room_id, member_id))";

/// In `contact/contact.db`: each group's own record, by its `username`:
/// `owner` its owner's id, and `ext_buffer` a protobuf listing its
/// members, each a field 1 holding their id (its field 1) and their name in
/// the group (its field 2, when they set one), among more not read.
pub const CHAT_ROOM: &str =
    "CREATE TABLE chat_room(id INTEGER PRIMARY KEY, username TEXT, owner TEXT, ext_buffer BLOB)";

/// In `contact/contact.db`: each group's notice (群公告), by its
/// `username_`: its text, who posted it (an id) and when, in seconds since
/// the epoch.
pub const CHAT_ROOM_INFO_DETAIL: &str = "CREATE TABLE chat_room_info_detail(room_id_ INTEGER PRIMARY KEY, username_ TEXT, announcement_ TEXT, announcement_editor_ TEXT, announcement_publish_time_ INTEGER, chat_room_status_ INTEGER, xml_announcement_ TEXT, ext_buffer_ BLOB)";

/// The avatars WeChat holds: `head_image/head_image.db`.
pub const HEAD_IMAGE_DB: &str = "head_image/head_image.db";

/// Each avatar WeChat holds, a JPEG or a PNG, by its contact's `username`.
pub const HEAD_IMAGE: &str = "CREATE TABLE head_image(username TEXT PRIMARY KEY, md5 TEXT, image_buffer BLOB, update_time INTEGER)";

/// The databases a chat's messages are in, under `db_storage`: those of
/// people and groups, `message/message_<n>.db`, and of official accounts,
/// `message/biz_message_<n>.db`, `<n>` from 0, the oldest first.
pub const MESSAGE_DBS: [&str; 2] = ["message/message_", "message/biz_message_"];

/// In each message database: the ids its messages name, a message's
/// `real_sender_id` being its sender's rowid here; in each media database,
/// the chats its voice messages are in, a voice's `chat_name_id` being its
/// chat's rowid here.
pub const NAME2ID: &str = "CREATE TABLE Name2Id(user_name TEXT PRIMARY KEY, is_session INTEGER)";

/// In a message database: one chat's messages, the table named
/// [`messages_table`] of the chat. `server_id` is WeChat's id of the
/// message, `local_type` its type (an app message's subtype in the high 32
/// bits), `sort_seq` its order among those of one second, `create_time`
/// when it was sent, in seconds since the epoch; `message_content` is
/// zstd-compressed when `WCDB_CT_message_content` is [`ZSTD`].
pub fn messages(table: &str) -> String {
    format!(
        "CREATE TABLE {table}(local_id INTEGER PRIMARY KEY AUTOINCREMENT, server_id INTEGER, local_type INTEGER, sort_seq INTEGER, real_sender_id INTEGER, create_time INTEGER, status INTEGER, upload_status INTEGER, download_status INTEGER, server_seq INTEGER, origin_source INTEGER, source TEXT, message_content TEXT, compress_content TEXT, packed_info_data BLOB, WCDB_CT_message_content INTEGER DEFAULT NULL, WCDB_CT_source INTEGER DEFAULT NULL)"
    )
}

/// The table of chat `chat`'s messages: `Msg_` and the MD5 of its id, in
/// lowercase hex.
pub fn messages_table(chat: &str) -> String {
    format!("Msg_{:x}", md5::compute(chat.as_bytes()))
}

/// What `WCDB_CT_message_content` is when the content is zstd-compressed.
pub const ZSTD: i64 = 4;

/// In each message database: when WeChat made it, in seconds since the
/// epoch, one row. WeChat's full-text index names a database by it.
pub const TIMESTAMP: &str = "CREATE TABLE TimeStamp(timestamp INTEGER)";

/// WeChat's own full-text index of the chats' messages:
/// `message/message_fts.db`.
pub const MESSAGE_FTS_DB: &str = "message/message_fts.db";

/// In `message/message_fts.db`: the chats its entries name, an entry's
/// `session_id` being its chat's rowid here.
pub const FTS_NAME2ID: &str = "CREATE TABLE name2id(username TEXT PRIMARY KEY)";

/// In `message/message_fts.db`: what it has indexed, one row a chat and a
/// message database: that chat's messages from `start_local_id` to
/// `end_local_id` of the database whose [`TIMESTAMP`] is `db_time_stamp`.
pub const FTS_RANGE: &str = "CREATE TABLE message_fts_v4_range(db_time_stamp INTEGER, start_local_id INTEGER, end_local_id INTEGER, session_id INTEGER, CONSTRAINT sessionId_dbTime PRIMARY KEY(session_id, db_time_stamp))";

/// The tables of `message/message_fts.db` whose entries are read, each
/// [`fts_content`] of its number: `message_fts_v4_<n>_content`.
pub const FTS_CONTENT: (&str, &str) = ("message_fts_v4_", "_content");

/// In `message/message_fts.db`: the entries of index `n`, one a message,
/// each the columns of the FTS5 table `message_fts_v4_<n>` WeChat declares
/// over them: `c0` the text WeChat searches it by (a text's words, a link's
/// title, a file's name), then its `message_local_id`, `sort_seq`,
/// `local_type`, `session_id`, `sender_id` and `create_time`. WeChat's own
/// tokenizer (`MMFtsTokenizer`) is the only one that reads the FTS5 table,
/// so the entries are read here, where they are plain.
pub fn fts_content(n: u32) -> String {
    format!(
        "CREATE TABLE '{}{n}{}'(id INTEGER PRIMARY KEY, c0, c1, c2, c3, c4, c5, c6)",
        FTS_CONTENT.0, FTS_CONTENT.1
    )
}

/// The databases a chat's voice messages are in, under `db_storage`:
/// `message/media_<n>.db`, `<n>` from 0.
pub const MEDIA_DBS: &str = "message/media_";

/// In each media database: the chats its voice messages are in, a voice
/// message's `chat_name_id` being its chat's rowid here.
pub const MEDIA_NAME2ID: &str = "CREATE TABLE Name2Id(user_name TEXT PRIMARY KEY)";

/// In each media database: each voice message's SILK bytes, `voice_data`,
/// by its `svr_id`, the message's server id.
pub const VOICE_INFO: &str = "CREATE TABLE VoiceInfo(chat_name_id INTEGER, create_time INTEGER, local_id INTEGER, svr_id INTEGER, voice_data BLOB, data_index TEXT DEFAULT '0')";

/// What the account saved (收藏): `favorite/favorite.db`.
pub const FAVORITE_DB: &str = "favorite/favorite.db";

/// Each favorite, one row: `type` what it is, `update_time` when it was
/// saved or last edited, in seconds since the epoch, `content` its
/// `<favitem>` XML, `fromusr` the chat it was saved from and
/// `realchatname` who said it there, in a group.
pub const FAV_DB_ITEM: &str = "CREATE TABLE fav_db_item(local_id INTEGER PRIMARY KEY AUTOINCREMENT, server_id INTEGER, type INTEGER, update_seq INTEGER, flag INTEGER, update_time INTEGER, version INTEGER, content TEXT, source_id TEXT, sync_status INTEGER, upload_status INTEGER, upload_error_code INTEGER, trans_res_status INTEGER, trans_res_error_code INTEGER, fromusr TEXT, fromusr_id INTEGER, realchatname TEXT, realchatname_id INTEGER, ext_buf TEXT, star INTEGER, star_type INTEGER, star_biz_key TEXT)";
