//! `wxread messages`: reads one chat's messages out of the account's
//! message databases, its senders' names out of its contacts, and prints
//! the page the messages crate makes of them.

use std::collections::BTreeMap;

use anyhow::anyhow;
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension as _, params};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_messages::{CHAT_FLAG, Chat, Contact, Cursor, Names, Range, Row};
use wxread_schema::CONTACT_DB;

use crate::account::Account;
use crate::world::World;

/// The message databases, as a refusal of missing keys names them.
const MESSAGE_DBS: &str = "message/message_<n>.db";

/// What `--account`, `--chat`, `--from`, `--to`, `--limit` and `--cursor`
/// said.
pub struct Asked {
    pub account: Option<String>,
    pub chat: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What messages answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let chat = asked.chat.clone().filter(|c| !c.is_empty()).ok_or_else(|| {
        anyhow!(
            "no chat was named: pass {CHAT_FLAG} <id>, a chat's id as `wxread chats` lists it (a wxid, or a group's <n>@chatroom)"
        )
    })?;
    let limit = Limit::parse(asked.limit.as_deref(), "messages").map_err(|e| anyhow!(e))?;
    let range = Range::parse(asked.from.as_deref(), asked.to.as_deref()).map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, MESSAGE_DBS])?;
    let contacts_db = account.database(CONTACT_DB)?;
    let names =
        Names { contacts: contacts(&contacts_db)?, in_group: in_group(&contacts_db, &chat)? };
    let table = wxread_schema::messages_table(&chat);
    let mut rows = Vec::new();
    let mut found = false;
    let mut senders = Vec::new();
    for db in wxread_messages::message_dbs(&account.databases()?) {
        let path = wxread_messages::db_path(&db);
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        let has: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [&table],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        senders.extend(ids(&conn).map_err(sql)?);
        if has.is_none() {
            continue;
        }
        found = true;
        rows.extend(read(&conn, &db, &table, range, cursor.as_ref(), limit).map_err(sql)?);
    }
    if !found {
        return Err(anyhow!(wxread_messages::no_such_chat(&chat, account.id())));
    }
    let own = wxread_messages::names::account_id(account.id(), |id| {
        names.contacts.contains_key(id) || senders.iter().any(|s| s == id)
    });
    let chat = Chat { id: chat, account: own };
    let page = wxread_messages::page(&chat, &names, rows, range, cursor.as_ref(), limit);
    Ok(json!({
        "account": account.id(),
        "chat": chat.id,
        "messages": page.messages,
        "next": page.next.map(|n| n.text()),
    }))
}

/// The page's worth of rows of the chat's table `table` in message
/// database `db` past `cursor` within `range`: at most one more than
/// `limit`, newest first, so the page knows whether another follows.
fn read(
    conn: &Connection,
    db: &str,
    table: &str,
    range: Range,
    cursor: Option<&Cursor>,
    limit: Limit,
) -> rusqlite::Result<Vec<Row>> {
    let below = cursor.map(|c| c.below_in(db));
    let mut query = conn.prepare(&format!(
        "SELECT {COLUMNS}
         FROM {table} m LEFT JOIN Name2Id n ON n.rowid = m.real_sender_id
         WHERE (?1 IS NULL OR IFNULL(m.create_time, 0) >= ?1) AND (?2 IS NULL OR IFNULL(m.create_time, 0) < ?2)
           AND (?3 IS NULL OR (IFNULL(m.create_time, 0), IFNULL(m.sort_seq, 0), m.local_id) < (?3, ?4, ?5))
         ORDER BY 5 DESC, 4 DESC, 1 DESC LIMIT ?6"
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
            row_of(db),
        )?
        .collect()
}

/// The columns a message's [`Row`] is read from, of its table `m` and its
/// sender's `Name2Id` row `n`, in the order [`row_of`] reads them.
pub(crate) const COLUMNS: &str = "m.local_id, IFNULL(m.server_id, 0), IFNULL(m.local_type, 0), IFNULL(m.sort_seq, 0), IFNULL(m.create_time, 0), n.user_name, m.message_content, m.WCDB_CT_message_content, m.packed_info_data";

/// What reads a row of [`COLUMNS`] in message database `db`.
pub(crate) fn row_of(db: &str) -> impl Fn(&rusqlite::Row) -> rusqlite::Result<Row> + '_ {
    move |r| {
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
