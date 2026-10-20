//! What `wxread favorites` answers of what the account saved (收藏): each
//! favorite as its kind, with its kind's fields, where it was saved from
//! and its tags, newest first, a page at a time; and where WeChat keeps a
//! favorite's files, which `wxread file get fav:<id>:<item>` writes. The
//! effect crate reads the rows out of `favorite/favorite.db` and tells
//! whether a file is there; nothing here reads or writes.
//!
//! WeChat for Mac 4.x keeps each favorite as one row of `fav_db_item`, its
//! `type` what it is and its `content` a `<favitem>`: its text, its link,
//! its source, and in a `<datalist>` each `<dataitem>` it is made of (an
//! image's file, a chat history's messages, a note's paragraphs). A
//! favorite's file lies under the account's directory at
//! `business/favorite/<data|thumb>/<h[..2]>/<h>`, `h` the MD5 of the
//! item's `dataid`, its `fullmd5` (its thumbnail's `thumbfullmd5`) and
//! `_data` (`_thumb`), sealed as an image is.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use roxmltree::Node;
use serde_json::{Map, Value, json};
use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_xml::{attr, child, named, text, xml};

/// Where a favorite's files lie, under the account's directory.
pub const SHELF: &str = "business/favorite";
/// What a favorite's file's id starts with, telling it from a message's.
pub const FILE_PREFIX: &str = "fav:";

/// What a favorite, or one item of a chat history or a note, is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Link,
    Image,
    Video,
    Voice,
    File,
    Location,
    Music,
    ChatHistory,
    Note,
    MiniProgram,
    Channels,
    Unknown,
}

impl Kind {
    /// Every kind, as the answer names them.
    pub const ALL: [Kind; 13] = [
        Kind::Text,
        Kind::Link,
        Kind::Image,
        Kind::Video,
        Kind::Voice,
        Kind::File,
        Kind::Location,
        Kind::Music,
        Kind::ChatHistory,
        Kind::Note,
        Kind::MiniProgram,
        Kind::Channels,
        Kind::Unknown,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Voice => "voice",
            Kind::File => "file",
            Kind::Location => "location",
            Kind::Music => "music",
            Kind::ChatHistory => "chat_history",
            Kind::Note => "note",
            Kind::MiniProgram => "mini_program",
            Kind::Channels => "channels",
            Kind::Unknown => "unknown",
        }
    }

    /// The kind of a favorite of `fav_db_item.type` `t`.
    pub fn of_type(t: i64) -> Kind {
        TYPES.into_iter().find(|(n, _)| *n == t).map_or(Kind::Unknown, |(_, k)| k)
    }

    /// The kind of an item of `datatype` `d`.
    fn of_datatype(d: i64) -> Kind {
        DATATYPES.into_iter().find(|(n, _)| *n == d).map_or(Kind::Unknown, |(_, k)| k)
    }

    /// Whether it carries a file `wxread file get` writes.
    pub fn is_media(self) -> bool {
        matches!(self, Kind::Image | Kind::Video | Kind::Voice | Kind::File)
    }

    /// Whether WeChat keeps a thumbnail of its file: an image's and a
    /// video's.
    pub fn has_thumbnail(self) -> bool {
        matches!(self, Kind::Image | Kind::Video)
    }
}

/// A favorite's `type`, and what it is: a short video (16) is a video, a
/// song shared from an app (21) music.
const TYPES: [(i64, Kind); 14] = [
    (1, Kind::Text),
    (2, Kind::Image),
    (3, Kind::Voice),
    (4, Kind::Video),
    (5, Kind::Link),
    (6, Kind::Location),
    (7, Kind::Music),
    (8, Kind::File),
    (14, Kind::ChatHistory),
    (16, Kind::Video),
    (18, Kind::Note),
    (19, Kind::MiniProgram),
    (20, Kind::Channels),
    (21, Kind::Music),
];

/// An item's `datatype`, and what it is.
const DATATYPES: [(i64, Kind); 12] = [
    (1, Kind::Text),
    (2, Kind::Image),
    (3, Kind::Voice),
    (4, Kind::Video),
    (5, Kind::Link),
    (6, Kind::Location),
    (7, Kind::Music),
    (8, Kind::File),
    (15, Kind::Video),
    (17, Kind::ChatHistory),
    (19, Kind::MiniProgram),
    (29, Kind::Music),
];

