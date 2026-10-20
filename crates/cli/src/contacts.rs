//! `wxread contacts`, `contact` and `group`: read the account's contact
//! database (and for a contact's avatar, its head image database) and
//! print what the contacts crate makes of them.

use std::collections::BTreeSet;

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_contacts::{Contact, Contacts, Cursor, Group, NoticeRow, Person, Room, Row};
use wxread_schema::{CONTACT_DB, HEAD_IMAGE_DB};

use crate::account::Account;
use crate::world::World;

/// What `contacts` was asked: `--account`, `--query`, `--limit` and
/// `--cursor`.
pub struct Asked {
    pub account: Option<String>,
    pub query: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What `contacts` answers of `world`.
pub fn list(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "contacts").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, HEAD_IMAGE_DB])?;
    let page = wxread_contacts::page(
        contacts(&account, true)?,
        asked.query.as_deref(),
        cursor.as_ref(),
        limit,
    );
    Ok(json!({
        "account": account.id(),
        "contacts": page.contacts.iter().map(contact_json).collect::<Vec<_>>(),
        "next": page.next.map(|n| n.text()),
    }))
}

/// What `contact <id>` answers of `world`.
pub fn one(world: &World, account: Option<&str>, id: &str) -> anyhow::Result<Value> {
    let account = Account::open(world, account, &[CONTACT_DB, HEAD_IMAGE_DB])?;
    let contact = wxread_contacts::find(&contacts(&account, true)?, id).map_err(|e| anyhow!(e))?;
    Ok(json!({"account": account.id(), "contact": contact_json(&contact)}))
}

/// What `group <id>` answers of `world`.
pub fn group(world: &World, account: Option<&str>, id: &str) -> anyhow::Result<Value> {
    let account = Account::open(world, account, &[CONTACT_DB])?;
    let contacts = contacts(&account, false)?;
    let db = account.database(CONTACT_DB)?;
    let group = wxread_contacts::group(id, &contacts, room(&db, id)?, notice(&db, id)?)
        .map_err(|e| anyhow!(e))?;
    Ok(json!({"account": account.id(), "group": group_json(&group)}))
}

fn contact_json(c: &Contact) -> Value {
    json!({
        "id": c.id,
        "display_name": c.display_name,
        "name": c.name,
        "remark": c.remark,
        "alias": c.alias,
        "signature": c.signature,
        "kind": c.kind.name(),
        "has_avatar": c.has_avatar,
    })
}

fn group_json(g: &Group) -> Value {
    let person = |p: &Person| json!({"id": p.id, "display_name": p.display_name});
    let members: Vec<Value> = g
        .members
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "display_name": m.display_name,
                "name": m.name,
                "name_in_group": m.name_in_group,
                "remark": m.remark,
            })
        })
        .collect();
    json!({
        "id": g.id,
        "display_name": g.display_name,
        "name": g.name,
        "remark": g.remark,
        "owner": g.owner.as_ref().map(person),
        "notice": g.notice.as_ref().map(|n| json!({
            "text": n.text,
            "by": n.by.as_ref().map(person),
            "at": n.at,
        })),
        "members": members,
    })
}

fn sql(e: rusqlite::Error) -> anyhow::Error {
    anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()))
}

/// The rows of `contact`, by id; each with whether WeChat holds its
/// avatar when `avatars` asks, which reads the head image database.
fn contacts(account: &Account, avatars: bool) -> anyhow::Result<Contacts> {
    let held = if avatars { held(&account.database(HEAD_IMAGE_DB)?)? } else { BTreeSet::new() };
    let db = account.database(CONTACT_DB)?;
    let mut query = db
        .prepare("SELECT username, alias, nick_name, remark, description FROM contact")
        .map_err(sql)?;
    let rows: Vec<Row> = query
        .query_map([], |r| {
            let id = r.get::<_, Option<String>>(0)?.unwrap_or_default();
            Ok(Row {
                has_avatar: held.contains(&id),
                id,
                alias: r.get(1)?,
                name: r.get(2)?,
                remark: r.get(3)?,
                signature: r.get(4)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(wxread_contacts::by_id(rows))
}

/// The ids of the contacts `head_image` holds an avatar of.
fn held(db: &Connection) -> anyhow::Result<BTreeSet<String>> {
    let sql =
        |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(HEAD_IMAGE_DB, &e.to_string()));
    let mut query = db
        .prepare("SELECT username FROM head_image WHERE length(image_buffer) > 0")
        .map_err(sql)?;
    let ids = query
        .query_map([], |r| r.get::<_, Option<String>>(0))
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(sql)?;
    Ok(ids.into_iter().flatten().collect())
}

/// Group `id`'s own record and the members `chatroom_member` lists:
/// nothing of either when WeChat keeps none.
fn room(db: &Connection, id: &str) -> anyhow::Result<Room> {
    let mut query = db
        .prepare(
            "SELECT m.username FROM chatroom_member r JOIN contact g ON g.id = r.room_id JOIN contact m ON m.id = r.member_id WHERE g.username = ?1",
        )
        .map_err(sql)?;
    let members: Vec<Option<String>> =
        query.query_map([id], |r| r.get(0)).and_then(Iterator::collect).map_err(sql)?;
    let (owner, ext_buffer) = db
        .query_row("SELECT owner, ext_buffer FROM chat_room WHERE username = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(sql)?
        .unwrap_or_default();
    Ok(Room { owner, ext_buffer, members: members.into_iter().flatten().collect() })
}

/// Group `id`'s notice, as `chat_room_info_detail` keeps it.
fn notice(db: &Connection, id: &str) -> anyhow::Result<Option<NoticeRow>> {
    db.query_row(
        "SELECT announcement_, announcement_editor_, announcement_publish_time_ FROM chat_room_info_detail WHERE username_ = ?1",
        [id],
        |r| Ok(NoticeRow { text: r.get(0)?, editor: r.get(1)?, published: r.get(2)? }),
    )
    .optional()
    .map_err(sql)
}
