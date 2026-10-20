//! A note (笔记) sent into a chat: an app message of type 24 whose
//! `<recorditem>` keeps a `<recordinfo>` as a favorite keeps a note, so its
//! items are read as `wxread favorites` reads a note's, each as its kind.
//! The files its items carry lie under the note's own directory in the
//! chat's ([`wxread_media::Media::NoteImage`], [`Media::NoteFile`]), its
//! name in the message's `packed_info_data` (field 9, its field 1); each is
//! named `note:<message>:<item>`, the message's server id and the item's
//! `dataid`, which `wxread file get` takes. Where WeChat keeps a note's
//! video or voice message was not seen, so those name no file.

use roxmltree::Node;
use serde_json::{Map, Value, json};
use wxread_favorites::{File, Files, Kind};
use wxread_media::{Held, Media};
use wxread_xml::{attr, child, named, text, xml};

use crate::{Chat, Row, proto};

/// What a note's item's file id starts with, telling it from a message's
/// and a favorite's.
pub const FILE_PREFIX: &str = "note:";

/// The id of a file a note's item carries: the note's message, by its
/// server id, and the item's `dataid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileId {
    pub message: i64,
    pub item: String,
}

impl FileId {
    /// The id `text` names, when it is a note's item's file's
    /// (`note:<message>:<item>`); None for any other.
    pub fn parse(text: &str) -> Option<FileId> {
        let (message, item) = text.strip_prefix(FILE_PREFIX)?.split_once(':')?;
        let message = message
            .parse()
            .ok()
            .filter(|m| *m != 0 && message.bytes().all(|b| b.is_ascii_digit()))?;
        if item.is_empty() || !item.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(FileId { message, item: item.to_string() })
    }

    pub fn text(&self) -> String {
        format!("{FILE_PREFIX}{}:{}", self.message, self.item)
    }
}

/// Whether `text` is meant as a note's item's file id, well formed or not.
pub fn is_file_id(text: &str) -> bool {
    text.starts_with(FILE_PREFIX)
}

/// What finds the files of the items of a note sent as message `message`
/// (its server id, 0 for one the server never saw), its directory `rec`:
/// whether WeChat holds each one as `held` says, None when it is not
/// looked for.
pub struct InChat<'h, H> {
    pub message: i64,
    pub rec: Option<String>,
    pub held: &'h H,
}

impl<H: Fn(&Media) -> Option<Held>> Files for InChat<'_, H> {
    fn of(&self, kind: Kind, item: Option<Node>, place: usize) -> File {
        let Some(media) = item.and_then(|i| media(kind, i, place, &self.rec)) else {
            return File::default();
        };
        let id = item
            .and_then(|i| attr(i, "dataid"))
            .filter(|_| self.message != 0)
            .map(|item| FileId { message: self.message, item }.text());
        let held = (self.held)(&media);
        File { id, present: held.map(|h| h.file), thumbnail: held.and_then(|h| h.thumbnail) }
    }
}

/// The fields of the note whose `<appmsg>` is `appmsg`: its items, their
/// files found by `files`. None when its record does not read.
pub fn fields(appmsg: Node, files: &impl Files) -> Option<Map<String, Value>> {
    let items = xml(&text(appmsg, "recorditem")?, |record| {
        child(record, "datalist")?;
        Some(wxread_favorites::items(record, files))
    })?;
    match json!({ "items": items }) {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// The note `row` of `chat` sent, read with the files of its items found by
/// `held`: None when it is not a note, or its record does not read.
pub fn read(
    chat: &Chat,
    row: &Row,
    held: &impl Fn(&Media) -> Option<Held>,
) -> Option<Map<String, Value>> {
    let content = crate::read(chat, row).content;
    let files = InChat { message: row.server_id, rec: rec(&row.packed), held };
    xml(&content, |root| fields(child(root, "appmsg")?, &files))
}

/// What finds the file of the item `item` (its `dataid`) of the note `row`
/// of `chat` sent: Ok(None) when it is not a note or holds no such item,
/// Err with its kind when it carries no file wxread finds.
pub fn item(chat: &Chat, row: &Row, item: &str) -> Result<Option<Media>, Kind> {
    let content = crate::read(chat, row).content;
    let rec = rec(&row.packed);
    let found = xml(&content, |root| {
        let a = child(root, "appmsg")?;
        if text(a, "type").as_deref() != Some("24") {
            return None;
        }
        xml(&text(a, "recorditem")?, |record| {
            let items = named(child(record, "datalist")?, "dataitem");
            let (place, node) = items
                .into_iter()
                .enumerate()
                .find(|(_, n)| attr(*n, "dataid").as_deref() == Some(item))?;
            let kind = wxread_favorites::datatype(node).map_or(Kind::Unknown, Kind::of_datatype);
            Some(media(kind, node, place, &rec).ok_or(kind))
        })
    });
    found.transpose()
}

/// The note's directory, as `packed` (its message's `packed_info_data`)
/// names it.
fn rec(packed: &[u8]) -> Option<String> {
    proto::text(packed, &[9, 1])
}

/// What finds the file of `item`, of `kind`, the `place`-th of its note's
/// items, the note's directory `rec`: an image's or a file's; None for the
/// rest.
fn media(kind: Kind, item: Node, place: usize, rec: &Option<String>) -> Option<Media> {
    match kind {
        Kind::Image => Some(Media::NoteImage { rec: rec.clone(), place }),
        Kind::File => {
            Some(Media::NoteFile { rec: rec.clone(), place, name: text(item, "datatitle") })
        }
        _ => None,
    }
}

/// What fixes passing a note's own id to `wxread file get`.
pub fn not_media(id: &str) -> String {
    format!(
        "message {id} is a note, whose items carry its files: pass an item's file_id, as `wxread messages` answers it under note.items (note:<message>:<item>)"
    )
}

/// What fixes a note's item's file id that is not one.
pub fn not_a_file_id(id: &str) -> String {
    format!(
        "{id} is not a note's item's file id: pass one as `wxread messages` answers it under note.items, its file_id (note:<message>:<item>)"
    )
}

/// What fixes naming an item a note does not hold, or a note the account
/// does not hold.
pub fn no_such_item(id: &FileId, account: &str) -> String {
    format!(
        "account {account} has no note sent as message {} holding item {}: pass a file_id as `wxread messages` answers it under note.items",
        id.message, id.item
    )
}

/// What fixes naming an item that carries no file wxread finds: a
/// paragraph or a place carries none; where WeChat keeps a note's video or
/// voice message was not seen.
pub fn not_a_file(id: &FileId, kind: Kind) -> String {
    let why = if kind.is_media() {
        "where WeChat for Mac keeps a note's video or voice message is not known to wxread, so its file_id is null"
    } else {
        "which carries no file"
    };
    format!(
        "{} is a {} of the note sent as message {}, {why}: pass the file_id of one of its images or files, as `wxread messages` answers it",
        id.text(),
        kind.name(),
        id.message
    )
}