/// One favorite, as `fav_db_item` holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub local_id: i64,
    /// `type`: what it is.
    pub fav_type: i64,
    /// When it was saved, or last edited, in seconds since the epoch.
    pub update_time: i64,
    /// Its `<favitem>`, as stored.
    pub content: String,
    /// `fromusr`: the chat it was saved from, a group's id for a group.
    pub from: Option<String>,
    /// `realchatname`: who said it, in a group.
    pub sender: Option<String>,
}

/// Where a favorite is in the list: newest saved first, then the one
/// saved last among those of one second.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    saved: Reverse<i64>,
    local_id: Reverse<i64>,
}

impl Place {
    fn of(row: &Row) -> Place {
        Place { saved: Reverse(row.update_time), local_id: Reverse(row.local_id) }
    }
}

/// Where a page ended: the last favorite it listed. Its text is the `next`
/// an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(Place);

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        let place = text.split_once(':').and_then(|(saved, id)| {
            Some(Place { saved: Reverse(saved.parse().ok()?), local_id: Reverse(id.parse().ok()?) })
        });
        place.map(Cursor).ok_or_else(|| {
            format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread favorites answered: pass the `next` of the answer before, or leave {CURSOR_FLAG} out to start at the newest favorite"
            )
        })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        format!("{}:{}", self.0.saved.0, self.0.local_id.0)
    }
}

/// What a favorite is read with: the account's own id, which the source
/// of what it said itself names, each contact's name as WeChat shows it,
/// by id, and the files WeChat holds of those [`paths`] names, by their
/// path under the account's directory.
pub struct Seen<'a> {
    pub own: &'a str,
    pub names: &'a BTreeMap<String, String>,
    pub held: &'a BTreeSet<String>,
}

/// One page of the favorites: their rows, newest first, each answered as
/// [`favorite`] answers it once the files of its [`paths`] are looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<Row>,
    /// Where the next page starts, when there are favorites past this one.
    pub next: Option<Cursor>,
}

/// The page of `rows` that starts after `after` (at the newest, for none)
/// and holds at most `limit`.
pub fn page(rows: Vec<Row>, after: Option<&Cursor>, limit: Limit) -> Page {
    let mut placed: Vec<(Place, Row)> = Vec::new();
    for row in rows {
        let place = Place::of(&row);
        if after.is_none_or(|c| place > c.0) {
            placed.push((place, row));
        }
    }
    placed.sort_by(|a, b| a.0.cmp(&b.0));
    let more = placed.len() > limit.get();
    placed.truncate(limit.get());
    let next = if more { placed.last().map(|(place, _)| Cursor(place.clone())) } else { None };
    Page { rows: placed.into_iter().map(|(_, row)| row).collect(), next }
}

/// A favorite as `wxread favorites` answers it: its id, when it was saved,
/// its kind and an object of its kind's fields named by its kind, where it
/// was saved from, its tags, and its type and content as WeChat keeps
/// them. A content that does not read leaves every field it would give
/// null: the favorite is still its kind, by its type.
pub fn favorite(row: &Row, seen: &Seen) -> Value {
    let kind = Kind::of_type(row.fav_type);
    let read = xml(&row.content, |root| {
        let item = child(root, "datalist").and_then(|l| named(l, "dataitem").into_iter().next());
        let at = At { files: &Saved { favorite: row.local_id, seen } };
        Some((at.fields(kind, root, item, true, 0), source(root, row, seen), tags(root)))
    });
    let (fields, source, tags) =
        read.unwrap_or_else(|| (Map::new(), source_of(row, None, seen), Vec::new()));
    let mut out = Map::new();
    out.insert("id".into(), json!(row.local_id.to_string()));
    out.insert("saved_at".into(), json!(rfc3339(row.update_time)));
    out.insert("kind".into(), json!(kind.name()));
    out.insert(kind.name().into(), Value::Object(fields));
    out.insert("source".into(), source);
    out.insert("tags".into(), json!(tags));
    out.insert("type".into(), json!(row.fav_type));
    out.insert("content".into(), json!(row.content));
    Value::Object(out)
}

