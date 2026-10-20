//! `wxread file get` and `wxread avatar`: find one media message's file
//! (or its thumbnail) where WeChat keeps it, or a favorite's, or a
//! contact's avatar in the head image database, and write it at `--out` as
//! the file it is.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension as _};
use serde_json::{Value, json};
use wxread_favorites::FileId;
use wxread_media::Media;
use wxread_messages::{Chat, Row};
use wxread_moments::MediaId;
use wxread_schema::{CONTACT_DB, FAVORITE_DB, HEAD_IMAGE_DB, SNS_DB};

use crate::account::Account;
use crate::media::{self, MEDIA_DB_PATHS};
use crate::messages::MESSAGE_DBS;
use crate::world::World;

/// What `file get` was asked: the message, whether its thumbnail, where
/// to write it, and the account.
pub struct Asked {
    pub id: String,
    pub thumbnail: bool,
    pub out: PathBuf,
    pub account: Option<String>,
}

/// A message found: its row, its chat, and the directory its chat's
/// images are kept under.
struct Found {
    row: Row,
    chat: String,
    chat_dir: String,
}

/// What `file get` answers of `world`.
pub fn get(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    if wxread_favorites::is_file_id(&asked.id) {
        return favorite(world, asked);
    }
    if let Some(id) = MediaId::parse(&asked.id) {
        return moment(world, asked, id);
    }
    let server_id: i64 = asked
        .id
        .parse()
        .ok()
        .filter(|n| *n != 0)
        .ok_or_else(|| anyhow!(wxread_media::not_an_id(&asked.id)))?;
    let account = Account::open(world, asked.account.as_deref(), &[MESSAGE_DBS, MEDIA_DB_PATHS])?;
    let found = find(&account, server_id)?
        .ok_or_else(|| anyhow!(wxread_media::no_such_message(&asked.id, account.id())))?;
    let chat = Chat { id: found.chat.clone(), account: account.id().to_string() };
    let media = wxread_messages::media_of(&chat, &found.row)
        .map_err(|kind| anyhow!(wxread_media::not_media(&asked.id, kind.name())))?;
    if asked.thumbnail && !media.has_thumbnail() {
        return Err(anyhow!(wxread_media::no_thumbnail(media.kind(), &asked.id)));
    }
    if let Media::Sticker { md5, url } = &media {
        return sticker(&asked.id, md5.as_deref(), url.as_deref(), &asked.out);
    }
    let at = wxread_answer::rfc3339(found.row.create_time).unwrap_or_default();
    let mut key = None;
    let bytes = |thumbnail: bool, key: &mut _| -> anyhow::Result<Option<Vec<u8>>> {
        match &media {
            Media::Voice => Ok(media::voices(&account, None, Some(server_id))?
                .into_iter()
                .next()
                .map(|(_, silk)| silk)),
            _ => match media::find(&account.dir(), &media, &found.chat_dir, thumbnail) {
                Some(path) if media.sealed() => media::decrypted(&account, &path, key).map(Some),
                Some(path) => media::read(&path).map(Some),
                None => Ok(None),
            },
        }
    };
    if let Some(bytes) = bytes(asked.thumbnail, &mut key)? {
        return media::write(&asked.out, &bytes);
    }
    if asked.thumbnail {
        let fix = wxread_media::thumbnail_fix(media.kind(), &asked.id, &found.chat, &at);
        return Ok(missing(None, &fix));
    }
    let thumbnail = if media.has_thumbnail() { bytes(true, &mut key)? } else { None };
    let written = thumbnail.map(|t| media::write(&asked.out, &t)).transpose()?;
    let fix = wxread_media::fetch_fix(media.kind(), &asked.id, &found.chat, &at, written.is_some());
    Ok(missing(written, &fix))
}

