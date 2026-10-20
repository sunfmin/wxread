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
