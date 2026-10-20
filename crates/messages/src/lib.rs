//! What `wxread messages` answers of the rows it read: each message of a
//! chat as its kind, its sender named, newest first or oldest first, a
//! page at a time. The
//! effect crate reads the rows out of the account's message databases and
//! the names out of its contacts; nothing here reads or writes.

use std::io::Read as _;

use serde_json::{Map, Value, json};
use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_media::{Held, Media};

pub mod kinds;
pub mod media;
pub mod names;
mod proto;

pub use names::{Contact, Names};

/// The options that pick a chat and a stretch of time.
pub const CHAT_FLAG: &str = "--chat";
pub const FROM_FLAG: &str = "--from";
pub const TO_FLAG: &str = "--to";
/// The option that reads a chat forward, from its first message.
pub const OLDEST_FIRST_FLAG: &str = "--oldest-first";

/// Which way a chat is read: newest first, unless `--oldest-first` says
/// from its first message forward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    NewestFirst,
    OldestFirst,
}

impl Order {
    /// The order `--oldest-first` says, given or not.
    pub fn of(oldest_first: bool) -> Order {
        if oldest_first { Order::OldestFirst } else { Order::NewestFirst }
    }

    /// The other way.
    pub fn reverse(self) -> Order {
        match self {
            Order::NewestFirst => Order::OldestFirst,
            Order::OldestFirst => Order::NewestFirst,
        }
    }
}

/// What a cursor of an [`Order::OldestFirst`] read starts with, so one
/// given with the other order is told apart and refused.
const OLDEST: &str = "oldest";

/// What a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Link,
    Quote,
    Location,
    Card,
    Forward,
    Recall,
    Nudge,
    Join,
    Leave,
    Renamed,
    Transfer,
    RedPacket,
    Call,
    Image,
    Video,
    File,
    Sticker,
    Voice,
    System,
    Unknown,
}

impl Kind {
    /// Every kind, as the answer names them.
    pub const ALL: [Kind; 21] = [
        Kind::Text,
        Kind::Link,
        Kind::Quote,
        Kind::Location,
        Kind::Card,
        Kind::Forward,
        Kind::Recall,
        Kind::Nudge,
        Kind::Join,
        Kind::Leave,
        Kind::Renamed,
        Kind::Transfer,
        Kind::RedPacket,
        Kind::Call,
        Kind::Image,
        Kind::Video,
        Kind::File,
        Kind::Sticker,
        Kind::Voice,
        Kind::System,
        Kind::Unknown,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::Quote => "quote",
            Kind::Location => "location",
            Kind::Card => "card",
            Kind::Forward => "forward",
            Kind::Recall => "recall",
            Kind::Nudge => "nudge",
            Kind::Join => "join",
            Kind::Leave => "leave",
            Kind::Renamed => "renamed",
            Kind::Transfer => "transfer",
            Kind::RedPacket => "red_packet",
            Kind::Call => "call",
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::File => "file",
            Kind::Sticker => "sticker",
            Kind::Voice => "voice",
            Kind::System => "system",
            Kind::Unknown => "unknown",
        }
    }
}

/// One row of a chat's messages table, as one message database holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The database it is in, by its name: `message_0`, `biz_message_1`.
    pub db: String,
    pub local_id: i64,
    /// WeChat's id of it: 0 for one the server never saw.
    pub server_id: i64,
    pub local_type: i64,
    pub sort_seq: i64,
    /// When it was sent, in seconds since the epoch.
    pub create_time: i64,
    /// Its sender's id, from `real_sender_id` through `Name2Id`.
    pub sender: Option<String>,
    /// `message_content`, as stored.
    pub content: Vec<u8>,
    /// `WCDB_CT_message_content`: [`wxread_schema::ZSTD`] for compressed.
    pub compressed: Option<i64>,
    /// `packed_info_data`.
    pub packed: Vec<u8>,
}