/// What `file get` answers of `world` for `id`, one of a Moment's media.
fn moment(world: &World, asked: &Asked, id: MediaId) -> anyhow::Result<Value> {
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB, SNS_DB])?;
    let no_moment = || anyhow!(wxread_moments::no_such_moment(&asked.id, account.id()));
    if !account.databases()?.iter().any(|d| d == SNS_DB) {
        return Err(no_moment());
    }
    let row = crate::moments::one(&account.database(SNS_DB)?, id.tid())?.ok_or_else(no_moment)?;
    let names = crate::contacts::display_names(&account)?;
    let moment = wxread_moments::moment(&row, &names);
    let medium = moment
        .media
        .get(id.place)
        .ok_or_else(|| anyhow!(wxread_moments::no_such_medium(&asked.id, moment.media.len())))?;
    let media = medium
        .file
        .as_ref()
        .ok_or_else(|| anyhow!(wxread_moments::not_a_file(&asked.id, &moment.id)))?;
    let author = Some(moment.author.display_name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(&moment.author.id);
    let at = moment.at.as_deref().unwrap_or("an unknown time");
    let source = format!("{author}'s Moment of {at} (in WeChat's Moments)");
    let mut key = None;
    let dir = account.dir();
    // Both a Moment's kinds, image and video, have a thumbnail.
    let mut bytes = |thumbnail: bool| -> anyhow::Result<Option<Vec<u8>>> {
        match media::find(&dir, media, "", thumbnail) {
            Some(path) if media.sealed() => {
                media::unsealed(&account, &path, &mut key, &source).map(Some)
            }
            Some(path) => media::read(&path).map(Some),
            None => Ok(None),
        }
    };
    if let Some(bytes) = bytes(asked.thumbnail)? {
        return media::write(&asked.out, &bytes);
    }
    let thumbnail = if asked.thumbnail { None } else { bytes(true)? };
    let written = thumbnail.map(|t| media::write(&asked.out, &t)).transpose()?;
    let fix = wxread_media::moment_fix(media.kind(), &asked.id, author, at, written.is_some());
    Ok(missing(written, &fix))
}

/// What `file get fav:<favorite>:<item>` answers of `world`: the
/// favorite's item's file, or its thumbnail, unsealed when WeChat sealed
/// it.
fn favorite(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let id = FileId::parse(&asked.id)
        .ok_or_else(|| anyhow!(wxread_favorites::not_a_file_id(&asked.id)))?;
    let account = Account::open(world, asked.account.as_deref(), &[FAVORITE_DB])?;
    let no_such = || anyhow!(wxread_favorites::no_such_file(&id, account.id()));
    let db = crate::favorites::database(&account)?.ok_or_else(no_such)?;
    let row = crate::favorites::row(&db, id.favorite)?.ok_or_else(no_such)?;
    let stored = wxread_favorites::stored(&row, &id.item)
        .map_err(|kind| anyhow!(wxread_favorites::not_media(&id, kind)))?
        .ok_or_else(no_such)?;
    if asked.thumbnail && stored.thumbnail.is_none() {
        return Err(anyhow!(wxread_favorites::no_thumbnail(&id, stored.kind)));
    }
    let source = format!("favorite {} (in Favorites, 收藏)", id.favorite);
    let dir = account.dir();
    let mut key = None;
    let mut bytes = |path: Option<&String>| -> anyhow::Result<Option<Vec<u8>>> {
        match path.map(|p| dir.join(p)).filter(|p| p.is_file()) {
            Some(p) => media::unsealed(&account, &p, &mut key, &source).map(Some),
            None => Ok(None),
        }
    };
    let wanted = if asked.thumbnail { stored.thumbnail.as_ref() } else { Some(&stored.file) };
    if let Some(bytes) = bytes(wanted)? {
        return media::write(&asked.out, &bytes);
    }
    if asked.thumbnail {
        return Ok(missing(None, &wxread_favorites::fetch_fix(&id, stored.kind, true, false)));
    }
    let thumbnail = bytes(stored.thumbnail.as_ref())?;
    let written = thumbnail.map(|t| media::write(&asked.out, &t)).transpose()?;
    let fix = wxread_favorites::fetch_fix(&id, stored.kind, false, written.is_some());
    Ok(missing(written, &fix))
}

/// The answer of a file WeChat does not hold: the thumbnail written in its
/// place, if any, and what makes WeChat fetch it.
fn missing(thumbnail: Option<Value>, fix: &str) -> Value {
    json!({ "missing": true, "thumbnail": thumbnail, "fix": fix })
}

