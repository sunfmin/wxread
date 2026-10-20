//! An account's media where WeChat keeps it on this Mac: whether a
//! message's file and thumbnail are there, their bytes (an image decrypted
//! in memory, a sticker fetched from WeChat's CDN into memory), and the
//! one file a command writes, at `--out` and nowhere else.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::anyhow;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use wxread_media::image::{self, ImageKey, KVCOMM_DIRS, RADIUM};
use wxread_media::{Held, Media, OUT_FLAG};
use wxread_schema::MEDIA_DBS;

use crate::account::Account;

/// The media databases, as a refusal of missing keys names them.
pub const MEDIA_DB_PATHS: &str = "message/media_<n>.db";

/// Where curl is: macOS's own (docs/adr/0001-macos-only-and-apache-2.md).
const CURL: &str = "/usr/bin/curl";
/// curl's exit when the server answered an HTTP error, with `--fail`.
const HTTP_ERROR: i32 = 22;

/// The directories under `dir`, the newest name first: its months
/// (`2026-10`), or under `msg/attach` its chats.
fn months(dir: &Path) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    all.sort();
    all.reverse();
    all
}

/// The files of `media`, a message of the chat whose directory is
/// `chat_dir`, under the account directory `dir` that are its file, or
/// with `thumbnail` its thumbnail: the one to take, if one is there.
pub fn find(dir: &Path, media: &Media, chat_dir: &str, thumbnail: bool) -> Option<PathBuf> {
    let shelf = dir.join(media.shelf(chat_dir)?);
    let names = media.names(thumbnail);
    let mut found = Vec::new();
    for month in months(&shelf) {
        for name in &names {
            let path = month.join(name);
            if let Ok(meta) = std::fs::metadata(&path)
                && meta.is_file()
            {
                found.push((path, meta.len()));
            }
        }
    }
    media.pick(&found).cloned()
}

/// Whether WeChat holds `media`'s file and thumbnail, a message of the
/// chat whose directory is `chat_dir` with server id `server_id`, under
/// the account directory `dir`, `voices` the server ids of the chat's
/// voice messages the media databases hold. A sticker's file is held when
/// its message names its address on WeChat's CDN, which is not fetched to
/// tell.
pub fn held(
    dir: &Path,
    media: &Media,
    chat_dir: &str,
    server_id: i64,
    voices: &BTreeSet<i64>,
) -> Held {
    let file = match media {
        Media::Voice => voices.contains(&server_id),
        Media::Sticker { url, .. } => url.is_some(),
        _ => find(dir, media, chat_dir, false).is_some(),
    };
    let thumbnail = media.has_thumbnail().then(|| find(dir, media, chat_dir, true).is_some());
    Held { file, thumbnail }
}

/// The account's media databases, by their path under db_storage.
pub fn media_dbs(account: &Account) -> anyhow::Result<Vec<String>> {
    let mut found: Vec<(u32, String)> = account
        .databases()?
        .into_iter()
        .filter_map(|path| {
            let n = path.strip_prefix(MEDIA_DBS)?.strip_suffix(".db")?;
            let n = n.parse().ok().filter(|_| n.bytes().all(|b| b.is_ascii_digit()))?;
            Some((n, path))
        })
        .collect();
    found.sort();
    Ok(found.into_iter().map(|(_, path)| path).collect())
}

/// The voice messages of `chat` the media databases hold, or with
/// `server_id` that one: each one's server id and its SILK bytes.
pub fn voices(
    account: &Account,
    chat: Option<&str>,
    server_id: Option<i64>,
) -> anyhow::Result<Vec<(i64, Vec<u8>)>> {
    let mut all = Vec::new();
    for path in media_dbs(account)? {
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        all.extend(voice_rows(&conn, chat, server_id).map_err(sql)?);
    }
    Ok(all)
}

