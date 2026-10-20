//! What a message is: its type and its content (the XML WeChat keeps
//! every kind but text in) go in, its kind and what that kind is made of
//! come out. Whatever is not one of the kinds, or does not read as WeChat
//! writes it, is unknown: never an error, its content and type kept beside
//! it either way. The kinds and their XML are as bizkernel's wechat-intake
//! and wechat-cli found them.
//!
//! `local_type` carries a subtype in its high 32 bits: an app message (49)
//! is `49 | <type> << 32`, the `<type>` of its `<appmsg>`.

use roxmltree::{Document, Node};
use serde_json::{Map, Value, json};

use crate::Kind;

/// WeChat's types: text, an image, a voice message, a contact's card, a
/// video, a sticker, a place, an app message, and WeChat's own notices.
pub const TEXT: i64 = 1;
pub const IMAGE: i64 = 3;
pub const VOICE: i64 = 34;
pub const CARD: i64 = 42;
pub const VIDEO: i64 = 43;
pub const STICKER: i64 = 47;
pub const LOCATION: i64 = 48;
pub const APP: i64 = 49;
pub const SYSTEM: i64 = 10000;
/// A notice WeChat keeps as XML: a recall, among others.
pub const SYSTEM_XML: i64 = 10002;

/// The subtypes of an app message read as links, and what each links to.
const LINKS: [(i64, &str); 7] = [
    (3, "music"),
    (4, "video"),
    (5, "web_page"),
    (33, "mini_program"),
    (36, "mini_program"),
    (51, "channels_feed"),
    (63, "channels_live"),
];
/// The other subtypes of an app message wxread reads: a file, a sticker, a
/// forwarded history, a quote, a nudge, a transfer and a red packet.
const APP_FILE: i64 = 6;
const APP_STICKER: i64 = 8;
const FORWARD: i64 = 19;
const QUOTE: i64 = 57;
const NUDGE: i64 = 62;
const TRANSFER: i64 = 2000;
const RED_PACKET: i64 = 2001;

/// How a recall's notice ends when it is plain text, in Chinese and in
/// English: the account's own ("你撤回了一条消息"), or someone else's
/// ("\"Ann\" 撤回了一条消息").
const RECALLED: [&str; 2] = ["撤回了一条消息", "recalled a message"];
/// How the account's own recall's notice starts.
const OWN: [&str; 2] = ["你", "You"];

/// A forward's `datatype` of a message in it: text, an image, a voice
/// message, a video, a link, a place, a file, a forwarded history.
const FORWARDED: [(i64, Kind); 8] = [
    (1, Kind::Text),
    (2, Kind::Image),
    (3, Kind::Voice),
    (4, Kind::Video),
    (5, Kind::Link),
    (6, Kind::Location),
    (8, Kind::File),
    (17, Kind::Forward),
];

/// What a message is.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub kind: Kind,
    /// What its kind is made of, the object its kind names in the answer.
    pub fields: Map<String, Value>,
    /// Who said it, when the content says so and the row does not: the
    /// account, for its own recall.
    pub sender: Option<String>,
}

