//! `wxread chats`: reads the account's session and contact databases and
//! prints the page of its chats the chats crate makes of them.

use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_chats::{Contact, Contacts, Cursor, Rooms, Row};
use wxread_contacts::Room;
use wxread_schema::{CONTACT_DB, SESSION_DB};

use crate::account::Account;
use crate::world::World;

/// What `--account`, `--with`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub with: Vec<String>,
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
    let contacts = contacts(&account)?;
    let rooms = rooms(&contact_db)?;
    let rows = rows(&sessions, &contacts)?;
    let page = if asked.with.is_empty() {
        wxread_chats::page(rows, &rooms, cursor.as_ref(), limit)
    } else {
        wxread_chats::holding(rows, &contacts, &rooms, &asked.with, cursor.as_ref(), limit)
            .map_err(|e| anyhow!(e))?
    };
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
                "members": c.members,
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
