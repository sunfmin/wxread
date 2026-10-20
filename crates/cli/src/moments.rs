//! `wxread moments`: reads the Moments WeChat has loaded out of the
//! account's `sns/sns.db`, the names of their authors and of whoever liked
//! or commented out of its contacts, and prints the page the moments crate
//! makes of them, each medium with whether WeChat holds it.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension as _};
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_moments::{Cursor, Moment, Person, Row, Said};
use wxread_schema::{CONTACT_DB, SNS_DB};

use crate::account::Account;
use crate::media;
use crate::world::World;

/// What `--account`, `--contact`, `--limit` and `--cursor` said.
pub struct Asked {
    pub account: Option<String>,
    pub contact: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

/// What moments answers of `world`.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let limit = Limit::parse(asked.limit.as_deref(), "Moments").map_err(|e| anyhow!(e))?;
    let cursor = asked.cursor.as_deref().map(Cursor::parse).transpose().map_err(|e| anyhow!(e))?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, SNS_DB])?;
    let rows = if account.databases()?.iter().any(|d| d == SNS_DB) {
        let only = asked.contact.as_deref().filter(|c| !c.is_empty());
        rows(&account.database(SNS_DB)?, only, None)?
    } else {
        Vec::new()
    };
    let names = names(&account.database(CONTACT_DB)?)?;
    let page = wxread_moments::page(&rows, &names, cursor.as_ref(), limit);
    let dir = account.dir();
    let moments: Vec<Value> = page.moments.iter().map(|m| answer(m, &dir)).collect();
    Ok(json!({
        "account": account.id(),
        "moments": moments,
        "next": page.next.map(|n| n.text()),
    }))
}

/// `moment` as the answer has it, each medium with whether WeChat holds it
/// under the account directory `dir`.
fn answer(moment: &Moment, dir: &std::path::Path) -> Value {
    let person = |p: &Person| json!({"id": p.id, "name": p.name});
    let said = |all: &[Said]| -> Vec<Value> {
        all.iter()
            .map(|s| {
                json!({
                    "by": person(&s.by),
                    "at": s.at,
                    "text": s.text,
                    "reply_to": s.reply_to.as_ref().map(person),
                })
            })
            .collect()
    };
    let media: Vec<Value> = moment
        .media
        .iter()
        .map(|m| {
            let held = m.file.as_ref().map(|f| media::held(dir, f, "", 0, &BTreeSet::new()));
            json!({
                "id": m.id,
                "kind": m.kind,
                "url": m.url,
                "thumbnail_url": m.thumbnail_url,
                "width": m.width,
                "height": m.height,
                "file_present": held.map(|h| h.file),
                "thumbnail_present": held.and_then(|h| h.thumbnail),
            })
        })
        .collect();
    let likes: Vec<Value> =
        moment.likes.iter().map(|l| json!({"by": person(&l.by), "at": l.at})).collect();
    json!({
        "id": moment.id,
        "author": person(&moment.author),
        "at": moment.at,
        "text": moment.text,
        "location": moment.location.as_ref().map(|l| json!({
            "name": l.name,
            "address": l.address,
            "city": l.city,
            "latitude": l.latitude.as_deref().and_then(|d| d.parse::<f64>().ok()),
            "longitude": l.longitude.as_deref().and_then(|d| d.parse::<f64>().ok()),
        })),
        "link": moment.link.as_ref().map(|l| json!({
            "title": l.title,
            "description": l.description,
            "url": l.url,
        })),
        "media": media,
        "likes": likes,
        "comments": said(&moment.comments),
        "type": moment.kind,
        "content": moment.content,
    })
}

/// The Moments `sns/sns.db` holds, of `only`'s alone when it names an
/// author, or the one of `tid` when it names one.
pub fn rows(db: &Connection, only: Option<&str>, tid: Option<i64>) -> anyhow::Result<Vec<Row>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(SNS_DB, &e.to_string()));
    let mut query = db
        .prepare(
            "SELECT tid, IFNULL(user_name, ''), content FROM SnsTimeLine
             WHERE (?1 IS NULL OR user_name = ?1) AND (?2 IS NULL OR tid = ?2)",
        )
        .map_err(sql)?;
    query
        .query_map(rusqlite::params![only, tid], |r| {
            Ok(Row {
                tid: r.get(0)?,
                author: r.get(1)?,
                content: String::from_utf8_lossy(&crate::messages::bytes(r.get(2)?)).into_owned(),
            })
        })
        .and_then(Iterator::collect)
        .map_err(sql)
}

/// The Moment of `tid`, if `sns/sns.db` holds it.
pub fn one(db: &Connection, tid: i64) -> anyhow::Result<Option<Row>> {
    Ok(rows(db, None, Some(tid))?.into_iter().next())
}

/// What WeChat shows for each contact, by id: the account's remark, else
/// their own name.
pub fn names(db: &Connection) -> anyhow::Result<BTreeMap<String, String>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let mut query = db.prepare("SELECT username, remark, nick_name FROM contact").map_err(sql)?;
    let rows = query
        .query_map([], |r| {
            let (remark, name): (Option<String>, Option<String>) = (r.get(1)?, r.get(2)?);
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                wxread_contacts::display_name(remark.as_deref(), None, name.as_deref()),
            ))
        })
        .and_then(Iterator::collect)
        .map_err(sql)?;
    Ok(rows)
}

/// The name WeChat shows for `id`, when the account has a contact of them.
pub fn name_of(db: &Connection, id: &str) -> anyhow::Result<Option<String>> {
    let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(CONTACT_DB, &e.to_string()));
    let found: Option<(Option<String>, Option<String>)> = db
        .query_row("SELECT remark, nick_name FROM contact WHERE username = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(sql)?;
    Ok(found.map(|(remark, name)| {
        wxread_contacts::display_name(remark.as_deref(), None, name.as_deref())
    }))
}