/// A sticker fetched from WeChat's CDN, checked against its MD5, written
/// at `out`; missing when there is nowhere to fetch it from.
fn sticker(id: &str, md5: Option<&str>, url: Option<&str>, out: &Path) -> anyhow::Result<Value> {
    let missing = |served| missing(None, &wxread_media::sticker_fix(id, served));
    let Some(url) = url else {
        return Ok(missing(false));
    };
    let Some(bytes) = media::fetch(url)? else {
        return Ok(missing(true));
    };
    if let Some(md5) = md5
        && !wxread_media::is_sticker(&bytes, md5)
    {
        return Err(anyhow!(
            "what WeChat's CDN served at {url} is not sticker {id}: its MD5 is not {md5}, the one its message names. Run this again later; nothing was written to {}",
            out.display()
        ));
    }
    media::write(out, &bytes)
}

/// The message of server id `server_id` in the account's message
/// databases, with its chat.
fn find(account: &Account, server_id: i64) -> anyhow::Result<Option<Found>> {
    for db in wxread_messages::message_dbs(&account.databases()?) {
        let path = wxread_messages::db_path(&db);
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        if let Some(found) = in_db(&conn, &db, server_id).map_err(sql)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// The message of server id `server_id` in message database `db`, of
/// whichever chat's table holds it.
fn in_db(conn: &Connection, db: &str, server_id: i64) -> rusqlite::Result<Option<Found>> {
    let mut tables = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'Msg\\_%' ESCAPE '\\' ORDER BY name",
    )?;
    let tables: Vec<String> = tables.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for table in tables {
        let Some(chat_dir) = table
            .strip_prefix("Msg_")
            .filter(|h| h.len() == 32 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            continue;
        };
        let row = conn
            .query_row(
                &format!(
                    "SELECT m.local_id, IFNULL(m.local_type, 0), IFNULL(m.sort_seq, 0), IFNULL(m.create_time, 0), n.user_name, m.message_content, m.WCDB_CT_message_content, m.packed_info_data
                     FROM {table} m LEFT JOIN Name2Id n ON n.rowid = m.real_sender_id WHERE m.server_id = ?1 LIMIT 1"
                ),
                [server_id],
                |r| {
                    Ok(Row {
                        db: db.to_string(),
                        local_id: r.get(0)?,
                        server_id,
                        local_type: r.get(1)?,
                        sort_seq: r.get(2)?,
                        create_time: r.get(3)?,
                        sender: r.get::<_, Option<String>>(4)?.filter(|s| !s.is_empty()),
                        content: crate::messages::bytes(r.get(5)?),
                        compressed: r.get(6)?,
                        packed: crate::messages::bytes(r.get(7)?),
                    })
                },
            )
            .optional()?;
        if let Some(row) = row {
            let chat = chat_of(conn, &table)?.unwrap_or_else(|| chat_dir.to_string());
            return Ok(Some(Found { row, chat, chat_dir: chat_dir.to_string() }));
        }
    }
    Ok(None)
}

/// The chat whose messages are in `table`: the id in `Name2Id` it is
/// named for.
fn chat_of(conn: &Connection, table: &str) -> rusqlite::Result<Option<String>> {
    let mut query = conn.prepare("SELECT user_name FROM Name2Id")?;
    let ids = query.query_map([], |r| r.get::<_, Option<String>>(0))?;
    for id in ids {
        if let Some(id) = id?
            && wxread_schema::messages_table(&id) == table
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// What `avatar <id>` answers of `world`.
pub fn avatar(world: &World, account: Option<&str>, id: &str, out: &Path) -> anyhow::Result<Value> {
    let account = Account::open(world, account, &[CONTACT_DB, HEAD_IMAGE_DB])?;
    fn sql(path: &'static str) -> impl Fn(rusqlite::Error) -> anyhow::Error {
        move |e| anyhow!(wxread_account::not_wechats(path, &e.to_string()))
    }
    let image: Option<Vec<u8>> = account
        .database(HEAD_IMAGE_DB)?
        .query_row("SELECT image_buffer FROM head_image WHERE username = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(sql(HEAD_IMAGE_DB))?
        .flatten()
        .filter(|b: &Vec<u8>| !b.is_empty());
    if let Some(image) = image {
        return media::write(out, &image);
    }
    let known: Option<i64> = account
        .database(CONTACT_DB)?
        .query_row("SELECT 1 FROM contact WHERE username = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(sql(CONTACT_DB))?;
    if known.is_none() {
        return Err(anyhow!(wxread_contacts::unknown(id)));
    }
    Ok(missing(None, &wxread_media::avatar_fix(id)))
}