/// A favorite's file, found by its id: its kind, and the paths under the
/// account's directory of its file and of its thumbnail, when its kind
/// has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub kind: Kind,
    pub file: String,
    pub thumbnail: Option<String>,
}

/// The id of a favorite's file: the favorite's, and its item's `dataid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileId {
    pub favorite: i64,
    pub item: String,
}

impl FileId {
    /// The id `text` names, when it is a favorite's file's
    /// (`fav:<favorite>:<item>`); None for any other.
    pub fn parse(text: &str) -> Option<FileId> {
        let (favorite, item) = text.strip_prefix(FILE_PREFIX)?.split_once(':')?;
        let favorite =
            favorite.parse().ok().filter(|_| favorite.bytes().all(|b| b.is_ascii_digit()))?;
        if item.is_empty()
            || !item.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return None;
        }
        Some(FileId { favorite, item: item.to_string() })
    }

    pub fn text(&self) -> String {
        format!("{FILE_PREFIX}{}:{}", self.favorite, self.item)
    }
}

/// Whether `text` is meant as a favorite's file's id, well formed or not.
pub fn is_file_id(text: &str) -> bool {
    text.starts_with(FILE_PREFIX)
}

/// Where the item `item` of favorite `row` keeps its file: Err with its
/// kind when it carries none, and Ok(None) when the favorite has no such
/// item.
pub fn stored(row: &Row, item: &str) -> Result<Option<Stored>, Kind> {
    let found = xml(&row.content, |root| {
        let mut items = Vec::new();
        every_item(root, &mut items);
        let node = items.into_iter().find(|n| attr(*n, "dataid").as_deref() == Some(item))?;
        let kind = datatype(node).map_or(Kind::of_type(row.fav_type), Kind::of_datatype);
        Some((kind, text(node, "fullmd5"), text(node, "thumbfullmd5")))
    });
    let Some((kind, md5, thumb_md5)) = found else {
        return Ok(None);
    };
    if !kind.is_media() {
        return Err(kind);
    }
    Ok(Some(Stored {
        kind,
        file: path(item, md5.as_deref(), "data"),
        thumbnail: kind.has_thumbnail().then(|| path(item, thumb_md5.as_deref(), "thumb")),
    }))
}

/// The path under the account's directory of every file and thumbnail
/// `row` names, of its items that carry one: those whose being there its
/// answer says.
pub fn paths(row: &Row) -> Vec<String> {
    let found = xml(&row.content, |root| {
        let mut items = Vec::new();
        every_item(root, &mut items);
        let mut out = Vec::new();
        for item in items {
            if let Some(dataid) = attr(item, "dataid") {
                out.push(path(&dataid, text(item, "fullmd5").as_deref(), "data"));
                out.push(path(&dataid, text(item, "thumbfullmd5").as_deref(), "thumb"));
            }
        }
        Some(out)
    });
    found.unwrap_or_default()
}

/// Every item under `node`, those of a chat history saved in a chat
/// history among them, in order, into `out`.
fn every_item<'a, 'i>(node: Node<'a, 'i>, out: &mut Vec<Node<'a, 'i>>) {
    for n in node.children() {
        if n.has_tag_name("dataitem") {
            out.push(n);
        }
        every_item(n, out);
    }
}

/// The path under the account's directory of the file `which` (`data` or
/// `thumb`) of the item `item`, `md5` its `fullmd5` or `thumbfullmd5`.
pub fn path(item: &str, md5: Option<&str>, which: &str) -> String {
    let h = format!("{:x}", md5::compute(format!("{item}{}_{which}", md5.unwrap_or(""))));
    let shelf = h.get(..2).unwrap_or("");
    format!("{SHELF}/{which}/{shelf}/{h}")
}

/// What an item says it is: its `datatype`.
fn datatype(item: Node) -> Option<i64> {
    attr(item, "datatype").or_else(|| text(item, "datatype"))?.parse().ok()
}