/// The stretch of time the messages are read from: at or after `from`,
/// before `to`, each in seconds since the epoch, None for no bound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Range {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl Range {
    /// The stretch `--from` and `--to` say, or what fixes them.
    pub fn parse(from: Option<&str>, to: Option<&str>) -> Result<Range, String> {
        let at = |flag: &str, text: Option<&str>| {
            text.map(|text| {
                wxread_answer::seconds(text).ok_or_else(|| {
                    format!(
                        "{flag} {text} is not a time: pass RFC 3339 with its offset (2026-10-20T09:30:00+08:00, 2026-10-20T01:30:00Z), or a date (2026-10-20), its midnight in UTC"
                    )
                })
            })
            .transpose()
        };
        let range = Range { from: at(FROM_FLAG, from)?, to: at(TO_FLAG, to)? };
        match (range.from, range.to) {
            (Some(f), Some(t)) if f >= t => Err(format!(
                "{FROM_FLAG} {} is not before {TO_FLAG} {}: {FROM_FLAG} is the first time read, {TO_FLAG} the time reading stops before; pass a {FROM_FLAG} earlier than {TO_FLAG}",
                from.unwrap_or_default(),
                to.unwrap_or_default()
            )),
            _ => Ok(range),
        }
    }

    /// Whether `at` is within it.
    pub fn holds(&self, at: i64) -> bool {
        self.from.is_none_or(|f| at >= f) && self.to.is_none_or(|t| at < t)
    }
}

/// Where a page ended: the last message it holds, by when it was sent,
/// its order in that second, and its row, and which way it was read. Its
/// text is the `next` an answer gives and `--cursor` takes back: that of
/// an oldest-first read starts `oldest.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    order: Order,
    create_time: i64,
    sort_seq: i64,
    db: String,
    local_id: i64,
}

impl Cursor {
    /// The cursor `text` writes, for a read in `order`, or what fixes it:
    /// a cursor of the other order is refused, naming the option that
    /// continues it.
    pub fn parse(text: &str, order: Order) -> Result<Cursor, String> {
        let (written, place) = match text.strip_prefix(OLDEST).and_then(|t| t.strip_prefix('.')) {
            Some(place) => (Order::OldestFirst, place),
            None => (Order::NewestFirst, text),
        };
        let parts: Vec<&str> = place.split('.').collect();
        let cursor = match parts.as_slice() {
            [create_time, sort_seq, db, local_id] if !db.is_empty() => {
                match (create_time.parse(), sort_seq.parse(), local_id.parse()) {
                    (Ok(create_time), Ok(sort_seq), Ok(local_id)) => Some(Cursor {
                        order: written,
                        create_time,
                        sort_seq,
                        db: db.to_string(),
                        local_id,
                    }),
                    _ => None,
                }
            }
            _ => None,
        };
        let start = match order {
            Order::NewestFirst => "the newest",
            Order::OldestFirst => "the first",
        };
        match cursor {
            None => Err(format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread messages answered: pass the `next` of the answer before, with the same {CHAT_FLAG}, or leave {CURSOR_FLAG} out to start at {start} message"
            )),
            Some(c) if c.order != order => Err(match c.order {
                Order::OldestFirst => format!(
                    "{CURSOR_FLAG} {text} is a `next` of an {OLDEST_FIRST_FLAG} read: pass {OLDEST_FIRST_FLAG} with it to read on forward, or leave {CURSOR_FLAG} out to start at {start} message"
                ),
                Order::NewestFirst => format!(
                    "{CURSOR_FLAG} {text} is a `next` of a newest-first read: leave {OLDEST_FIRST_FLAG} out with it to read on back, or leave {CURSOR_FLAG} out to start at {start} message"
                ),
            }),
            Some(c) => Ok(c),
        }
    }

    /// Which way the read it ended was going.
    pub fn order(&self) -> Order {
        self.order
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        let place = format!("{}.{}.{}.{}", self.create_time, self.sort_seq, self.db, self.local_id);
        match self.order {
            Order::NewestFirst => place,
            Order::OldestFirst => format!("{OLDEST}.{place}"),
        }
    }

    fn of(row: &Row, order: Order) -> Cursor {
        Cursor {
            order,
            create_time: row.create_time,
            sort_seq: row.sort_seq,
            db: row.db.clone(),
            local_id: row.local_id,
        }
    }

    /// The bound of a row of database `db` that comes after this cursor:
    /// its `(create_time, sort_seq, local_id)` is below the one answered,
    /// newest first, and above it, oldest first.
    pub fn bound_in(&self, db: &str) -> (i64, i64, i64) {
        let local_id = match db.cmp(self.db.as_str()) {
            std::cmp::Ordering::Less => i64::MAX,
            std::cmp::Ordering::Equal => self.local_id,
            std::cmp::Ordering::Greater => i64::MIN,
        };
        (self.create_time, self.sort_seq, local_id)
    }

    /// Whether `row` comes after the message this cursor names, in the
    /// order it was read.
    fn before(&self, row: &Row) -> bool {
        let wanted = match self.order {
            Order::NewestFirst => std::cmp::Ordering::Less,
            Order::OldestFirst => std::cmp::Ordering::Greater,
        };
        order(&Cursor::of(row, self.order), self) == wanted
    }
}

