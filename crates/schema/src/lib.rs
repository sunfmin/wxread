//! The tables of WeChat 4.x's databases wxread reads, declared as WeChat
//! declares them (as WeChat for Mac 4.1 writes them, and bizkernel's
//! wechat-intake read them), each beside the database under `db_storage`
//! that holds it. wxread's queries name only these tables and columns, and
//! the tests' synthetic accounts are made from these declarations, so the
//! two cannot drift apart.

/// The contacts: `contact/contact.db`.
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

/// Each group's own record, in `contact/contact.db`: `ext_buffer` is a
/// protobuf listing its members, each a field 1 holding their id (its
/// field 1) and the name they gave themselves in the group (its field 2,
/// when they gave one).
pub const CHAT_ROOM: &str =
    "CREATE TABLE chat_room(id INTEGER PRIMARY KEY, username TEXT, owner TEXT, ext_buffer BLOB)";

/// The databases a chat's messages are in, under `db_storage`: those of
/// people and groups, `message/message_<n>.db`, and of official accounts,
/// `message/biz_message_<n>.db`, `<n>` from 0, the oldest first.
pub const MESSAGE_DBS: [&str; 2] = ["message/message_", "message/biz_message_"];

/// In each message database: the ids its messages name, a message's
/// `real_sender_id` being its sender's rowid here.
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
