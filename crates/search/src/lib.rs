//! What `wxread search` answers: the messages of every chat (or of one)
//! that a text finds, newest first, a page at a time, each with its chat
//! and the text it was found in.
//!
//! WeChat keeps a full-text index of the chats' messages
//! (`message/message_fts.db`): one entry a message, holding the text
//! WeChat searches it by. What the index holds is searched there, by that
//! text; what it has not indexed yet (the newest messages, a database it
//! does not cover) is read message by message and searched by the same
//! text, taken from its kind. The two never overlap, so a message is found
//! once. The effect crate reads the index's entries and the rows; nothing
//! here reads or writes.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use wxread_answer::{CURSOR_FLAG, Limit};
use wxread_media::{Held, Media};
use wxread_messages::{CHAT_FLAG, Chat, FROM_FLAG, Kind, Names, Range, Row, TO_FLAG};

/// What `searched` answers: WeChat's index searched, with what it has not
/// indexed yet read message by message; or, with no index, every message
/// read.
pub const BY_INDEX: &str = "wechat_index";
pub const BY_MESSAGE: &str = "messages";

/// The text searched for: found in a message's text whatever its case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    lower: String,
}

impl Query {
    /// The query `text` is, or what fixes it.
    pub fn parse(text: &str) -> Result<Query, String> {
        let lower = text.trim().to_lowercase();
        if lower.is_empty() {
            return Err(
                "no text was given to search for: pass it, `wxread search <text>`, quoted when it holds a space"
                    .into(),
            );
        }
        Ok(Query { lower })
    }

    /// Whether `text` holds the query, ignoring case.
    pub fn finds(&self, text: &str) -> bool {
        text.to_lowercase().contains(&self.lower)
    }
}