/// Where messages stand, oldest first: by when each was sent, then by its
/// order in that second, then by its database and row, so each has one
/// place.
fn order(a: &Cursor, b: &Cursor) -> std::cmp::Ordering {
    (a.create_time, a.sort_seq, a.db.as_str(), a.local_id).cmp(&(
        b.create_time,
        b.sort_seq,
        b.db.as_str(),
        b.local_id,
    ))
}

/// Where a page starts: at a chat's newest or first message, or after the
/// message a cursor names, read on the way it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start<'a> {
    First(Order),
    After(&'a Cursor),
}

impl Start<'_> {
    /// Which way the page is read.
    pub fn order(&self) -> Order {
        match self {
            Start::First(order) => *order,
            Start::After(cursor) => cursor.order,
        }
    }

    /// The cursor it starts after, if any.
    pub fn after(&self) -> Option<&Cursor> {
        match self {
            Start::First(_) => None,
            Start::After(cursor) => Some(cursor),
        }
    }
}

/// One page of a chat's messages.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub messages: Vec<Value>,
    /// Where the next page starts, when there are messages past this one.
    pub next: Option<Cursor>,
}

/// What a chat is, to read its messages: its id, and the account's own id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: String,
    pub account: String,
}

impl Chat {
    fn is_group(&self) -> bool {
        wxread_chats::Kind::of(&self.id) == wxread_chats::Kind::Group
    }
}

/// The page of `chat`'s messages in `rows` within `range` that starts at
/// `start` and holds at most `limit`: newest first or oldest first, as
/// `start` reads, by when each was sent and its order in that second, so a
/// cursor names one place and each message is on one page; each named by
/// `names`, and a media message's file and thumbnail held as `held` says.
pub fn page(
    chat: &Chat,
    names: &Names,
    rows: Vec<Row>,
    range: Range,
    start: Start<'_>,
    limit: Limit,
    held: impl Fn(&Media, &Row) -> Held,
) -> Page {
    let way = start.order();
    let after = start.after();
    let mut rows: Vec<Row> = rows
        .into_iter()
        .filter(|r| range.holds(r.create_time) && after.is_none_or(|c| c.before(r)))
        .collect();
    rows.sort_by(|a, b| {
        let (a, b) = (Cursor::of(a, way), Cursor::of(b, way));
        match way {
            Order::NewestFirst => order(&b, &a),
            Order::OldestFirst => order(&a, &b),
        }
    });
    let more = rows.len() > limit.get();
    rows.truncate(limit.get());
    let next = if more { rows.last().map(|r| Cursor::of(r, way)) } else { None };
    Page { messages: rows.iter().map(|r| message(chat, names, r, &held)).collect(), next }
}

/// `row` as the answer has it: its id, time and sender (one its content
/// names only by name, who goes by it in `names`), its kind and that
/// kind's fields, a media kind's with whether WeChat holds its file and
/// thumbnail as `held` says, and beside them WeChat's content and type.
pub fn message(
    chat: &Chat,
    names: &Names,
    row: &Row,
    held: &impl Fn(&Media, &Row) -> Held,
) -> Value {
    let Read { sender, content, mut parsed } = read(chat, row);
    let sender = sender.or_else(|| parsed.named.as_deref().and_then(|n| names.who(chat, n)));
    if let Some(found) = media::locate(parsed.kind, &parsed.fields) {
        let h = held(&found, row);
        parsed.fields.insert("file_present".into(), json!(h.file));
        parsed.fields.insert("thumbnail_present".into(), json!(h.thumbnail));
    }
    let (t, sub) = kinds::split(row.local_type);
    let mut out = Map::new();
    out.insert("id".into(), json!((row.server_id != 0).then(|| row.server_id.to_string())));
    out.insert("at".into(), json!(rfc3339(row.create_time)));
    out.insert("sender".into(), json!(sender));
    out.insert("sender_name".into(), json!(sender.as_deref().and_then(|s| names.of(s))));
    out.insert("kind".into(), json!(parsed.kind.name()));
    out.insert(parsed.kind.name().into(), Value::Object(parsed.fields));
    out.insert("type".into(), json!(t));
    out.insert("subtype".into(), json!(sub));
    out.insert("content".into(), json!(content));
    Value::Object(out)
}