/// What finds the file an item carries: its id, which `wxread file get`
/// takes, and whether WeChat holds it and its thumbnail. A favorite's items
/// are named by the favorite (`fav:<favorite>:<item>`); a note's sent into
/// a chat, by its message.
pub trait Files {
    /// The file of `item`, of the media kind `kind`, the `place`-th item
    /// of its list.
    fn of(&self, kind: Kind, item: Option<Node>, place: usize) -> File;
}

/// The file an item carries, as its answer says it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    /// What `wxread file get` takes: None when nothing names it.
    pub id: Option<String>,
    /// Whether WeChat holds it: None when that cannot be told.
    pub present: Option<bool>,
    /// Whether WeChat holds its thumbnail: None for a kind that has none.
    pub thumbnail: Option<bool>,
}

/// The items of the `<datalist>` under `list`, in order, each as its kind
/// with who said it and when, as a chat history or a note keeps them, the
/// files they carry found by `files`.
pub fn items(list: Node, files: &impl Files) -> Vec<Value> {
    At { files }.items(list)
}

/// A favorite's files: named by its id and each item's `dataid`, and held
/// when WeChat holds what [`paths`] names.
struct Saved<'s, 'n> {
    favorite: i64,
    seen: &'s Seen<'n>,
}

impl Files for Saved<'_, '_> {
    fn of(&self, kind: Kind, item: Option<Node>, _place: usize) -> File {
        let dataid = item.and_then(|i| attr(i, "dataid"));
        let id = dataid.as_ref().map(|d| FileId { favorite: self.favorite, item: d.clone() });
        let held = |which: &str, md5: &str| {
            let md5 = item.and_then(|i| text(i, md5));
            dataid
                .as_deref()
                .is_some_and(|d| self.seen.held.contains(&path(d, md5.as_deref(), which)))
        };
        File {
            id: id.map(|i| i.text()),
            present: Some(held("data", "fullmd5")),
            thumbnail: kind.has_thumbnail().then(|| held("thumb", "thumbfullmd5")),
        }
    }
}

/// Reading the fields of a favorite or of an item, the files its items
/// carry found by `files`.
struct At<'f, F> {
    files: &'f F,
}

