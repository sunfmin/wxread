//! `wxread articles`: reads the messages official accounts sent out of the
//! account's official accounts' stores, and their names out of its contact
//! database, and prints the page of articles the articles crate makes of
//! them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_articles::{Cursor, Message};
use wxread_schema::{CONTACT_DB, ZSTD, is_biz_message_db, messages_table};

use crate::account::Account;
use crate::world::World;

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
    let account = Account::open_with(
        world,
        asked.account.as_deref(),
        &[CONTACT_DB],
        Some(is_biz_message_db),
    )?;
    let mut sent = Vec::new();
    for path in account.matching() {
        sent.extend(messages(&account.database(path)?, path, asked.official_account.as_deref())?);
    }
    let names = names(&account.database(CONTACT_DB)?)?;
    let page = wxread_articles::page(&sent, &names, cursor.as_ref(), limit);
    let articles: Vec<Value> = page
        .articles
        .iter()
        .map(|a| {
            json!({
                "official_account": {"id": a.official_account, "name": a.name},
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
                    content: content(r.get(3)?, r.get(4)?),
                })
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(sql)?;
        sent.extend(rows);
    }
    Ok(sent)
}

/// What the account calls each official account it has a contact of, by
/// its id, as WeChat shows it.
fn names(db: &Connection) -> anyhow::Result<BTreeMap<String, String>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let mut query = db
        .prepare("SELECT username, remark, nick_name FROM contact WHERE username LIKE 'gh\\_%' ESCAPE '\\'")
        .map_err(sql)?;
    let rows = query
        .query_map([], |r| {
            let (remark, name): (Option<String>, Option<String>) = (r.get(1)?, r.get(2)?);
            Ok((
                r.get::<_, String>(0)?,
                wxread_contacts::display_name(remark.as_deref(), None, name.as_deref()),
            ))
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(rows)
}

/// A message's content as text: zstd-decompressed when `compressed` says
/// it is, bytes that are not UTF-8 replaced.
fn content(raw: rusqlite::types::Value, compressed: Option<i64>) -> String {
    use rusqlite::types::Value as Sql;
    let bytes = match raw {
        Sql::Text(s) => s.into_bytes(),
        Sql::Blob(b) => b,
        Sql::Integer(n) => n.to_string().into_bytes(),
        Sql::Real(x) => x.to_string().into_bytes(),
        Sql::Null => Vec::new(),
    };
    let bytes = if compressed == Some(ZSTD) { unzstd(&bytes).unwrap_or(bytes) } else { bytes };
    String::from_utf8_lossy(&bytes).into_owned()
}

fn unzstd(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut out = Vec::new();
    ruzstd::decoding::StreamingDecoder::new(bytes).ok()?.read_to_end(&mut out).ok()?;
    Some(out)
}