/// The text a message is searched by, as WeChat's index has it: a text's
/// or a quote's words, a link's title and summary, a file's name, a
/// place's name and address, a card's name, a forward's title and what it
/// holds, a note's paragraphs and its places' and files' names, a
/// transfer's or a red packet's memo. None for the kinds WeChat does not
/// index: media and notices.
pub fn searchable(parsed: &wxread_messages::kinds::Parsed) -> Option<String> {
    let field = |name: &str| parsed.fields.get(name).and_then(Value::as_str);
    let parts: Vec<&str> = match parsed.kind {
        Kind::Text | Kind::Quote => vec![field("text")].into_iter().flatten().collect(),
        Kind::Link => [field("title"), field("summary")].into_iter().flatten().collect(),
        Kind::File | Kind::Card => [field("name")].into_iter().flatten().collect(),
        Kind::Location => [field("name"), field("address")].into_iter().flatten().collect(),
        Kind::Transfer | Kind::RedPacket => [field("memo")].into_iter().flatten().collect(),
        Kind::Forward => {
            let held = parsed.fields.get("messages").and_then(Value::as_array);
            let held = held.into_iter().flatten().filter_map(|m| m["content"].as_str());
            field("title").into_iter().chain(held).collect()
        }
        Kind::Note => {
            let items = parsed.fields.get("items").and_then(Value::as_array);
            items.into_iter().flatten().flat_map(note_words).collect()
        }
        _ => Vec::new(),
    };
    let parts: Vec<&str> = parts.into_iter().filter(|p| !p.trim().is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// What a note's item is searched by: a paragraph's words, a place's name
/// and address, a file's name.
fn note_words(item: &Value) -> Vec<&str> {
    let of = |kind: &str, field: &str| item[kind][field].as_str();
    [of("text", "text"), of("location", "name"), of("location", "address"), of("file", "name")]
        .into_iter()
        .flatten()
        .collect()
}

/// One message found: its chat, its row, and the text it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub chat: String,
    pub row: Row,
    pub matched: String,
}

/// `row` of chat `chat`, read message by message, when `query` finds it
/// in its [`searchable`] text; `account` the account's own id.
pub fn found(query: &Query, account: &str, chat: &str, row: Row) -> Option<Found> {
    let read = wxread_messages::read(&Chat { id: chat.into(), account: account.into() }, &row);
    let matched = searchable(&read.parsed).filter(|text| query.finds(text))?;
    Some(Found { chat: chat.into(), row, matched })
}

/// Where a page ended: the last message it holds, by when it was sent,
/// its order in that second, its chat, and its row. Its text is the `next`
/// an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    create_time: i64,
    sort_seq: i64,
    db: String,
    local_id: i64,
    chat: String,
}

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        let parts: Vec<&str> = text.splitn(5, '.').collect();
        let cursor = match parts.as_slice() {
            [create_time, sort_seq, db, local_id, chat] if !db.is_empty() && !chat.is_empty() => {
                match (create_time.parse(), sort_seq.parse(), local_id.parse()) {
                    (Ok(create_time), Ok(sort_seq), Ok(local_id)) => Some(Cursor {
                        create_time,
                        sort_seq,
                        db: db.to_string(),
                        local_id,
                        chat: chat.to_string(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        };
        cursor.ok_or_else(|| {
            format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread search answered: pass the `next` of the answer before, with the same text, {CHAT_FLAG}, {FROM_FLAG} and {TO_FLAG}, or leave {CURSOR_FLAG} out to start at the newest message"
            )
        })
    }

    /// What `next` says of it. The chat's id is last: it may hold a dot.
    pub fn text(&self) -> String {
        format!(
            "{}.{}.{}.{}.{}",
            self.create_time, self.sort_seq, self.db, self.local_id, self.chat
        )
    }

    fn of(found: &Found) -> Cursor {
        Cursor {
            create_time: found.row.create_time,
            sort_seq: found.row.sort_seq,
            db: found.row.db.clone(),
            local_id: found.row.local_id,
            chat: found.chat.clone(),
        }
    }

    /// The bound of a row of chat `chat` in database `db` that comes after
    /// this cursor: its `(create_time, sort_seq, local_id)` is below the
    /// one answered.
    pub fn below_in(&self, chat: &str, db: &str) -> (i64, i64, i64) {
        let local_id = match (chat, db).cmp(&(self.chat.as_str(), self.db.as_str())) {
            Ordering::Less => i64::MAX,
            Ordering::Equal => self.local_id,
            Ordering::Greater => i64::MIN,
        };
        (self.create_time, self.sort_seq, local_id)
    }

    /// Whether the message at `at` comes after the one this cursor names.
    fn passed_by(&self, at: &Cursor) -> bool {
        order(at, self) == Ordering::Less
    }
}

/// Where messages stand, oldest first: by when each was sent, then by its
/// order in that second, then by its chat, database and row, so each has
/// one place across every chat.
fn order(a: &Cursor, b: &Cursor) -> Ordering {
    (a.create_time, a.sort_seq, a.chat.as_str(), a.db.as_str(), a.local_id).cmp(&(
        b.create_time,
        b.sort_seq,
        b.chat.as_str(),
        b.db.as_str(),
        b.local_id,
    ))
}

/// One entry of WeChat's index: the text WeChat searches a message by,
/// and where the message is: its chat's `session_id`, its row, and when it
/// was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub session: i64,
    pub local_id: i64,
    pub sort_seq: i64,
    pub create_time: i64,
}

/// One row of the index's ranges: chat `session`'s messages `first` to
/// `last` of the message database made at `stamp` are indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indexed {
    pub session: i64,
    pub stamp: i64,
    pub first: i64,
    pub last: i64,
}

/// One message database: its name (`message_0`), when WeChat made it (its
/// `TimeStamp`, None when it has none), and the chats it holds a table of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Db {
    pub name: String,
    pub stamp: Option<i64>,
    pub chats: BTreeSet<String>,
}

/// What WeChat's index holds: whose chat each `session_id` is, and which
/// of each chat's messages in each database it indexed. Empty for an
/// account WeChat keeps no index for: every message is then read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    chats: BTreeMap<i64, String>,
    covered: BTreeMap<(String, String), (i64, i64)>,
}

/// A message WeChat's index found: its chat, database and row, when it
/// was sent, and the text it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub chat: String,
    pub db: String,
    pub local_id: i64,
    pub matched: String,
    create_time: i64,
    sort_seq: i64,
}

impl Hit {
    /// Whether `row`, read where this hit says, is the message its entry
    /// indexed: that row, sent when the entry says, in its place in that
    /// second.
    pub fn is(&self, row: &Row) -> bool {
        row.local_id == self.local_id
            && row.create_time == self.create_time
            && row.sort_seq == self.sort_seq
    }

    fn at(&self) -> Cursor {
        Cursor {
            create_time: self.create_time,
            sort_seq: self.sort_seq,
            db: self.db.clone(),
            local_id: self.local_id,
            chat: self.chat.clone(),
        }
    }
}