impl Parsed {
    fn of(kind: Kind, fields: Value) -> Parsed {
        let fields = match fields {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        Parsed { kind, fields, sender: None }
    }

    fn unknown() -> Parsed {
        Parsed::of(Kind::Unknown, json!({}))
    }
}

/// `local_type`'s type and subtype: 49 and 57 for a quote.
pub fn split(local_type: i64) -> (i64, i64) {
    (local_type & 0xffff_ffff, local_type.checked_shr(32).unwrap_or(0))
}

/// What a message of `local_type` saying `content` (a group's without its
/// `<sender>:\n`), with `packed` its `packed_info_data`, is, seen by the
/// account `account`.
pub fn parse(account: &str, local_type: i64, content: &str, packed: &[u8]) -> Parsed {
    let (t, sub) = split(local_type);
    if t == TEXT {
        return Parsed::of(Kind::Text, json!({ "text": content }));
    }
    if t == SYSTEM || t == SYSTEM_XML {
        return system(account, content);
    }
    let sub = if t == APP && sub == 0 {
        xml(content, |root| text(child(root, "appmsg")?, "type")?.parse().ok()).unwrap_or(0)
    } else {
        sub
    };
    // A media message is its kind by its type alone: what it is made of
    // is what its file is found by, when its XML says.
    let media = match (t, sub) {
        (IMAGE, _) => Some(Kind::Image),
        (VOICE, _) => Some(Kind::Voice),
        (VIDEO, _) => Some(Kind::Video),
        (STICKER, _) | (APP, APP_STICKER) => Some(Kind::Sticker),
        (APP, APP_FILE) => Some(Kind::File),
        _ => None,
    };
    if let Some(kind) = media {
        return Parsed::of(kind, Value::Object(crate::media::fields(kind, content, packed)));
    }
    xml(content, |root| match t {
        CARD => card(root),
        LOCATION => location(root),
        APP => app(root, sub),
        _ => None,
    })
    .unwrap_or_else(Parsed::unknown)
}

/// What `read` makes of the XML document `content`, read as WeChat writes
/// it: from its first tag, without the XML declarations WeChat puts inside
/// some messages, and with an `&` that starts no entity, as in some links'
/// addresses. None when it is not XML even then.
pub fn xml<R>(content: &str, read: impl FnOnce(Node) -> Option<R>) -> Option<R> {
    let text = declarations_removed(content.trim());
    let escaped;
    let doc = match Document::parse(&text) {
        Ok(doc) => doc,
        Err(_) => {
            escaped = bare_ampersands_escaped(&text);
            Document::parse(&escaped).ok()?
        }
    };
    read(doc.root_element())
}

/// `text` without its `<?xml …?>` declarations.
fn declarations_removed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("<?xml") {
        out.push_str(before);
        rest = after.split_once("?>").map_or("", |(_, after)| after);
    }
    out.push_str(rest);
    out
}

/// `text` with each `&` that starts no entity (`&amp;`, `&#38;`) written
/// `&amp;`.
fn bare_ampersands_escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pieces = text.split('&');
    if let Some(first) = pieces.next() {
        out.push_str(first);
    }
    for piece in pieces {
        out.push('&');
        let entity = piece.split_once(';').is_some_and(|(name, _)| is_entity(name));
        if !entity {
            out.push_str("amp;");
        }
        out.push_str(piece);
    }
    out
}

/// Whether `&<name>;` is an entity XML reads: a name, or a character XML
/// allows by its number.
fn is_entity(name: &str) -> bool {
    let all =
        |s: &str, f: fn(&u8) -> bool| !s.is_empty() && s.len() <= 8 && s.bytes().all(|c| f(&c));
    // A character XML allows: not a control character but tab, newline
    // and return.
    let allowed = |n: Option<u32>| {
        n.and_then(char::from_u32)
            .is_some_and(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
    };
    match (name.strip_prefix("#x"), name.strip_prefix('#')) {
        (Some(hex), _) => {
            all(hex, u8::is_ascii_hexdigit) && allowed(u32::from_str_radix(hex, 16).ok())
        }
        (None, Some(decimal)) => all(decimal, u8::is_ascii_digit) && allowed(decimal.parse().ok()),
        (None, None) => {
            name.starts_with(|c: char| c.is_ascii_alphabetic())
                && all(name, u8::is_ascii_alphanumeric)
        }
    }
}

/// `node`'s child named `path`'s last part, through each part before it:
/// `child(n, "appmsg/refermsg/svrid")`.
pub fn child<'a, 'i>(node: Node<'a, 'i>, path: &str) -> Option<Node<'a, 'i>> {
    let mut at = node;
    for name in path.split('/') {
        at = named(at, name).into_iter().next()?;
    }
    Some(at)
}

/// `node`'s children named `name`, in order.
pub fn named<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    let mut found = Vec::new();
    for c in node.children() {
        if c.has_tag_name(name) {
            found.push(c);
        }
    }
    found
}

