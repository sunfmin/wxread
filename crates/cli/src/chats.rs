//! `wxread chats`: reads the account's session and contact databases and
//! prints the page of its chats the chats crate makes of them.

use std::collections::BTreeMap;

use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_chats::{Contact, Cursor, Row};
use wxread_page::Limit;
use wxread_schema::{CONTACT_DB, SESSION_DB};

use crate::account::Account;
use crate::world::World;

/// What `--account`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What chats answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "chats").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[SESSION_DB, CONTACT_DB])?;
    let sessions = account.database(SESSION_DB)?;
    let contact_db = account.database(CONTACT_DB)?;
    let rows = rows(&sessions, &contacts(&contact_db)?)?;
    let page = wxread_chats::page(rows, cursor.as_ref(), limit);
    let chats: Vec<Value> = page
        .chats
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "kind": c.kind.name(),
                "last_message_at": c.last_message_at,
                "unread": c.unread,
            })
        })
        .collect();
    Ok(json!({
        "account": account.id(),
        "chats": chats,
        "next": page.next.map(|n| n.text()),
    }))
}

/// Every row of `SessionTable`, each with its contact from `contacts`.
fn rows(sessions: &Connection, contacts: &BTreeMap<String, Contact>) -> anyhow::Result<Vec<Row>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(SESSION_DB, &e.to_string()));
    let mut query = sessions
        .prepare("SELECT username, unread_count, last_timestamp, sort_timestamp FROM SessionTable")
        .map_err(sql)?;
    let rows = query
        .query_map([], |r| {
            let id: Option<String> = r.get(0)?;
            let id = id.unwrap_or_default();
            Ok(Row {
                contact: contacts.get(&id).cloned(),
                id,
                unread: r.get(1)?,
                last_timestamp: r.get(2)?,
                sort_timestamp: r.get(3)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(rows)
}

/// What the account calls each contact, by its id.
fn contacts(db: &Connection) -> anyhow::Result<BTreeMap<String, Contact>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let mut query = db.prepare("SELECT username, remark, nick_name FROM contact").map_err(sql)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                Contact { remark: r.get(1)?, nick_name: r.get(2)? },
            ))
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(rows)
}