impl Index {
    /// The index whose `name2id` is `sessions` (each rowid to its chat's
    /// id) and whose ranges are `ranges`, over the message databases
    /// `dbs`. A range covers the one database made at its stamp that holds
    /// its chat's table; one that names no such database, or several,
    /// covers nothing, and those messages are read one by one.
    pub fn new(sessions: BTreeMap<i64, String>, ranges: &[Indexed], dbs: &[Db]) -> Index {
        let mut covered = BTreeMap::new();
        for range in ranges.iter().filter(|r| r.first <= r.last) {
            let Some(chat) = sessions.get(&range.session) else { continue };
            let mut holding =
                dbs.iter().filter(|db| db.stamp == Some(range.stamp) && db.chats.contains(chat));
            if let (Some(db), None) = (holding.next(), holding.next()) {
                covered.insert((chat.clone(), db.name.clone()), (range.first, range.last));
            }
        }
        Index { chats: sessions, covered }
    }

    /// The rows of chat `chat` in database `db` the index holds, first to
    /// last; None for none, when each is read.
    pub fn covered(&self, chat: &str, db: &str) -> Option<(i64, i64)> {
        self.covered.get(&(chat.to_string(), db.to_string())).copied()
    }

    /// The databases of chat `chat` whose range holds its row `local_id`.
    /// An entry does not name its database, so where two ranges hold the
    /// row each is a place it may be, and the row read there tells
    /// ([`Hit::is`]).
    fn dbs_of(&self, chat: &str, local_id: i64) -> Vec<String> {
        let mut dbs = Vec::new();
        for ((c, db), &(first, last)) in &self.covered {
            if c == chat && first <= local_id && local_id <= last {
                dbs.push(db.clone());
            }
        }
        dbs
    }

    /// The entries of `entries` `query` finds, of chat `only` when one is
    /// named, sent within `range` and past `after`: each in the database
    /// the index says, newest first. One no range covers is left to be
    /// read with the rows the index does not hold.
    pub fn hits(
        &self,
        query: &Query,
        entries: Vec<Entry>,
        only: Option<&str>,
        range: Range,
        after: Option<&Cursor>,
    ) -> Vec<Hit> {
        let mut hits = Vec::new();
        for e in entries {
            let Some(chat) = self.chats.get(&e.session) else { continue };
            if only.is_some_and(|o| o != chat)
                || !range.holds(e.create_time)
                || !query.finds(&e.text)
            {
                continue;
            }
            for db in self.dbs_of(chat, e.local_id) {
                let hit = Hit {
                    chat: chat.clone(),
                    db,
                    local_id: e.local_id,
                    matched: e.text.clone(),
                    create_time: e.create_time,
                    sort_seq: e.sort_seq,
                };
                if after.is_none_or(|c| c.passed_by(&hit.at())) {
                    hits.push(hit);
                }
            }
        }
        hits.sort_by(|a, b| order(&b.at(), &a.at()));
        hits
    }
}

/// One page of what was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub found: Vec<Found>,
    /// Where the next page starts, when more was found past this one.
    pub next: Option<Cursor>,
}

/// The page of `found` sent within `range` that starts after `after` (at
/// the newest, for none) and holds at most `limit`: newest first across
/// every chat, each message once.
pub fn page(found: Vec<Found>, range: Range, after: Option<&Cursor>, limit: Limit) -> Page {
    let mut found: Vec<Found> = found
        .into_iter()
        .filter(|f| {
            range.holds(f.row.create_time) && after.is_none_or(|c| c.passed_by(&Cursor::of(f)))
        })
        .collect();
    found.sort_by(|a, b| order(&Cursor::of(b), &Cursor::of(a)));
    found.dedup_by(|a, b| order(&Cursor::of(a), &Cursor::of(b)) == Ordering::Equal);
    let more = found.len() > limit.get();
    found.truncate(limit.get());
    let next = if more { found.last().map(Cursor::of) } else { None };
    Page { found, next }
}

/// `found` as the answer has it: the message as `wxread messages` answers
/// it, with its chat's id and name, and the text it was found in;
/// `account` the account's own id, `names` the names of the chat's
/// senders, `chat_name` what WeChat shows the chat as, and `held` whether
/// WeChat holds a media message's file and thumbnail.
pub fn answer(
    found: &Found,
    account: &str,
    names: &Names,
    chat_name: Option<&str>,
    held: &impl Fn(&Media, &Row) -> Held,
) -> Value {
    let chat = Chat { id: found.chat.clone(), account: account.into() };
    let mut message = wxread_messages::message(&chat, names, &found.row, held);
    if let Value::Object(fields) = &mut message {
        fields.insert("chat".into(), json!(found.chat));
        fields.insert("chat_name".into(), json!(chat_name.filter(|n| !n.is_empty())));
        fields.insert("matched".into(), json!(found.matched));
    }
    message
}

#[cfg(test)]
mod tests;