/// The text of `path` under `node`, trimmed: None when it is absent or
/// empty.
pub fn text(node: Node, path: &str) -> Option<String> {
    child(node, path).and_then(inner)
}

/// The text a node holds, trimmed, CDATA or not: None when empty.
fn inner(node: Node) -> Option<String> {
    let mut t = String::new();
    for n in node.children() {
        if let Some(text) = n.text().filter(|_| n.is_text()) {
            t.push_str(text);
        }
    }
    Some(t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Attribute `name` of `node`, trimmed: None when it is absent or empty.
pub fn attr(node: Node, name: &str) -> Option<String> {
    node.attribute(name).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn card(root: Node) -> Option<Parsed> {
    Some(Parsed::of(
        Kind::Card,
        json!({ "contact": attr(root, "username")?, "name": attr(root, "nickname") }),
    ))
}

fn location(root: Node) -> Option<Parsed> {
    let l = child(root, "location")?;
    let degrees = |name| attr(l, name)?.parse::<f64>().ok().filter(|d| d.is_finite());
    Some(Parsed::of(
        Kind::Location,
        json!({
            "latitude": degrees("x")?,
            "longitude": degrees("y")?,
            "name": attr(l, "poiname"),
            "address": attr(l, "label"),
        }),
    ))
}

/// An app message of subtype `sub`.
fn app(root: Node, sub: i64) -> Option<Parsed> {
    let a = child(root, "appmsg")?;
    match sub {
        QUOTE => {
            let r = child(a, "refermsg");
            let quoted = |name| r.and_then(|r| text(r, name));
            let excerpt = match quoted("type").as_deref() {
                Some("1") => quoted("content"),
                _ => None,
            };
            Some(Parsed::of(
                Kind::Quote,
                json!({
                    "text": text(a, "title").unwrap_or_default(),
                    "quoted": {
                        "id": quoted("svrid").filter(|id| id != "0"),
                        "sender": quoted("chatusr").or_else(|| quoted("fromusr")),
                        "name": quoted("displayname"),
                        "excerpt": excerpt,
                    },
                }),
            ))
        }
        FORWARD => {
            let mut messages = Vec::new();
            xml(&text(a, "recorditem")?, |r| {
                flatten(r, None, &mut messages);
                Some(())
            })?;
            Some(Parsed::of(
                Kind::Forward,
                json!({ "title": text(a, "title").unwrap_or_default(), "messages": messages }),
            ))
        }
        NUDGE => {
            let p = child(a, "patinfo")?;
            Some(Parsed::of(
                Kind::Nudge,
                json!({
                    "from": text(p, "fromusername")?,
                    "to": text(p, "pattedusername")?,
                    "text": text(a, "title").unwrap_or_default(),
                }),
            ))
        }
        TRANSFER => {
            let w = child(a, "wcpayinfo")?;
            let state = match text(w, "paysubtype").as_deref() {
                Some("1") => Some("sent"),
                Some("3") => Some("received"),
                Some("4") => Some("returned"),
                _ => None,
            };
            Some(Parsed::of(
                Kind::Transfer,
                json!({
                    "amount_fen": fen(text(w, "feedesc").as_deref()),
                    "state": state,
                    "memo": text(w, "pay_memo"),
                }),
            ))
        }
        RED_PACKET => {
            let w = child(a, "wcpayinfo")?;
            // A group collection (`newaa`, its bill numbered) is money
            // asked for, not sent.
            if text(w, "newaa/billno").is_some() {
                return None;
            }
            Some(Parsed::of(
                Kind::RedPacket,
                json!({
                    "amount_fen": fen(text(w, "feedesc").as_deref()),
                    "state": null,
                    "memo": text(w, "sendertitle").or_else(|| text(w, "receivertitle")),
                }),
            ))
        }
        sub => {
            let (_, to) = LINKS.into_iter().find(|(s, _)| *s == sub)?;
            let feed = child(a, "finderFeed").or_else(|| child(a, "finderLive"));
            let title = text(a, "title")
                .or_else(|| feed.and_then(|f| text(f, "desc").or_else(|| text(f, "nickname"))))?;
            let app_name = child(a, "weappinfo").and_then(|w| text(w, "appname"));
            Some(Parsed::of(
                Kind::Link,
                json!({
                    "to": to,
                    "title": title,
                    "summary": text(a, "des").or_else(|| feed.and_then(|f| text(f, "nickname"))),
                    "url": text(a, "url"),
                    "source": text(a, "sourcedisplayname").or(app_name),
                }),
            ))
        }
    }
}

/// An amount as WeChat shows it (`￥0.01`), in fen: None when it shows
/// none, or not in yuan.
pub fn fen(shown: Option<&str>) -> Option<i64> {
    let n = shown?.trim();
    let n = n.strip_prefix('￥').or_else(|| n.strip_prefix('¥'))?.trim();
    let (yuan, fen) = n.split_once('.').unwrap_or((n, ""));
    let digits = |d: &str| !d.is_empty() && d.len() <= 15 && d.bytes().all(|c| c.is_ascii_digit());
    let fen = match fen.len() {
        0 => "00".to_string(),
        1 => format!("{fen}0"),
        _ => fen.to_string(),
    };
    if !digits(yuan) || fen.len() != 2 || !digits(&fen) {
        return None;
    }
    yuan.parse::<i64>().ok()?.checked_mul(100)?.checked_add(fen.parse::<i64>().ok()?)
}

/// The messages of a `<recordinfo>` into `out`, in order, a forward nested
/// in it followed by its own, each naming in `inside` the position of the
/// forward it is in.
fn flatten(record: Node, inside: Option<usize>, out: &mut Vec<Value>) {
    let Some(list) = child(record, "datalist") else {
        return;
    };
    for item in named(list, "dataitem") {
        let datatype: i64 = attr(item, "datatype")
            .or_else(|| text(item, "datatype"))
            .and_then(|d| d.parse().ok())
            .unwrap_or(0);
        let kind = FORWARDED
            .into_iter()
            .find(|(d, _)| *d == datatype)
            .map_or(Kind::Unknown, |(_, kind)| kind);
        let source = child(item, "dataitemsource");
        let content = match kind {
            Kind::Text => text(item, "datadesc"),
            _ => text(item, "datatitle").or_else(|| text(item, "datadesc")),
        };
        let position = out.len();
        out.push(json!({
            "sender": source.and_then(|s| text(s, "realchatname").or_else(|| text(s, "fromusr"))),
            "name": text(item, "sourcename"),
            "at": text(item, "srcMsgCreateTime")
                .and_then(|s| s.parse::<i64>().ok())
                .and_then(wxread_page::rfc3339),
            "kind": kind.name(),
            "content": content.unwrap_or_default(),
            "type": datatype,
            "inside": inside,
        }));
        if kind == Kind::Forward {
            let nested = child(item, "recordxml");
            match nested.and_then(|r| child(r, "recordinfo")) {
                Some(r) => flatten(r, Some(position), out),
                None => {
                    xml(&nested.and_then(inner).unwrap_or_default(), |r| {
                        flatten(r, Some(position), out);
                        Some(())
                    });
                }
            }
        }
    }
}

/// A notice of WeChat's own: a recall, a join, a leave or a rename; else a
/// notice of another kind, its text; unknown when it has none to read.
fn system(account: &str, content: &str) -> Parsed {
    let notice = content.trim();
    if !notice.starts_with('<') {
        if RECALLED.iter().any(|r| notice.ends_with(r)) {
            let own = OWN.iter().any(|o| notice.starts_with(o));
            return Parsed {
                sender: own.then(|| account.to_string()),
                ..Parsed::of(Kind::Recall, json!({ "text": notice }))
            };
        }
        if notice.is_empty() {
            return Parsed::unknown();
        }
        return Parsed::of(Kind::System, json!({ "text": notice }));
    }
    xml(notice, |root| match root.attribute("type")? {
        "revokemsg" => {
            Some(Parsed::of(Kind::Recall, json!({ "text": text(root, "revokemsg/content")? })))
        }
        "sysmsgtemplate" => template(account, child(root, "sysmsgtemplate/content_template")?),
        _ => None,
    })
    .unwrap_or_else(Parsed::unknown)
}

/// A link of a template: its name, its members' ids and what it shows.
struct Slot {
    name: String,
    members: Vec<String>,
    shown: String,
}

impl Slot {
    /// `link`: a member list, shown by their names joined by its
    /// separator, or plain text.
    fn of(link: Node) -> Option<Slot> {
        let members: Vec<(String, String)> = child(link, "memberlist")
            .map(|m| {
                named(m, "member")
                    .into_iter()
                    .filter_map(|m| {
                        Some((text(m, "username")?, text(m, "nickname").unwrap_or_default()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let separator = text(link, "separator").unwrap_or_else(|| "、".into());
        let shown = if members.is_empty() {
            text(link, "plain").unwrap_or_default()
        } else {
            members.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(&separator)
        };
        Some(Slot {
            name: attr(link, "name")?,
            members: members.into_iter().map(|(u, _)| u).collect(),
            shown,
        })
    }
}

/// A `<content_template>`: its `<template>` with `$name$` slots, each
/// filled by its link (`link_list/link[@name]`): a join, a leave or a
/// rename, else a notice of another kind.
fn template(account: &str, t: Node) -> Option<Parsed> {
    let template = text(t, "template")?;
    let slots: Vec<Slot> = child(t, "link_list")
        .map(|l| named(l, "link").into_iter().filter_map(Slot::of).collect())
        .unwrap_or_default();
    let notice = slots
        .iter()
        .fold(template.clone(), |s, slot| s.replace(&format!("${}$", slot.name), &slot.shown));
    let slot = |name: &str| slots.iter().find(|s| s.name == name);
    // "你邀请…", "你将…移出": the account did it.
    let by_account = template.starts_with('你');
    let by = |name: &str| {
        if by_account {
            Some(account.to_string())
        } else {
            slot(name).and_then(|s| s.members.first().cloned())
        }
    };
    if template.contains("修改群名") {
        let name = slot("remark").map(|s| s.shown.clone()).filter(|n| !n.is_empty())?;
        return Some(Parsed::of(
            Kind::Renamed,
            json!({ "name": name, "by": by("username"), "text": notice }),
        ));
    }
    let (kind, by_slot) = if template.contains("加入") {
        // By a QR code, `adder` joined by `from`'s; else `username`
        // brought them in.
        (Kind::Join, if slot("adder").is_some() { "from" } else { "username" })
    } else if template.contains("移出") {
        (Kind::Leave, "username")
    } else {
        return Some(Parsed::of(Kind::System, json!({ "text": notice })));
    };
    let mut contacts: Vec<String> = Vec::new();
    for s in slots.iter().filter(|s| by_account || s.name != by_slot) {
        contacts.extend_from_slice(&s.members);
    }
    // "…邀请你和…加入了群聊": the account joined too.
    if template.contains("邀请你") {
        contacts.insert(0, account.to_string());
    }
    if contacts.is_empty() {
        return Some(Parsed::of(Kind::System, json!({ "text": notice })));
    }
    Some(Parsed::of(kind, json!({ "contacts": contacts, "by": by(by_slot), "text": notice })))
}