fn voice_rows(
    conn: &Connection,
    chat: Option<&str>,
    server_id: Option<i64>,
) -> rusqlite::Result<Vec<(i64, Vec<u8>)>> {
    let mut query = conn.prepare(
        "SELECT IFNULL(v.svr_id, 0), v.voice_data FROM VoiceInfo v LEFT JOIN Name2Id n ON n.rowid = v.chat_name_id
         WHERE (?1 IS NULL OR n.user_name = ?1) AND (?2 IS NULL OR v.svr_id = ?2) AND length(v.voice_data) > 0",
    )?;
    query.query_map(params![chat, server_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
}

/// The account's image key, found as wxkey finds it: derived from a
/// kvcomm file's code and the account's id, the one that opens the
/// account's first image sealed under one, a message's, else a favorite's
/// thumbnail; None when none does.
pub fn image_key(account: &Account) -> Option<ImageKey> {
    let attach = account.dir().join("msg/attach");
    let images = months(&attach).into_iter().flat_map(|chat| months(&chat)).map(|m| m.join("Img"));
    let thumbs = months(&account.dir().join(wxread_favorites::SHELF).join("thumb"));
    let sample = images.chain(thumbs).find_map(|dir| {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        files.into_iter().find_map(|p| std::fs::read(p).ok().filter(|d| image::needs_key(d)))
    })?;
    let documents = account.documents();
    let mut dirs: Vec<PathBuf> = KVCOMM_DIRS.iter().map(|d| documents.join(d)).collect();
    dirs.extend(months(&documents.join(RADIUM)).into_iter().map(|m| m.join("kvcomm")));
    let codes: BTreeSet<u32> = dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter_map(|e| image::kvcomm_code(e.file_name().to_str()?))
        .collect();
    let codes: Vec<u32> = codes.into_iter().collect();
    let own = wxread_messages::names::account_id(account.id(), |_| true);
    image::image_key(&codes, &[own.as_str(), account.id()], &sample)
}

/// An image `.dat` at `path`, decrypted in memory, `key` finding the
/// account's image key when it is sealed under one.
pub fn decrypted(
    account: &Account,
    path: &Path,
    key: &mut Option<Option<ImageKey>>,
) -> anyhow::Result<Vec<u8>> {
    let dat = read(path)?;
    let key = key_of(account, &dat, key)?;
    let path = path.display().to_string();
    image::decrypt(&dat, key.as_ref())
        .map_err(|r| anyhow!(wxread_media::image_refused(&path, r, "the message")))
}

/// A favorite's file at `path` in memory: unsealed when WeChat sealed it
/// (an image, a note, a PDF alike), `key` finding the account's image key
/// when it is sealed under one; else as it is. `source` names the
/// favorite, which a refusal says to open.
pub fn unsealed(
    account: &Account,
    path: &Path,
    key: &mut Option<Option<ImageKey>>,
    source: &str,
) -> anyhow::Result<Vec<u8>> {
    let dat = read(path)?;
    if !image::is_sealed(&dat) {
        return Ok(dat);
    }
    let key = key_of(account, &dat, key)?;
    let path = path.display().to_string();
    image::unseal(&dat, key.as_ref())
        .map_err(|r| anyhow!(wxread_media::image_refused(&path, r, source)))
}

/// The image key `dat` is sealed under, found once: None for one that
/// needs none.
fn key_of(
    account: &Account,
    dat: &[u8],
    key: &mut Option<Option<ImageKey>>,
) -> anyhow::Result<Option<ImageKey>> {
    if !image::needs_key(dat) {
        return Ok(None);
    }
    let found = *key.get_or_insert_with(|| image_key(account));
    let found = found.ok_or_else(|| {
        anyhow!(wxread_media::no_image_key(
            account.id(),
            &account.documents().display().to_string()
        ))
    })?;
    Ok(Some(found))
}

/// The bytes of WeChat's file at `path`.
pub fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        anyhow!(
            "{} cannot be read ({e}): run `wxread doctor`, which names the fix of macOS refusing to let it be read",
            path.display()
        )
    })
}

/// A sticker fetched from WeChat's CDN at `url` into memory: None when
/// the CDN answers that it serves none there.
pub fn fetch(url: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let out = Command::new(CURL)
        .args(["--silent", "--show-error", "--fail", "--location", "--max-time", "30", "--"])
        .arg(url)
        .output()
        .map_err(|e| anyhow!("{CURL} could not be run ({e}): wxread fetches stickers with macOS's own curl; restore it, then run this again"))?;
    match out.status.code() {
        Some(0) => Ok(Some(out.stdout)),
        Some(HTTP_ERROR) => Ok(None),
        _ => Err(anyhow!(
            "WeChat's CDN could not be reached for the sticker at {url} ({}): check this Mac's network, then run this again",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// Writes `bytes` to `out`, and answers what is there: its path, size,
/// SHA-256 and content type. A file that already holds them is left as it
/// is, so a second run changes nothing.
pub fn write(out: &Path, bytes: &[u8]) -> anyhow::Result<Value> {
    let path = std::path::absolute(out).map_err(|e| {
        anyhow!("{OUT_FLAG} {} is not a path ({e}): pass a file's path", out.display())
    })?;
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_dir() => {
            return Err(anyhow!(
                "{OUT_FLAG} {} is a directory: pass the path of the file to write, in it or anywhere",
                path.display()
            ));
        }
        Ok(_) if std::fs::read(&path).is_ok_and(|held| held == bytes) => {}
        _ => std::fs::write(&path, bytes).map_err(|e| {
            let fix = if e.kind() == ErrorKind::NotFound {
                "create the directory it is in, or pass a path in one that exists"
            } else {
                "pass a path this user may write"
            };
            anyhow!("{OUT_FLAG} {} cannot be written ({e}): {fix}", path.display())
        })?,
    }
    Ok(json!({
        "path": path.display().to_string(),
        "size": bytes.len(),
        "sha256": format!("{:x}", Sha256::digest(bytes)),
        "content_type": wxread_media::content_type(bytes),
    }))
}
