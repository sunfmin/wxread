//! `wxread voice`: finds one voice message in its chat's message
//! databases, its sound in the account's media databases, decodes the
//! SILK in-process (docs/adr/0005-voice-is-decoded-in-process.md) and
//! writes the file `--out` names: the one file wxread writes.

use std::path::PathBuf;

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension as _};
use serde_json::{Value, json};
use wxread_messages::{Chat, Names};
use wxread_voice::{Format, MEDIA_DB_PATHS, SAMPLE_RATE};

use crate::account::Account;
use crate::messages::{self, MESSAGE_DBS};
use crate::world::World;

/// What the message id, `--account`, `--chat`, `--out` and `--raw` said.
pub struct Asked {
    pub id: String,
    pub account: Option<String>,
    pub chat: Option<String>,
    pub out: Option<PathBuf>,
    pub raw: bool,
}

/// What voice answers of `world`, once it has written the file.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let chat = messages::chat(asked.chat.as_deref())?;
    let id = wxread_voice::message_id(&asked.id).map_err(|e| anyhow!(e))?;
    let out = asked.out.clone().ok_or_else(|| anyhow!(wxread_voice::no_out()))?;
    let format = Format::of(asked.raw);
    let account = Account::open(world, asked.account.as_deref(), &[MESSAGE_DBS, MEDIA_DB_PATHS])?;
    let databases = account.databases()?;
    let table = wxread_schema::messages_table(&chat);
    let mut found = false;
    let mut row = None;
    for db in wxread_messages::message_dbs(&databases) {
        let path = wxread_messages::db_path(&db);
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        if !messages::has_table(&conn, &table).map_err(sql)? {
            continue;
        }
        found = true;
        row = messages::by_server_id(&conn, &db, &table, id).map_err(sql)?;
        if row.is_some() {
            break;
        }
    }
    if !found {
        return Err(anyhow!(wxread_messages::no_such_chat(&chat, account.id())));
    }
    let row = row.ok_or_else(|| anyhow!(wxread_voice::no_such_message(id, &chat, account.id())))?;
    let said = Chat { id: chat.clone(), account: account.id().to_string() };
    let message = wxread_messages::message(&said, &Names::default(), &row);
    let kind = message["kind"].as_str().unwrap_or_default();
    if kind != "voice" {
        return Err(anyhow!(wxread_voice::not_a_voice(id, &chat, kind)));
    }
    let silk = sound(&account, &databases, &chat, id)?
        .ok_or_else(|| anyhow!(wxread_voice::no_sound(id, &chat, account.id())))?;
    let pcm = decode(&silk);
    let bytes = match format {
        Format::Silk => silk,
        Format::Wav => {
            let pcm =
                pcm.as_ref().map_err(|why| anyhow!(wxread_voice::undecodable(id, &chat, why)))?;
            wxread_voice::wav(pcm, SAMPLE_RATE).ok_or_else(|| {
                anyhow!(wxread_voice::undecodable(id, &chat, "longer than a WAV holds"))
            })?
        }
    };
    let path = std::path::absolute(&out).unwrap_or(out);
    let shown = path.display().to_string();
    std::fs::write(&path, &bytes)
        .map_err(|e| anyhow!(wxread_voice::unwritable(&shown, &e.to_string())))?;
    Ok(json!({
        "account": account.id(),
        "chat": chat,
        "id": id.to_string(),
        "path": shown,
        "format": format.name(),
        "duration_ms": pcm.ok().and_then(|pcm| wxread_voice::duration_ms(pcm.len(), SAMPLE_RATE)),
        "size": bytes.len(),
        "length_ms": message["voice"]["length_ms"],
        "transcription": message["voice"]["transcription"],
    }))
}

/// The SILK of voice message `id` of `chat`, out of the first of the
/// account's media databases (among `databases`) that holds it.
fn sound(
    account: &Account,
    databases: &[String],
    chat: &str,
    id: i64,
) -> anyhow::Result<Option<Vec<u8>>> {
    for path in wxread_voice::media_dbs(databases) {
        let conn = account.database(&path)?;
        let sql = |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(&path, &e.to_string()));
        if !messages::has_table(&conn, "VoiceInfo").map_err(sql)? {
            continue;
        }
        if let Some(silk) = voice_data(&conn, chat, id).map_err(sql)? {
            return Ok(Some(silk));
        }
    }
    Ok(None)
}

/// The `voice_data` media database `conn` keeps of message `id` of `chat`.
fn voice_data(conn: &Connection, chat: &str, id: i64) -> rusqlite::Result<Option<Vec<u8>>> {
    conn.query_row(
        "SELECT v.voice_data FROM VoiceInfo v JOIN Name2Id n ON n.rowid = v.chat_name_id
         WHERE n.user_name = ?1 AND v.svr_id = ?2 AND v.voice_data IS NOT NULL LIMIT 1",
        rusqlite::params![chat, id],
        |r| r.get(0),
    )
    .optional()
}

/// The samples `silk` decodes to at [`SAMPLE_RATE`], or why it does not
/// decode: a decoder that panics on bytes it was not written for is a
/// failure to decode, not the end of the run.
fn decode(silk: &[u8]) -> Result<Vec<i16>, String> {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let decoded = std::panic::catch_unwind(|| silk::decode(silk, SAMPLE_RATE));
    std::panic::set_hook(quiet);
    match decoded {
        Ok(Ok(pcm)) => Ok(pcm),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the decoder failed on it".to_string()),
    }
}
