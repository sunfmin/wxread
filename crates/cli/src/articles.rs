//! `wxread articles`: reads the messages official accounts sent out of the
//! account's official accounts' stores, and their names out of its contact
//! database, and prints the page of articles the articles crate makes of
//! them.

use std::collections::BTreeSet;

use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_articles::{Cursor, Message};
use wxread_schema::{CONTACT_DB, MESSAGE_DBS, messages_table};

use crate::account::Account;
use crate::world::World;

/// The official accounts' stores, as a refusal of missing keys names them.
const BIZ_MESSAGE_DBS: &str = "message/biz_message_<n>.db";

/// What `--account`, `--official-account`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub official_account: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What articles answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "articles").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, BIZ_MESSAGE_DBS])?;
    let mut sent = Vec::new();
    for db in wxread_messages::message_dbs(&account.databases()?) {
        let path = wxread_messages::db_path(&db);
        if path.starts_with(MESSAGE_DBS[1]) {
            let only = asked.official_account.as_deref();
            sent.extend(messages(&account.database(&path)?, &path, only)?);
        }
    }
    let names = crate::contacts::display_names(&account)?;
    let page = wxread_articles::page(&sent, &names, cursor.as_ref(), limit);
    let articles: Vec<Value> = page
        .articles
        .iter()
        .map(|a| {
            json!({
                "official_account": {"id": a.official_account, "display_name": a.display_name},
                "sent_at": a.sent_at,
                "title": a.title,
                "digest": a.digest,
                "url": a.url,
                "cover": a.cover,
            })
        })
        .collect();
    Ok(json!({
        "account": account.id(),
        "articles": articles,
        "next": page.next.map(|n| n.text()),
    }))
}

/// Every message of each official account the store `db` (at `path`)
/// holds, or of `only` alone: each one `Name2Id` names whose table of
/// messages is there.
fn messages(db: &Connection, path: &str, only: Option<&str>) -> anyhow::Result<Vec<Message>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(path, &e.to_string()));
    let tables: BTreeSet<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'Msg\\_%' ESCAPE '\\'")
        .and_then(|mut q| q.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>())
        .map_err(sql)?;
    let ids: Vec<String> = db
        .prepare("SELECT user_name FROM Name2Id")
        .and_then(|mut q| {
            q.query_map([], |r| r.get::<_, Option<String>>(0))?.collect::<Result<Vec<_>, _>>()
        })
        .map_err(sql)?
        .into_iter()
        .flatten()
        .filter(|id: &String| only.is_none_or(|o| o == id))
        .collect();
    let mut sent = Vec::new();
    for id in ids {
        let table = messages_table(&id);
        if !tables.contains(&table) {
            continue;
        }
        let mut query = db
            .prepare(&format!(
                "SELECT local_id, local_type, create_time, message_content, WCDB_CT_message_content FROM {table}"
            ))
            .map_err(sql)?;
        let rows = query
            .query_map([], |r| {
                Ok(Message {
                    official_account: id.clone(),
                    local_id: r.get(0)?,
                    local_type: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    create_time: r.get(2)?,
                    content: crate::messages::bytes(r.get(3)?),
                    compressed: r.get(4)?,
                })
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(sql)?;
        sent.extend(rows);
    }
    Ok(sent)
}
