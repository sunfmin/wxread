//! `wxread chats`: reads the account's session and contact databases and
//! prints the page of its chats the chats crate makes of them; of each
//! group on the page, its newest messages and whether the account spoke
//! in it, out of the message databases.

use std::collections::BTreeMap;

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension as _};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_chats::{Contact, Contacts, Cursor, Kind, Rooms, Row};
use wxread_contacts::Room;
use wxread_messages::{Names, RECENT};
use wxread_schema::{CONTACT_DB, SESSION_DB};

use crate::account::Account;
use crate::messages::{self, MESSAGE_DBS};
use crate::world::World;

/// What `--account`, `--with`, `--query`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub with: Vec<String>,
    pub query: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What chats answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "chats").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account =
        Account::open(world, asked.account.as_deref(), &[SESSION_DB, CONTACT_DB, MESSAGE_DBS])?;
    let sessions = account.database(SESSION_DB)?;
    let contact_db = account.database(CONTACT_DB)?;
    let contacts = contacts(&account)?;
    let rooms = rooms(&contact_db)?;
    let rows = rows(&sessions, &contacts)?;
    let query = asked.query.as_deref();
    let page = if asked.with.is_empty() {
        wxread_chats::page(rows, &rooms, query, cursor.as_ref(), limit)
    } else {
        wxread_chats::holding(rows, &contacts, &rooms, &asked.with, query, cursor.as_ref(), limit)
            .map_err(|e| anyhow!(e))?
    };
    let groups: Vec<&str> =
        page.chats.iter().filter(|c| c.kind == Kind::Group).map(|c| c.id.as_str()).collect();
    let mut spoken = spoken(&account, &contact_db, &groups)?;
    let chats: Vec<Value> = page
        .chats
        .iter()
        .map(|c| {
            let (recent, account_spoke) =
                spoken.remove(&c.id).unwrap_or((Value::Null, Value::Null));
            json!({
                "id": c.id,
                "display_name": c.display_name,
                "kind": c.kind.name(),
                "read_with": c.read_with,
                "last_message_at": c.last_message_at,
                "unread": c.unread,
                "members": c.members,
                "recent": recent,
                "account_spoke": account_spoke,
            })
        })
        .collect();
    Ok(json!({
        "account": account.id(),
        "chats": chats,
        "next": page.next.map(|n| n.text()),
    }))
}

/// Of each group of `groups`, by its id: who sent most of its newest
/// messages ([`wxread_messages::recent`], as `recent` answers it, null
/// for none), and whether the account sent any message of it this Mac
/// holds. Every message database is opened once, and only when there is
/// a group to read.
fn spoken(
    account: &Account,
    contact_db: &Connection,
    groups: &[&str],
) -> anyhow::Result<BTreeMap<String, (Value, Value)>> {
    if groups.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut dbs = Vec::new();
    let mut senders = Vec::new();
    for db in wxread_messages::message_dbs(&account.databases()?) {
        let path = wxread_messages::db_path(&db);
        let conn = account.database(&path)?;
        senders.extend(messages::ids(&conn).map_err(not_wechats(&path))?);
        dbs.push((db, path, conn));
    }
    let contacts = messages::contacts(contact_db)?;
    let own = wxread_messages::names::account_id(account.id(), |id| {
        contacts.contains_key(id) || senders.iter().any(|s| s == id)
    });
    let mut found = BTreeMap::new();
    for group in groups {
        let table = wxread_schema::messages_table(group);
        let (mut rows, mut spoke) = (Vec::new(), false);
        for (db, path, conn) in &dbs {
            let sql = not_wechats(path);
            if !messages::has_table(conn, &table).map_err(&sql)? {
                continue;
            }
            rows.extend(newest(conn, db, &table).map_err(&sql)?);
            spoke = spoke || said_any(conn, &table, &own).map_err(&sql)?;
        }
        rows.sort_by_key(|r| std::cmp::Reverse((r.create_time, r.sort_seq, r.local_id)));
        let chat = wxread_messages::Chat { id: group.to_string(), account: own.clone() };
        let names =
            Names { contacts: contacts.clone(), in_group: messages::in_group(contact_db, group)? };
        let recent = wxread_messages::recent(&chat, &rows).map_or(Value::Null, |r| {
            json!({
                "messages": r.messages,
                "top_sender": names.person(Some(&r.top_sender), None),
                "from_top_sender": r.from_top_sender,
            })
        });
        found.insert(group.to_string(), (recent, json!(spoke)));
    }
    Ok(found)
}

/// The [`RECENT`] newest rows of the chat's table `table` in message
/// database `db`, newest first.
fn newest(conn: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<wxread_messages::Row>> {
    let mut query = conn.prepare(&format!(
        "{} ORDER BY 5 DESC, 4 DESC, 1 DESC LIMIT ?1",
        messages::select(table)
    ))?;
    let limit = i64::try_from(RECENT).unwrap_or(i64::MAX);
    query.query_map([limit], |r| messages::row(db, r))?.collect()
}

/// Whether `own` sent any message of the chat's table `table`.
fn said_any(conn: &Connection, table: &str, own: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!(
            "SELECT 1 FROM {table} m JOIN Name2Id n ON n.rowid = m.real_sender_id WHERE n.user_name = ?1 LIMIT 1"
        ),
        [own],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
}

/// What a failed read of message database `path` is answered as.
fn not_wechats(path: &str) -> impl Fn(rusqlite::Error) -> anyhow::Error + '_ {
    move |e| anyhow!(wxread_account::not_wechats(path, &e.to_string()))
}

/// Every row of `SessionTable`, each with its contact from `contacts`.
fn rows(sessions: &Connection, contacts: &Contacts) -> anyhow::Result<Vec<Row>> {
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

/// What the account calls each contact, by its id: an unnamed group by
/// its members' names.
fn contacts(account: &Account) -> anyhow::Result<Contacts> {
    let rows = crate::contacts::contacts(account, false)?;
    Ok(rows
        .into_iter()
        .map(|(id, r)| (id, Contact { remark: r.remark, nick_name: r.name, members: r.members }))
        .collect())
}

/// The members of every group `chatroom_member` or `chat_room` keeps a
/// record of, by the group's id: two reads, whatever the groups.
fn rooms(db: &Connection) -> anyhow::Result<Rooms> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let mut records: std::collections::BTreeMap<String, Room> = Default::default();
    let mut query = db
        .prepare(
            "SELECT g.username, m.username FROM chatroom_member r JOIN contact g ON g.id = r.room_id JOIN contact m ON m.id = r.member_id",
        )
        .map_err(sql)?;
    let listed: Vec<(Option<String>, Option<String>)> = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .and_then(Iterator::collect)
        .map_err(sql)?;
    for (group, member) in listed {
        if let (Some(group), Some(member)) = (group, member) {
            records.entry(group).or_default().members.push(member);
        }
    }
    let mut query = db.prepare("SELECT username, ext_buffer FROM chat_room").map_err(sql)?;
    let buffers: Vec<(Option<String>, Option<Vec<u8>>)> = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .and_then(Iterator::collect)
        .map_err(sql)?;
    for (group, ext_buffer) in buffers {
        if let Some(group) = group {
            records.entry(group).or_default().ext_buffer = ext_buffer;
        }
    }
    records.remove("");
    Ok(records.into_iter().map(|(id, room)| (id, wxread_contacts::members(&room))).collect())
}
