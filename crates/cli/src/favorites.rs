//! `wxread favorites`: reads what the account saved out of its favorites
//! database and its contacts' names out of its contact database, looks for
//! the files of the page's favorites where WeChat keeps them, and prints
//! the page the favorites crate makes of them; and the one favorite whose
//! file `wxread file get fav:<id>:<item>` writes.

use std::collections::BTreeSet;

use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_favorites::{Cursor, Row, Seen};
use wxread_schema::{CONTACT_DB, FAVORITE_DB};

use crate::account::Account;
use crate::world::World;

/// What `--account`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What favorites answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "favorites").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, FAVORITE_DB])?;
    let Some(db) = database(&account)? else {
        return Ok(json!({ "account": account.id(), "favorites": [], "next": null }));
    };
    let rows = rows(&db, None).map_err(sql)?;
    let page = wxread_favorites::page(rows, cursor.as_ref(), limit);
    let names = crate::contacts::display_names(&account)?;
    let own = wxread_messages::names::account_id(account.id(), |id| names.contains_key(id));
    let dir = account.dir();
    let held: BTreeSet<String> = page
        .rows
        .iter()
        .flat_map(wxread_favorites::paths)
        .filter(|p| dir.join(p).is_file())
        .collect();
    let seen = Seen { own: &own, names: &names, held: &held };
    let favorites: Vec<Value> =
        page.rows.iter().map(|row| wxread_favorites::favorite(row, &seen)).collect();
    Ok(json!({
        "account": account.id(),
        "favorites": favorites,
        "next": page.next.map(|n| n.text()),
    }))
}

/// The favorites database, opened: None when the account has none, as
/// one that never saved anything may not; a refusal naming how to get its
/// key when the keys hold none of it.
pub fn database(account: &Account) -> anyhow::Result<Option<Connection>> {
    if !account.databases()?.iter().any(|d| d == FAVORITE_DB) {
        return Ok(None);
    }
    if !account.has_key(FAVORITE_DB) {
        return Err(anyhow!(wxread_favorites::no_key(FAVORITE_DB, account.id())));
    }
    account.database(FAVORITE_DB).map(Some)
}

/// Every favorite in `db`, or with `id` that one alone.
pub fn rows(db: &Connection, id: Option<i64>) -> rusqlite::Result<Vec<Row>> {
    let mut query = db.prepare(
        "SELECT local_id, IFNULL(type, 0), IFNULL(update_time, 0), IFNULL(content, ''), fromusr, realchatname
         FROM fav_db_item WHERE ?1 IS NULL OR local_id = ?1",
    )?;
    query
        .query_map([id], |r| {
            Ok(Row {
                local_id: r.get(0)?,
                fav_type: r.get(1)?,
                update_time: r.get(2)?,
                content: r.get(3)?,
                from: r.get(4)?,
                sender: r.get(5)?,
            })
        })?
        .collect()
}

/// The favorite `id` in `db`.
pub fn row(db: &Connection, id: i64) -> anyhow::Result<Option<Row>> {
    Ok(rows(db, Some(id)).map_err(sql)?.into_iter().next())
}

/// A query of the favorites database that failed.
fn sql(e: rusqlite::Error) -> anyhow::Error {
    anyhow!(wxread_account::not_wechats(FAVORITE_DB, &e.to_string()))
}