impl<F: Files> At<'_, F> {
    /// The fields of a `kind`, read from `own` (the `<favitem>`, with
    /// `top`; else the item itself) and `item` (its first item, or the
    /// item itself, the `place`-th of its list).
    fn fields(
        &self,
        kind: Kind,
        own: Node,
        item: Option<Node>,
        top: bool,
        place: usize,
    ) -> Map<String, Value> {
        let of = |name: &str| text(own, name);
        let it = |name: &str| item.and_then(|i| text(i, name));
        let either = |own_name: &str, item_name: &str| {
            if top { of(own_name).or_else(|| it(item_name)) } else { it(item_name) }
        };
        let fields = match kind {
            Kind::Text => json!({ "text": either("desc", "datadesc") }),
            Kind::Link => {
                let web = child(own, "weburlitem");
                let on_web = |name: &str| web.and_then(|w| text(w, name));
                json!({
                    "title": on_web("pagetitle").or_else(|| either("title", "datatitle")),
                    "summary": on_web("pagedesc").or_else(|| either("desc", "datadesc")),
                    "url": text(own, "source/link").or_else(|| it("link")).or_else(|| it("stream_weburl")),
                    "cover": on_web("pagethumb_url"),
                    "source": on_web("appmsgshareitem/srcdisplayname"),
                })
            }
            Kind::Location => {
                let l = child(own, "locitem").or_else(|| item.and_then(|i| child(i, "locitem")));
                let at = |name: &str| l.and_then(|l| text(l, name));
                let degrees = |name: &str| at(name)?.parse::<f64>().ok().filter(|d| d.is_finite());
                json!({
                    "latitude": degrees("lat"),
                    "longitude": degrees("lng"),
                    "name": at("poiname"),
                    "address": at("label"),
                })
            }
            Kind::Music => json!({
                "title": it("datatitle").or_else(|| either("title", "datatitle")),
                "summary": it("datadesc").or_else(|| either("desc", "datadesc")),
                "url": it("stream_weburl"),
                "audio_url": it("stream_dataurl"),
            }),
            Kind::MiniProgram => {
                let a = child(own, "appbranditem")
                    .or_else(|| item.and_then(|i| child(i, "appbranditem")));
                let app = |name: &str| a.and_then(|a| text(a, name));
                json!({
                    "title": either("title", "datatitle"),
                    "app": app("sourcedisplayname"),
                    "app_id": app("appid"),
                    "path": app("pagepath"),
                })
            }
            Kind::Channels => {
                let f =
                    child(own, "finderFeed").or_else(|| item.and_then(|i| child(i, "finderFeed")));
                let feed = |name: &str| f.and_then(|f| text(f, name));
                json!({
                    "title": either("title", "datatitle"),
                    "author": feed("nickname"),
                    "summary": feed("desc"),
                })
            }
            Kind::ChatHistory => {
                let list = if top {
                    Some(own)
                } else {
                    item.and_then(|i| child(i, "recordxml/recordinfo"))
                };
                json!({
                    "title": either("title", "datatitle"),
                    "summary": either("desc", "datadesc"),
                    "items": list.map(|l| self.items(l)).unwrap_or_default(),
                })
            }
            Kind::Note => json!({ "items": self.items(own) }),
            Kind::Image | Kind::Video | Kind::Voice | Kind::File => {
                self.media(kind, own, item, top, place)
            }
            Kind::Unknown => json!({}),
        };
        match fields {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    /// A media kind's fields: its file's id, size and whether WeChat holds
    /// it and its thumbnail, beside its kind's own.
    fn media(&self, kind: Kind, own: Node, item: Option<Node>, top: bool, place: usize) -> Value {
        let it = |name: &str| item.and_then(|i| text(i, name));
        let file = self.files.of(kind, item, place);
        let mut fields = Map::new();
        fields.insert("file_id".into(), json!(file.id));
        fields.insert("size".into(), json!(it("fullsize").and_then(|s| s.parse::<u64>().ok())));
        match kind {
            Kind::Video => {
                fields.insert(
                    "seconds".into(),
                    json!(it("duration").and_then(|s| s.parse::<u64>().ok())),
                );
            }
            Kind::Voice => {
                fields.insert(
                    "length_ms".into(),
                    json!(it("duration").and_then(|s| s.parse::<u64>().ok())),
                );
            }
            Kind::File => {
                let name = it("datatitle").or_else(|| if top { text(own, "title") } else { None });
                let extension = it("datafmt").map(|f| f.trim_start_matches('.').to_string());
                fields.insert("name".into(), json!(name));
                fields.insert("extension".into(), json!(extension.filter(|e| !e.is_empty())));
            }
            _ => {}
        }
        fields.insert("file_present".into(), json!(file.present));
        fields.insert("thumbnail_present".into(), json!(file.thumbnail));
        Value::Object(fields)
    }

    /// The items of the `<datalist>` under `list`, in order, each as its
    /// kind with who said it and when, as a chat history keeps them.
    fn items(&self, list: Node) -> Vec<Value> {
        let Some(list) = child(list, "datalist") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (place, item) in named(list, "dataitem").into_iter().enumerate() {
            let t = datatype(item);
            let kind = t.map_or(Kind::Unknown, Kind::of_datatype);
            let source = child(item, "dataitemsource");
            let from = |name: &str| source.and_then(|s| text(s, name));
            let mut one = Map::new();
            one.insert("kind".into(), json!(kind.name()));
            one.insert(
                kind.name().into(),
                Value::Object(self.fields(kind, item, Some(item), false, place)),
            );
            one.insert("sender".into(), json!(from("realchatname").or_else(|| from("fromusr"))));
            one.insert("sender_name".into(), json!(text(item, "datasrcname")));
            let at = text(item, "srcMsgCreateTime").and_then(|s| s.parse::<i64>().ok());
            one.insert("at".into(), json!(at.and_then(rfc3339)));
            one.insert("type".into(), json!(t));
            out.push(Value::Object(one));
        }
        out
    }
}

/// Where favorite `row` was saved from, read from its `<source>`.
fn source(root: Node, row: &Row, seen: &Seen) -> Value {
    source_of(row, Some(child(root, "source")), seen)
}

/// Who said what was saved and in which chat, each named as WeChat shows
/// them, when it was said, its message's id and the link it was saved
/// from: in a group, `fromusr` is the group and `realchatname` who said
/// it; in a direct chat `fromusr` said it, to `tousr`. `s` is None when
/// the content does not read, and the row's own columns of the same say
/// it instead.
fn source_of(row: &Row, s: Option<Option<Node>>, seen: &Seen) -> Value {
    let at = |name: &str| s.flatten().and_then(|s| text(s, name));
    let column = |c: &Option<String>| if s.is_none() { c.clone() } else { None };
    let from = at("fromusr").or_else(|| column(&row.from)).filter(|f| !f.is_empty());
    let sender = at("realchatname").or_else(|| column(&row.sender)).filter(|f| !f.is_empty());
    let to = at("tousr");
    let said_by = sender.or_else(|| from.clone());
    let own = from.as_deref().is_some_and(|f| f == seen.own);
    let group = from.as_deref().is_some_and(|f| f.ends_with("@chatroom"));
    let chat = if own && !group && to.is_some() { to } else { from.clone() };
    let name = |id: &Option<String>| id.as_ref().and_then(|i| seen.names.get(i)).cloned();
    let said_at = at("createtime").and_then(|t| t.parse::<i64>().ok()).and_then(rfc3339);
    json!({
        "from": said_by,
        "from_name": name(&said_by),
        "chat": chat,
        "chat_name": name(&chat),
        "at": said_at,
        "message_id": at("msgid").filter(|m| m != "0"),
        "link": at("link"),
    })
}

/// The tags the account gave a favorite, in its order.
fn tags(root: Node) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(list) = child(root, "taglist") {
        for tag in named(list, "tag") {
            if let Some(t) = tag.text().map(str::trim).filter(|t| !t.is_empty()) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// What fixes keys that hold none of the favorites database `db` of
/// `account`.
pub fn no_key(db: &str, account: &str) -> String {
    format!(
        "the keys supplied have no key of {db}, which account {account} keeps its favorites in: WeChat may hold that key only once its Favorites (收藏) has been opened. Open Favorites in WeChat, then get the keys again with `sudo wxread keys` (`wxread keys --help` says how) and supply them to every command: --keys <file>, or their contents as WXREAD_KEYS"
    )
}

/// What fixes a favorite's file id that is not one.
pub fn not_a_file_id(id: &str) -> String {
    format!(
        "{id} is not a favorite's file id: pass one as `wxread favorites` answers it, its file_id (fav:<favorite>:<item>)"
    )
}

/// What fixes naming a favorite, or an item of one, the account does not
/// hold.
pub fn no_such_file(id: &FileId, account: &str) -> String {
    format!(
        "account {account} has no favorite {} holding item {}: pass a file_id as `wxread favorites` answers it",
        id.favorite, id.item
    )
}

/// What fixes naming an item that carries no file.
pub fn not_media(id: &FileId, kind: Kind) -> String {
    format!(
        "{} is a {} of favorite {}, which carries no file: pass the file_id of an image, video, voice or file, as `wxread favorites` answers it",
        id.text(),
        kind.name(),
        id.favorite
    )
}

/// What fixes asking for the thumbnail of a kind WeChat keeps none of.
pub fn no_thumbnail(id: &FileId, kind: Kind) -> String {
    format!(
        "{} is a {}, of which WeChat keeps no thumbnail (only images and videos have one): leave --thumbnail out",
        id.text(),
        kind.name()
    )
}

/// What makes WeChat fetch a favorite's file, or with `thumbnail_asked`
/// its thumbnail, it does not hold; `thumbnail` whether the answer holds
/// the thumbnail in the file's place.
pub fn fetch_fix(id: &FileId, kind: Kind, thumbnail_asked: bool, thumbnail: bool) -> String {
    let what = if thumbnail_asked { "thumbnail" } else { "file" };
    let instead = if thumbnail {
        "; until then --out holds its thumbnail, which WeChat does hold"
    } else {
        ""
    };
    format!(
        "WeChat for Mac on this Mac does not hold this {}'s {what} (it never downloaded it, or cleared it): open favorite {} in WeChat's Favorites (收藏) and let it download, then run this again{instead}",
        kind.name(),
        id.favorite
    )
}

#[cfg(test)]
mod tests;
