//! `wxread search`: reads WeChat's full-text index of the chats' messages
//! where the account has one, the rows it found and the rows it has not
//! indexed out of the message databases, and the names out of the contact
//! database, and prints the page the search crate makes of them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension as _, params};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_media::Media;
use wxread_messages::{Names, Range, Row};
use wxread_schema::{CONTACT_DB, FTS_CONTENT, MESSAGE_FTS_DB, messages_table};
use wxread_search::{BY_INDEX, BY_MESSAGE, Cursor, Db, Entry, Found, Index, Indexed, Query};

use crate::account::Account;
use crate::media;
use crate::messages::{MESSAGE_DBS, contacts, ids, in_group, row, select};
use crate::world::World;

/// What `<text>`, `--account`, `--chat`, `--from`, `--to`, `--limit` and
/// `--cursor` said.
pub struct Asked {
    pub text: Option<String>,
    pub account: Option<String>,
    pub chat: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// One message database, open: its name, its connection, and each chat
/// it holds a table of, by id.
struct Opened {
    name: String,
    conn: Connection,
    chats: BTreeSet<String>,
}

/// What search answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let text = asked.text.clone().unwrap_or_default();
    let query = Query::parse(&text).map_err(|e| anyhow!(e))?;
    let limit = Limit::parse(asked.limit.as_deref(), "messages").map_err(|e| anyhow!(e))?;
    let range = Range::parse(asked.from.as_deref(), asked.to.as_deref()).map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let only = asked.chat.as_deref().filter(|c| !c.is_empty());
    let account =
        Account::open(world, asked.account.as_deref(), &[CONTACT_DB, MESSAGE_DBS, MESSAGE_FTS_DB])?;
    let databases = account.databases()?;
    let contacts_db = account.database(CONTACT_DB)?;
    let contacts = contacts(&contacts_db)?;
    let mut opened = Vec::new();
    let mut dbs = Vec::new();
    let mut senders = Vec::new();
    for name in wxread_messages::message_dbs(&databases) {
        let path = wxread_messages::db_path(&name);
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        let named = ids(&conn).map_err(sql)?;
        let tables = tables(&conn).map_err(sql)?;
        // A chat's table is named by its id, which its Name2Id, a contact
        // or --chat gives.
        let known = named.iter().chain(contacts.keys()).map(String::as_str).chain(only);
        let chats: BTreeSet<String> =
            known.filter(|id| tables.contains(&messages_table(id))).map(String::from).collect();
        let stamp: Option<i64> = conn
            .query_row("SELECT timestamp FROM TimeStamp", [], |r| r.get::<_, Option<i64>>(0))
            .optional()
            .or_else(|e| no_table(e, None))
            .map_err(sql)?
            .flatten();
        dbs.push(Db { name: name.clone(), stamp, chats: chats.clone() });
        senders.extend(named);
        opened.push(Opened { name, conn, chats });
    }
    if let Some(chat) = only
        && !opened.iter().any(|db| db.chats.contains(chat))
    {
        return Err(anyhow!(wxread_messages::no_such_chat(chat, account.id())));
    }
    let own = wxread_messages::names::account_id(account.id(), |id| {
        contacts.contains_key(id) || senders.iter().any(|s| s == id)
    });
    let (index, entries, searched) = if databases.iter().any(|d| d == MESSAGE_FTS_DB) {
        let (index, entries) = index(&account.database(MESSAGE_FTS_DB)?, &dbs)?;
        (index, entries, BY_INDEX)
    } else {
        (Index::default(), Vec::new(), BY_MESSAGE)
    };
    let mut found = Vec::new();
    // What the index found, each row read where it says, until a page's
    // worth and one more is there: what it found past them is older.
    for hit in index.hits(&query, entries, only, range, cursor.as_ref()) {
        if found.len() > limit.get() {
            break;
        }
        let Some(db) = opened.iter().find(|db| db.name == hit.db) else { continue };
        let path = wxread_messages::db_path(&db.name);
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        let row = one(&db.conn, &db.name, &hit.chat, hit.local_id).map_err(sql)?;
        if let Some(row) = row.filter(|r| hit.is(r)) {
            found.push(Found { chat: hit.chat, row, matched: hit.matched });
        }
    }
    // What the index does not hold, read message by message.
    for db in &opened {
        let path = wxread_messages::db_path(&db.name);
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        for chat in db.chats.iter().filter(|c| only.is_none_or(|o| o == *c)) {
            let unindexed = Unindexed {
                chat,
                covered: index.covered(chat, &db.name),
                range,
                cursor: cursor.as_ref(),
            };
            let rows = scan(
                &db.conn,
                &db.name,
                &unindexed,
                |row| wxread_search::found(&query, &own, chat, row),
                limit,
            )
            .map_err(sql)?;
            found.extend(rows);
        }
    }
    let page = wxread_search::page(found, range, cursor.as_ref(), limit);
    let mut names: BTreeMap<String, Names> = BTreeMap::new();
    let shown_chats = crate::contacts::display_names(&account)?;
    // A voice message has no text to be found by, so no voice is on the
    // page and the media databases are not read.
    let (dir, voices) = (account.dir(), BTreeSet::new());
    let mut messages = Vec::new();
    for f in &page.found {
        if !names.contains_key(&f.chat) {
            let in_group = in_group(&contacts_db, &f.chat)?;
            names.insert(f.chat.clone(), Names { contacts: contacts.clone(), in_group });
        }
        let chat_dir = wxread_media::chat_dir(&f.chat);
        let held = |m: &Media, r: &Row| media::held(&dir, m, &chat_dir, r.server_id, &voices);
        let shown = &names[&f.chat];
        let chat_name = shown_chats.get(&f.chat).map(String::as_str);
        messages.push(wxread_search::answer(f, &own, shown, chat_name, &held));
    }
    Ok(json!({
        "account": account.id(),
        "query": text,
        "searched": searched,
        "messages": messages,
        "next": page.next.map(|n| n.text()),
    }))
}