/// A row of a chat, read: who said it, its content as the answer has it,
/// and its kind.
pub struct Read {
    pub sender: Option<String>,
    pub content: String,
    pub parsed: kinds::Parsed,
}

/// `row` of `chat`, read: its sender, its content decompressed and without
/// a group's `<sender>:\n`, and its kind with that kind's fields.
pub fn read(chat: &Chat, row: &Row) -> Read {
    let (sender, content) = said(chat, row);
    let mut parsed = kinds::parse(&chat.account, row.local_type, &content, &row.packed);
    let sender = parsed.sender.take().or(sender);
    Read { sender, content, parsed }
}

/// What finds the file of `row`, a message of `chat`; or, for one that
/// carries none, its kind.
pub fn media_of(chat: &Chat, row: &Row) -> Result<Media, Kind> {
    let parsed = read(chat, row).parsed;
    media::locate(parsed.kind, &parsed.fields).ok_or(parsed.kind)
}

/// Who said `row` in `chat`, and what: its content decompressed, a
/// group's without the `<sender>:\n` WeChat starts it with, its sender
/// from that prefix when the row names none.
fn said(chat: &Chat, row: &Row) -> (Option<String>, String) {
    let content = text_of(row);
    if !chat.is_group() {
        return (row.sender.clone(), content);
    }
    match (&row.sender, content.split_once(":\n")) {
        (Some(s), Some((prefix, rest))) if prefix == s => (Some(s.clone()), rest.to_string()),
        (None, Some((prefix, rest)))
            if !prefix.is_empty()
                && !prefix.contains(char::is_whitespace)
                && !prefix.contains('<') =>
        {
            (Some(prefix.to_string()), rest.to_string())
        }
        _ => (row.sender.clone(), content),
    }
}

/// A row's content as text: [`content_text`] of it.
fn text_of(row: &Row) -> String {
    content_text(&row.content, row.compressed)
}

/// `content`, a message's `message_content` as stored, as text:
/// zstd-decompressed when `compressed` (its `WCDB_CT_message_content`) says
/// it is, bytes that are not UTF-8 replaced.
pub fn content_text(content: &[u8], compressed: Option<i64>) -> String {
    let unzstd = |bytes: &[u8]| {
        let mut out = Vec::new();
        ruzstd::decoding::StreamingDecoder::new(bytes).ok()?.read_to_end(&mut out).ok()?;
        Some(out)
    };
    let bytes = match compressed {
        Some(wxread_schema::ZSTD) => unzstd(content).unwrap_or_else(|| content.to_vec()),
        _ => content.to_vec(),
    };
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The message databases `databases` (each a path under `db_storage`)
/// holds, by name (`message_0`), in order: those of people and groups,
/// then those of official accounts, each by its number.
pub fn message_dbs(databases: &[String]) -> Vec<String> {
    let mut found: Vec<(usize, u32, String)> = databases
        .iter()
        .filter_map(|path| {
            wxread_schema::MESSAGE_DBS.iter().enumerate().find_map(|(at, prefix)| {
                let n = path.strip_prefix(prefix)?.strip_suffix(".db")?;
                if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let name = path.strip_prefix("message/")?.strip_suffix(".db")?;
                Some((at, n.parse().ok()?, name.to_string()))
            })
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, _, name)| name).collect()
}

/// The path under `db_storage` of the message database named `name`.
pub fn db_path(name: &str) -> String {
    format!("message/{name}.db")
}

/// What fixes a chat none of the account's message databases has: one of
/// WeChat's folders is refused naming what reads it; any other is no chat
/// of the account's, or one WeChat on this Mac holds no message of.
pub fn no_such_chat(chat: &str, account: &str) -> String {
    if let Some(folder) = wxread_chats::Folder::of(chat) {
        return folder.not_a_chat();
    }
    format!(
        "account {account} has no messages of chat {chat}: it is no chat of the account's, or WeChat on this Mac holds none of its messages (they were never synced to this Mac, or were deleted); `wxread chats` lists the chats, and `wxread chats --query {chat}` answers whether it is one"
    )
}

#[cfg(test)]
mod tests;
