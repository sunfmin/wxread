//! `wxread messages`: reads one chat's messages out of the account's
//! message databases, its senders' names out of its contacts, and prints
//! the page the messages crate makes of them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::anyhow;
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension as _, params};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_media::Media;
use wxread_messages::{CHAT_FLAG, Chat, Contact, Cursor, Names, Order, Range, Row, Start};
use wxread_schema::CONTACT_DB;

use crate::account::Account;
use crate::media::{self, MEDIA_DB_PATHS};
use crate::world::World;

/// The message databases, as a refusal of missing keys names them.
pub const MESSAGE_DBS: &str = "message/message_<n>.db";

/// What `--account`, `--chat`, `--from`, `--to`, `--limit`, `--cursor`
/// and `--oldest-first` said.
pub struct Asked {
    pub account: Option<String>,
    pub chat: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
    pub oldest_first: bool,
}

/// What messages answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let chat = chat(asked.chat.as_deref())?;
    let limit = Limit::parse(asked.limit.as_deref(), "messages").map_err(|e| anyhow!(e))?;
    let range = Range::parse(asked.from.as_deref(), asked.to.as_deref()).map_err(|e| anyhow!(e))?;
    let order = Order::of(asked.oldest_first);
    let cursor = asked
        .cursor
        .as_deref()
        .map(|c| Cursor::parse(c, order))
        .transpose()
        .map_err(|e| anyhow!(e))?;
    let start = cursor.as_ref().map_or(Start::First(order), Start::After);
    let account =
        Account::open(world, asked.account.as_deref(), &[CONTACT_DB, MESSAGE_DBS, MEDIA_DB_PATHS])?;
    let page = Store::open(&account)?.page(&chat, range, start, limit)?;
    Ok(json!({
        "account": account.id(),
        "chat": crate::contacts::chat(&account, &chat)?,
        "messages": page.messages,
        "next": page.next.map(|n| n.text()),
    }))
}

/// An account's message databases, each read out of its file once, and
/// what names their senders: what reads the messages of one chat, or of
/// many without reading the databases again for each.
pub struct Store<'a> {
    account: &'a Account,
    contacts_db: Connection,
    contacts: BTreeMap<String, Contact>,
    /// Each message database: its name, its path under db_storage, and it.
    dbs: Vec<(String, String, Connection)>,
    /// Every id the message databases' `Name2Id` names.
    senders: Vec<String>,
}

impl<'a> Store<'a> {
    /// The message databases of `account`, and its contacts.
    pub fn open(account: &'a Account) -> anyhow::Result<Store<'a>> {
        let contacts_db = account.database(CONTACT_DB)?;
        let contacts = contacts(&contacts_db)?;
        let mut dbs = Vec::new();
        let mut senders = Vec::new();
        for db in wxread_messages::message_dbs(&account.databases()?) {
            let path = wxread_messages::db_path(&db);
            let conn = account.database(&path)?;
            let sql =
                |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
            senders.extend(ids(&conn).map_err(sql)?);
            dbs.push((db, path, conn));
        }
        Ok(Store { account, contacts_db, contacts, dbs, senders })
    }

    /// The page of chat `chat`'s messages from `start` within `range`, at
    /// most `limit`, as `wxread messages` answers them; [`NoMessages`] when
    /// WeChat on this Mac holds none of its messages.
    pub fn page(
        &self,
        chat: &str,
        range: Range,
        start: Start<'_>,
        limit: Limit,
    ) -> anyhow::Result<wxread_messages::Page> {
        let account = self.account;
        let names =
            Names { contacts: self.contacts.clone(), in_group: in_group(&self.contacts_db, chat)? };
        let table = wxread_schema::messages_table(chat);
        let mut rows = Vec::new();
        let mut found = false;
        for (db, path, conn) in &self.dbs {
            let sql =
                |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(path, &e.to_string()));
            if !has_table(conn, &table).map_err(sql)? {
                continue;
            }
            found = true;
            rows.extend(read(conn, db, &table, range, start, limit).map_err(sql)?);
        }
        if !found {
            return Err(NoMessages(wxread_messages::no_such_chat(chat, account.id())).into());
        }
        let own = wxread_messages::names::account_id(account.id(), |id| {
            names.contacts.contains_key(id) || self.senders.iter().any(|s| s == id)
        });
        let chat = Chat { id: chat.to_string(), account: own };
        // The media databases are read only when a voice message may be on
        // the page, to tell whether WeChat holds its sound.
        let voice = |r: &Row| {
            wxread_messages::kinds::split(r.local_type).0 == wxread_messages::kinds::VOICE
        };
        let voices = if rows.iter().any(voice) {
            media::voices(account, Some(&chat.id), None)?.into_iter().map(|(id, _)| id).collect()
        } else {
            BTreeSet::new()
        };
        let (dir, chat_dir) = (account.dir(), wxread_media::chat_dir(&chat.id));
        let held = |m: &Media, r: &Row| media::held(&dir, m, &chat_dir, r.server_id, &voices);
        Ok(wxread_messages::page(&chat, &names, rows, range, start, limit, held))
    }
}

/// The failure of a chat WeChat on this Mac holds no message of, as the
/// line that says so: told apart from the others by whoever only wants a
/// chat's newest message, if it has one.
#[derive(Debug)]
pub struct NoMessages(String);

impl std::fmt::Display for NoMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NoMessages {}

