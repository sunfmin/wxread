//! `wxread voice`: finds one voice message in its chat's message
//! databases, its sound in the account's media databases, decodes the
//! SILK in-process (docs/adr/0005-voice-is-decoded-in-process.md) and
//! writes the file `--out` names: the one file wxread writes.

use std::path::PathBuf;

use anyhow::anyhow;
use serde_json::{Value, json};
use wxread_media::Held;
use wxread_messages::{Chat, Names};
use wxread_voice::{Format, SAMPLE_RATE};

use crate::account::Account;
use crate::media::{self, MEDIA_DB_PATHS};
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
    // Voice answers none of what WeChat holds: the sound it writes is read below.
    let message =
        wxread_messages::message(&said, &Names::default(), &row, &|_: &_, _: &_| Held::default());
    let kind = message["kind"].as_str().unwrap_or_default();
    if kind != "voice" {
        return Err(anyhow!(wxread_voice::not_a_voice(id, &chat, kind)));
    }
    let silk = media::voices(&account, Some(&chat), Some(id))?
        .into_iter()
        .next()
        .map(|(_, silk)| silk)
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
