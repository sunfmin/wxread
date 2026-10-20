//! What `wxread moments` answers of the Moments (朋友圈) WeChat has
//! loaded: each with its author, when it was posted, its text, location,
//! link and media, and its likes and comments as WeChat last loaded them,
//! newest first, a page at a time; and which of a Moment's media `wxread
//! file get` is asked for. The effect crate reads the rows out of
//! `sns/sns.db` and the names out of the contacts; nothing here reads or
//! writes.
//!
//! A Moment's XML is as SIWX and wx-workbench found WeChat 4.x writes it:
//! `<SnsDataItem>`, its `<TimelineObject>` the Moment as WeChat's server
//! sent it, its `<LocalExtraInfo>` the likes and comments WeChat loaded.

use std::collections::BTreeMap;

use roxmltree::Node;
use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_media::Media;
use wxread_xml::{attr, child, named, text, xml};

/// The option that picks one author's Moments.
pub const CONTACT_FLAG: &str = "--contact";

/// WeChat's type of a Moment's image and of its video, in `<media><type>`.
pub const IMAGE: &str = "2";
pub const VIDEO: &str = "6";

/// One Moment, as `SnsTimeLine` holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `tid`: WeChat's id of it, unsigned, as SQLite reads it, signed.
    pub tid: i64,
    /// `user_name`: its author's id.
    pub author: String,
    /// `content`: its XML.
    pub content: String,
}

impl Row {
    /// Its id, as WeChat writes it: `tid` read unsigned.
    pub fn id(&self) -> u64 {
        u64::from_ne_bytes(self.tid.to_ne_bytes())
    }
}

/// A contact or member as a Moment names them: their id, and their name
/// as WeChat shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: String,
    pub name: String,
}

/// Where a Moment was posted from, as its author tagged it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub name: String,
    pub address: String,
    pub city: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
}

/// What a Moment links to: an article, a song, a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    pub description: String,
    pub url: String,
}

/// One of a Moment's media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaOf {
    /// What `wxread file get` takes: `<the Moment's id>:<its place>`.
    pub id: String,
    /// `image`, `video`, or `other` for a kind WeChat keeps no file of.
    pub kind: &'static str,
    /// Where WeChat's CDN serves it, and its thumbnail; "" for none.
    pub url: String,
    pub thumbnail_url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// What finds the file WeChat caches of it: None for `other`.
    pub file: Option<Media>,
}

/// One like, or one comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub by: Person,
    /// When (RFC 3339, UTC); None for no time.
    pub at: Option<String>,
    /// A comment's text; "" for a like.
    pub text: String,
    /// The comment it answers, by its author; None for none.
    pub reply_to: Option<Person>,
}

/// One Moment, as `wxread moments` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub id: String,
    pub author: Person,
    /// When it was posted (RFC 3339, UTC); None for no time.
    pub at: Option<String>,
    /// What its author wrote; "" for nothing.
    pub text: String,
    pub location: Option<Location>,
    pub link: Option<Link>,
    pub media: Vec<MediaOf>,
    pub likes: Vec<Said>,
    pub comments: Vec<Said>,
    /// WeChat's type of it (`<ContentObject><type>`); None when it says none.
    pub kind: Option<i64>,
    /// Its XML, as WeChat keeps it.
    pub content: String,
}

/// Where a page ended: the last Moment it listed. Its text, the Moment's
/// id, is the `next` an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor(u64);

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        digits(text).map(Cursor).ok_or_else(|| {
            format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread moments answered: pass the `next` of the answer before, or leave {CURSOR_FLAG} out to start at the newest Moment"
            )
        })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        self.0.to_string()
    }
}

/// `text` as a number written in digits alone.
fn digits(text: &str) -> Option<u64> {
    Some(text).filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))?.parse().ok()
}

/// One page of the Moments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub moments: Vec<Moment>,
    pub next: Option<Cursor>,
}

/// The page of the Moments in `rows` that starts after `after` (at the
/// newest, for none) and holds at most `limit`, each named by `names` (what
/// WeChat shows for each contact, by id). Newest first: a Moment's id
/// begins with the millisecond it was posted in, so it orders them.
pub fn page(
    rows: &[Row],
    names: &BTreeMap<String, String>,
    after: Option<&Cursor>,
    limit: Limit,
) -> Page {
    let mut rows: Vec<&Row> = rows.iter().filter(|r| after.is_none_or(|c| r.id() < c.0)).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.id()));
    let more = rows.len() > limit.get();
    rows.truncate(limit.get());
    let next = if more { rows.last().map(|r| Cursor(r.id())) } else { None };
    Page { moments: rows.into_iter().map(|r| moment(r, names)).collect(), next }
}

/// `row` as the answer has it, each person named by `names`, else by the
/// name the Moment gives them. A Moment whose XML does not read is still
/// listed, with its id, author and content.
pub fn moment(row: &Row, names: &BTreeMap<String, String>) -> Moment {
    let id = row.id().to_string();
    let person = |who: &str, given| person(names, who, given);
    let bare = Moment {
        id: id.clone(),
        author: person(&row.author, None),
        at: None,
        text: String::new(),
        location: None,
        link: None,
        media: Vec::new(),
        likes: Vec::new(),
        comments: Vec::new(),
        kind: None,
        content: row.content.clone(),
    };
    xml(&row.content, |root| {
        let timeline = find(root, "TimelineObject")?;
        let local = find(root, "LocalExtraInfo");
        let object = child(timeline, "ContentObject");
        let field = |node: Option<Node>, path| node.and_then(|n| text(n, path)).unwrap_or_default();
        // WeChat's id of the Moment as its XML gives it, which its media
        // are cached by; the row's when it gives none.
        let post = text(timeline, "id").and_then(|p| p.parse().ok()).unwrap_or(row.id());
        let media = object
            .and_then(|o| child(o, "mediaList"))
            .map(|list| named(list, "media"))
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(n, m)| media_of(&id, post, n, m))
            .collect();
        let link = Some(field(object, "contentUrl")).filter(|u| !u.is_empty()).map(|url| Link {
            title: field(object, "title"),
            description: field(object, "description"),
            url,
        });
        Some(Moment {
            author: person(&row.author, local.and_then(|l| text(l, "nickname"))),
            at: text(timeline, "createTime").and_then(|t| t.parse().ok()).and_then(rfc3339),
            text: field(Some(timeline), "contentDesc"),
            location: child(timeline, "location").and_then(location),
            link,
            media,
            likes: said(local, "like_user_list", names),
            comments: said(local, "comment_user_list", names),
            kind: object.and_then(|o| text(o, "type")).and_then(|t| t.parse().ok()),
            ..bare.clone()
        })
    })
    .unwrap_or(bare)
}