/// The chat `--chat` names, or what fixes it.
pub fn chat(given: Option<&str>) -> anyhow::Result<String> {
    given.filter(|c| !c.is_empty()).map(str::to_string).ok_or_else(|| {
        anyhow!(
            "no chat was named: pass {CHAT_FLAG} <id>, a chat's id as `wxread chats` lists it (a wxid, or a group's <n>@chatroom)"
        )
    })
}

/// The page's worth of rows of the chat's table `table` in message
/// database `db` from `start` within `range`: at most one more than
/// `limit`, in the order `start` reads, so the page knows whether another
/// follows.
fn read(
    conn: &Connection,
    db: &str,
    table: &str,
    range: Range,
    start: Start<'_>,
    limit: Limit,
) -> rusqlite::Result<Vec<Row>> {
    let below = start.after().map(|c| c.bound_in(db));
    let (past, way) = match start.order() {
        Order::NewestFirst => ("<", "DESC"),
        Order::OldestFirst => (">", "ASC"),
    };
    let mut query = conn.prepare(&format!(
        "{} WHERE (?1 IS NULL OR IFNULL(m.create_time, 0) >= ?1) AND (?2 IS NULL OR IFNULL(m.create_time, 0) < ?2)
           AND (?3 IS NULL OR (IFNULL(m.create_time, 0), IFNULL(m.sort_seq, 0), m.local_id) {past} (?3, ?4, ?5))
         ORDER BY 5 {way}, 4 {way}, 1 {way} LIMIT ?6",
        select(table)
    ))?;
    let more = i64::try_from(limit.get()).unwrap_or(i64::MAX).saturating_add(1);
    query
        .query_map(
            params![
                range.from,
                range.to,
                below.map(|b| b.0),
                below.map(|b| b.1),
                below.map(|b| b.2),
                more
            ],
            |r| row(db, r),
        )?
        .collect()
}

/// The row of the chat's table `table` in message database `db` whose
/// server id is `server_id`, if it has one.
pub fn by_server_id(
    conn: &Connection,
    db: &str,
    table: &str,
    server_id: i64,
) -> rusqlite::Result<Option<Row>> {
    conn.query_row(&format!("{} WHERE m.server_id = ?1 LIMIT 1", select(table)), [server_id], |r| {
        row(db, r)
    })
    .optional()
}

/// Whether the message database `conn` has the table `table`: a chat's,
/// when it holds some of its messages.
pub fn has_table(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
}

/// The query of a [`Row`] of the chat's table `table`, its sender's id
/// joined from `Name2Id`, before its `WHERE`.
pub(crate) fn select(table: &str) -> String {
    format!(
        "SELECT m.local_id, IFNULL(m.server_id, 0), IFNULL(m.local_type, 0), IFNULL(m.sort_seq, 0), IFNULL(m.create_time, 0), n.user_name, m.message_content, m.WCDB_CT_message_content, m.packed_info_data
         FROM {table} m LEFT JOIN Name2Id n ON n.rowid = m.real_sender_id"
    )
}

/// The [`Row`] of message database `db` that [`select`] read as `r`.
pub(crate) fn row(db: &str, r: &rusqlite::Row) -> rusqlite::Result<Row> {
    Ok(Row {
        db: db.to_string(),
        local_id: r.get(0)?,
        server_id: r.get(1)?,
        local_type: r.get(2)?,
        sort_seq: r.get(3)?,
        create_time: r.get(4)?,
        sender: r.get::<_, Option<String>>(5)?.filter(|s| !s.is_empty()),
        content: bytes(r.get(6)?),
        compressed: r.get(7)?,
        packed: bytes(r.get(8)?),
    })
}

/// A column's value as the bytes it holds.
pub(crate) fn bytes(value: Sql) -> Vec<u8> {
    match value {
        Sql::Text(s) => s.into_bytes(),
        Sql::Blob(b) => b,
        Sql::Integer(n) => n.to_string().into_bytes(),
        Sql::Real(x) => x.to_string().into_bytes(),
        Sql::Null => Vec::new(),
    }
}

/// Every id a message database's `Name2Id` names.
pub(crate) fn ids(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut query = conn.prepare("SELECT user_name FROM Name2Id")?;
    let ids = query.query_map([], |r| r.get::<_, Option<String>>(0))?;
    ids.filter_map(Result::transpose).collect()
}

/// What the account calls each contact, by its id.
pub(crate) fn contacts(db: &Connection) -> anyhow::Result<BTreeMap<String, Contact>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let mut query = db.prepare("SELECT username, remark, nick_name FROM contact").map_err(sql)?;
    let contacts = query
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                Contact { remark: r.get(1)?, nick_name: r.get(2)? },
            ))
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(contacts)
}

/// What each member of `chat` calls themselves in it, when it is a group.
pub(crate) fn in_group(db: &Connection, chat: &str) -> anyhow::Result<BTreeMap<String, String>> {
    if wxread_chats::Kind::of(chat) != wxread_chats::Kind::Group {
        return Ok(BTreeMap::new());
    }
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let buffer: Option<Vec<u8>> = db
        .query_row("SELECT ext_buffer FROM chat_room WHERE username = ?1", [chat], |r| r.get(0))
        .optional()
        .map_err(sql)?
        .flatten();
    Ok(buffer
        .map(|b| wxread_contacts::names_in_group(&b).into_iter().collect())
        .unwrap_or_default())
}