/// Every table a database has, by name.
fn tables(conn: &Connection) -> rusqlite::Result<BTreeSet<String>> {
    let mut query = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let names = query.query_map([], |r| r.get::<_, String>(0))?;
    names.collect()
}

/// `fallback` for a query of a table the database lacks; any other
/// failure as it is.
fn no_table<T>(e: rusqlite::Error, fallback: T) -> rusqlite::Result<T> {
    match &e {
        rusqlite::Error::SqliteFailure(_, Some(why)) if why.starts_with("no such table") => {
            Ok(fallback)
        }
        _ => Err(e),
    }
}

/// WeChat's index in `fts`, over the message databases `dbs`, and every
/// entry it holds.
fn index(fts: &Connection, dbs: &[Db]) -> anyhow::Result<(Index, Vec<Entry>)> {
    let sql =
        |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(MESSAGE_FTS_DB, &e.to_string()));
    let sessions: BTreeMap<i64, String> = fts
        .prepare("SELECT rowid, username FROM name2id WHERE username IS NOT NULL")
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(sql)?;
    let ranges: Vec<Indexed> = fts
        .prepare("SELECT session_id, db_time_stamp, start_local_id, end_local_id FROM message_fts_v4_range WHERE session_id IS NOT NULL AND db_time_stamp IS NOT NULL AND start_local_id IS NOT NULL AND end_local_id IS NOT NULL")
        .and_then(|mut q| {
            q.query_map([], |r| {
                Ok(Indexed { session: r.get(0)?, stamp: r.get(1)?, first: r.get(2)?, last: r.get(3)? })
            })?
            .collect()
        })
        .map_err(sql)?;
    let index = Index::new(sessions, &ranges, dbs);
    let mut entries = Vec::new();
    for table in tables(fts).map_err(sql)? {
        let number = table.strip_prefix(FTS_CONTENT.0).and_then(|t| t.strip_suffix(FTS_CONTENT.1));
        if !number.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let mut query = fts
            .prepare(&format!(
                "SELECT c0, c4, c1, IFNULL(c2, 0), IFNULL(c6, 0) FROM '{table}' WHERE c0 IS NOT NULL AND c1 IS NOT NULL AND c4 IS NOT NULL"
            ))
            .map_err(sql)?;
        let rows = query
            .query_map([], |r| {
                Ok(Entry {
                    text: String::from_utf8_lossy(&crate::messages::bytes(r.get(0)?)).into_owned(),
                    session: r.get(1)?,
                    local_id: r.get(2)?,
                    sort_seq: r.get(3)?,
                    create_time: r.get(4)?,
                })
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(sql)?;
        entries.extend(rows);
    }
    Ok((index, entries))
}

/// Row `local_id` of chat `chat`'s table in message database `db`, when
/// it is there.
fn one(conn: &Connection, db: &str, chat: &str, local_id: i64) -> rusqlite::Result<Option<Row>> {
    let table = messages_table(chat);
    conn.query_row(&format!("{} WHERE m.local_id = ?1", select(&table)), [local_id], |r| row(db, r))
        .optional()
}

/// The rows of one chat in one database to read message by message: those
/// outside what the index holds of it (`covered`, first to last), sent
/// within `range`, past `cursor`.
struct Unindexed<'a> {
    chat: &'a str,
    covered: Option<(i64, i64)>,
    range: Range,
    cursor: Option<&'a Cursor>,
}

/// What `find` finds among the rows `unindexed` names in database `db`,
/// read newest first until a page's worth and one more is found.
fn scan(
    conn: &Connection,
    db: &str,
    unindexed: &Unindexed,
    find: impl Fn(Row) -> Option<Found>,
    limit: Limit,
) -> rusqlite::Result<Vec<Found>> {
    let table = messages_table(unindexed.chat);
    let below = unindexed.cursor.map(|c| c.below_in(unindexed.chat, db));
    let mut query = conn.prepare(&format!(
        "{} WHERE (?1 IS NULL OR IFNULL(m.create_time, 0) >= ?1) AND (?2 IS NULL OR IFNULL(m.create_time, 0) < ?2)
           AND (?3 IS NULL OR (IFNULL(m.create_time, 0), IFNULL(m.sort_seq, 0), m.local_id) < (?3, ?4, ?5))
           AND (?6 IS NULL OR m.local_id < ?6 OR m.local_id > ?7)
         ORDER BY 5 DESC, 4 DESC, 1 DESC",
        select(&table)
    ))?;
    let rows = query.query_map(
        params![
            unindexed.range.from,
            unindexed.range.to,
            below.map(|b| b.0),
            below.map(|b| b.1),
            below.map(|b| b.2),
            unindexed.covered.map(|c| c.0),
            unindexed.covered.map(|c| c.1),
        ],
        |r| row(db, r),
    )?;
    let mut found = Vec::new();
    for row in rows {
        if let Some(f) = find(row?) {
            found.push(f);
            if found.len() > limit.get() {
                break;
            }
        }
    }
    Ok(found)
}
