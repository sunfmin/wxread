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

/// The official accounts' messages, WeChat's store for them apart from the
/// chats': `message/biz_message_<n>.db`, one or more, each holding some of
/// the official accounts whole.
pub const BIZ_MESSAGE_PREFIX: &str = "message/biz_message_";

/// Whether database `path` is one of the official accounts' message stores.
pub fn is_biz_message_db(path: &str) -> bool {
    path.strip_prefix(BIZ_MESSAGE_PREFIX)
        .and_then(|rest| rest.strip_suffix(".db"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// In each message store: the ids its messages name, one row each, an
/// official account's `gh_…` among them.
pub const NAME2ID: &str = "CREATE TABLE Name2Id(user_name TEXT PRIMARY KEY, is_session INTEGER)";

/// In each message store: one chat's messages (an official account's, in
/// a `biz_message_<n>.db`), its table named [`messages_table`].
/// `local_type` is the message's type in its low 32 bits and its subtype in
/// the high (an app message `49 | <appmsg type> << 32`), `create_time` when
/// it was sent in seconds since the epoch, and `message_content` its XML,
/// zstd-compressed when `WCDB_CT_message_content` is [`ZSTD`].
pub fn messages(table: &str) -> String {
    format!(
        "CREATE TABLE {table}(local_id INTEGER PRIMARY KEY AUTOINCREMENT, server_id INTEGER, local_type INTEGER, sort_seq INTEGER, real_sender_id INTEGER, create_time INTEGER, status INTEGER, upload_status INTEGER, download_status INTEGER, server_seq INTEGER, origin_source INTEGER, source TEXT, message_content TEXT, compress_content TEXT, packed_info_data BLOB, WCDB_CT_message_content INTEGER DEFAULT NULL, WCDB_CT_source INTEGER DEFAULT NULL)"
    )
}

/// The table of chat `id`'s messages: `Msg_` and the MD5 of its id, in
/// lowercase hex.
pub fn messages_table(id: &str) -> String {
    format!("Msg_{:x}", md5::compute(id.as_bytes()))
}

/// What `WCDB_CT_message_content` is when the content is zstd-compressed.
pub const ZSTD: i64 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chat_s_messages_are_in_the_table_of_its_id_s_md5() {
        assert_eq!(messages_table("gh_abc"), format!("Msg_{:x}", md5::compute("gh_abc")));
        assert_eq!(messages_table(""), "Msg_d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn the_official_accounts_stores_are_biz_message_n() {
        for path in ["message/biz_message_0.db", "message/biz_message_12.db"] {
            assert!(is_biz_message_db(path), "{path}");
        }
        for path in [
            "message/message_0.db",
            "message/biz_message_.db",
            "message/biz_message_0.db-wal",
            "biz_message_0.db",
            "message/biz_message_x.db",
        ] {
            assert!(!is_biz_message_db(path), "{path}");
        }
    }
}