/// The contact `who`, named by `names`, else `given`.
fn person(names: &BTreeMap<String, String>, who: &str, given: Option<String>) -> Person {
    let known = names.get(who).filter(|n| !n.is_empty()).cloned();
    Person { id: who.to_string(), name: known.or(given).unwrap_or_default() }
}

/// The likes or comments in `local`'s `list` that are not deleted, each
/// person named by `names`.
fn said(local: Option<Node>, list: &str, names: &BTreeMap<String, String>) -> Vec<Said> {
    let mut all = Vec::new();
    let Some(list) = local.and_then(|l| child(l, list)) else {
        return all;
    };
    for c in named(list, "user_comment") {
        let deleted = text(c, "b_deleted").is_some_and(|d| d != "0");
        let Some(by) = text(c, "username").filter(|_| !deleted) else {
            continue;
        };
        let at = text(c, "create_time").and_then(|t| t.parse().ok()).and_then(rfc3339);
        all.push(Said {
            by: person(names, &by, text(c, "nickname")),
            at,
            text: text(c, "content").unwrap_or_default(),
            reply_to: text(c, "ref_username").map(|r| person(names, &r, None)),
        });
    }
    all
}

/// The first element named `name` at or under `root`.
fn find<'a, 'i>(root: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    root.descendants().find(|n| n.has_tag_name(name))
}

/// The `n`th medium of the Moment `id`, `post` as its XML names it. One
/// whose XML gives no id of it is cached under no name it can be found
/// by: it has no file.
fn media_of(id: &str, post: u64, n: usize, m: Node) -> MediaOf {
    let media = text(m, "id").and_then(|k| k.parse().ok());
    let (kind, file) = match text(m, "type").unwrap_or_default().as_str() {
        IMAGE => ("image", media.map(|media| Media::MomentImage { post, media })),
        VIDEO => ("video", media.map(|media| Media::MomentVideo { post, media })),
        _ => ("other", None),
    };
    let size = child(m, "size");
    let dimension = |name| size.and_then(|s| attr(s, name)).and_then(|v| v.parse().ok());
    MediaOf {
        id: format!("{id}:{n}"),
        kind,
        url: text(m, "url").unwrap_or_default(),
        thumbnail_url: text(m, "thumb").unwrap_or_default(),
        width: dimension("width"),
        height: dimension("height"),
        file,
    }
}

/// The location `node` tags, when it tags any.
fn location(node: Node) -> Option<Location> {
    let given = |name| attr(node, name).unwrap_or_default();
    let degrees = |name| attr(node, name).filter(|v| v.parse::<f64>().is_ok_and(|d| d != 0.0));
    let found = Location {
        name: given("poiName"),
        address: given("poiAddress"),
        city: given("city"),
        latitude: degrees("latitude"),
        longitude: degrees("longitude"),
    };
    let none = found.name.is_empty()
        && found.address.is_empty()
        && found.city.is_empty()
        && found.latitude.is_none()
        && found.longitude.is_none();
    (!none).then_some(found)
}

/// One of a Moment's media, as `wxread file get` names it: the Moment's id
/// and the medium's place among its media, from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaId {
    pub moment: u64,
    pub place: usize,
}

impl MediaId {
    /// The medium `text` names (`<moment id>:<place>`); None when it names
    /// none, as a message's id does not.
    pub fn parse(text: &str) -> Option<MediaId> {
        let (moment, place) = text.split_once(':')?;
        Some(MediaId { moment: digits(moment)?, place: usize::try_from(digits(place)?).ok()? })
    }

    /// The Moment's `tid`, as SQLite holds it.
    pub fn tid(&self) -> i64 {
        i64::from_ne_bytes(self.moment.to_ne_bytes())
    }
}

/// What fixes naming a Moment `sns/sns.db` does not hold.
pub fn no_such_moment(id: &str, account: &str) -> String {
    format!(
        "account {account} holds no Moment of {id}: pass a medium's id as `wxread moments` answers it, under media (<the Moment's id>:<its place>); WeChat may have dropped the Moment since"
    )
}

/// What fixes naming a medium of Moment `moment` no file of which can be
/// found: one of kind other, or one its Moment gives no id of.
pub fn not_a_file(id: &str, moment: &str) -> String {
    format!(
        "{id} is a medium of Moment {moment} no file of which can be found (only a Moment's images and videos have one, cached under the id the Moment gives them): pass the id of one whose file_present is not null, as `wxread moments` answers it"
    )
}

/// What fixes naming a medium past a Moment's last.
pub fn no_such_medium(id: &str, count: usize) -> String {
    format!(
        "{id} names no medium: its Moment has {count}, from 0; pass one as `wxread moments` answers it, under media"
    )
}

#[cfg(test)]
mod tests;
