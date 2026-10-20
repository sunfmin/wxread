//! What `wxread chats` answers of the rows it read: each chat with its
//! name, kind and times, newest first, a page at a time. The effect crate
//! reads the rows out of the session and contact databases; nothing here
//! reads or writes.

use flux_rs::attrs::*;

/// The options that page.
pub const LIMIT_FLAG: &str = "--limit";
pub const CURSOR_FLAG: &str = "--cursor";

/// How many chats a page holds unless `--limit` says, and at most.
pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 1000;

/// One row of `SessionTable`, and the account's contact of the same id,
/// when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub unread: Option<i64>,
    pub last_timestamp: Option<i64>,
    pub sort_timestamp: Option<i64>,
    pub contact: Option<Contact>,
}

/// What a contact is called: the account's remark for it, and its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub remark: Option<String>,
    pub nick_name: Option<String>,
}

/// Whether a chat is with one contact or in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Direct,
    Group,
}

impl Kind {
    /// The kind of the chat `id`: a group's id is `<n>@chatroom`.
    pub fn of(id: &str) -> Kind {
        if id.ends_with("@chatroom") { Kind::Group } else { Kind::Direct }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Direct => "direct",
            Kind::Group => "group",
        }
    }
}

/// A chat as `wxread chats` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: String,
    /// The account's remark for it, else its own name; "" for neither.
    pub name: String,
    pub kind: Kind,
    /// When its last message was (RFC 3339, UTC), or None for none.
    pub last_message_at: Option<String>,
    pub unread: u64,
}

/// How many chats one page holds: from one to [`MAX_LIMIT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[refined_by(n: int)]
#[invariant(1 <= n && n <= MAX_LIMIT)]
pub struct Limit {
    #[field(usize[n])]
    n: usize,
}

impl Limit {
    /// The limit `n`, when it is one.
    pub fn new(n: usize) -> Option<Limit> {
        match n {
            1..=MAX_LIMIT => Some(Limit { n }),
            _ => None,
        }
    }

    #[spec(fn() -> Limit[DEFAULT_LIMIT])]
    pub fn default_limit() -> Limit {
        Limit { n: DEFAULT_LIMIT }
    }

    #[spec(fn(Limit[@n]) -> usize{v: v == n && 1 <= v && v <= MAX_LIMIT})]
    pub fn get(self) -> usize {
        self.n
    }
}

/// What fixes a `--limit` that is not one.
pub fn limit_refused(given: &str) -> String {
    format!(
        "{LIMIT_FLAG} {given} is not a number of chats from 1 to {MAX_LIMIT}: pass one, or leave {LIMIT_FLAG} out for {DEFAULT_LIMIT}"
    )
}

/// Where a page ended: the last chat it listed, by where WeChat sorts it.
/// Its text is the `next` an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    sort: i64,
    id: String,
}

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        text.split_once(':')
            .and_then(|(sort, id)| Some(Cursor { sort: sort.parse().ok()?, id: id.to_string() }))
            .filter(|c| !c.id.is_empty())
            .ok_or_else(|| {
                format!(
                    "{CURSOR_FLAG} {text} is not a `next` that wxread chats answered: pass the `next` of the answer before, or leave {CURSOR_FLAG} out to start at the newest chat"
                )
            })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        format!("{}:{}", self.sort, self.id)
    }

    fn of(row: &Row) -> Cursor {
        Cursor { sort: row.sort_timestamp.unwrap_or(0), id: row.id.clone() }
    }

    /// Whether `row` sorts after the chat this cursor names.
    fn before(&self, row: &Row) -> bool {
        let sort = row.sort_timestamp.unwrap_or(0);
        sort < self.sort || (sort == self.sort && row.id > self.id)
    }
}

/// One page of the chats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub chats: Vec<Chat>,
    /// Where the next page starts, when there are chats past this one.
    pub next: Option<Cursor>,
}

/// The page of the chats in `rows` that starts after `after` (at the
/// newest, for none) and holds at most `limit`: newest first by where
/// WeChat sorts them, and by id among those sorted alike, so a cursor
/// names one place and each chat is on one page. A row with no id is no
/// chat.
pub fn page(rows: Vec<Row>, after: Option<&Cursor>, limit: Limit) -> Page {
    let mut rows: Vec<Row> = rows
        .into_iter()
        .filter(|r| !r.id.is_empty() && after.is_none_or(|c| c.before(r)))
        .collect();
    rows.sort_by(|a, b| {
        let key = |r: &Row| r.sort_timestamp.unwrap_or(0);
        key(b).cmp(&key(a)).then_with(|| a.id.cmp(&b.id))
    });
    let more = rows.len() > limit.get();
    rows.truncate(limit.get());
    let next = if more { rows.last().map(Cursor::of) } else { None };
    Page { chats: rows.into_iter().map(chat).collect(), next }
}

fn chat(row: Row) -> Chat {
    let named = |s: Option<String>| s.filter(|s| !s.is_empty());
    let name = row
        .contact
        .and_then(|c| named(c.remark).or_else(|| named(c.nick_name)))
        .unwrap_or_default();
    Chat {
        kind: Kind::of(&row.id),
        name,
        last_message_at: row.last_timestamp.and_then(rfc3339),
        unread: row.unread.and_then(|n| u64::try_from(n).ok()).unwrap_or(0),
        id: row.id,
    }
}

/// `secs` since the epoch as RFC 3339 in UTC (`2026-10-20T01:02:03Z`); None
/// for none (0 or before) and for a time past the year 9999.
pub fn rfc3339(secs: i64) -> Option<String> {
    if secs <= 0 || secs > 253_402_300_799 {
        return None;
    }
    let days = secs.checked_div(86_400)?;
    let time = secs.checked_rem(86_400)?;
    // Howard Hinnant's civil_from_days, for a day on or after the epoch.
    let z = days.checked_add(719_468)?;
    let era = z.checked_div(146_097)?;
    let doe = z.checked_sub(era.checked_mul(146_097)?)?;
    let yoe = doe
        .checked_sub(doe.checked_div(1_460)?)?
        .checked_add(doe.checked_div(36_524)?)?
        .checked_sub(doe.checked_div(146_096)?)?
        .checked_div(365)?;
    let doy = doe.checked_sub(
        yoe.checked_mul(365)?
            .checked_add(yoe.checked_div(4)?)?
            .checked_sub(yoe.checked_div(100)?)?,
    )?;
    let mp = doy.checked_mul(5)?.checked_add(2)?.checked_div(153)?;
    let day =
        doy.checked_sub(mp.checked_mul(153)?.checked_add(2)?.checked_div(5)?)?.checked_add(1)?;
    let month = if mp < 10 { mp.checked_add(3)? } else { mp.checked_sub(9)? };
    let year = yoe.checked_add(era.checked_mul(400)?)?.checked_add(i64::from(month <= 2))?;
    let (hour, rest) = (time.checked_div(3_600)?, time.checked_rem(3_600)?);
    let (minute, second) = (rest.checked_div(60)?, rest.checked_rem(60)?);
    Some(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn row(id: &str, sort: i64) -> Row {
        Row {
            id: id.to_string(),
            unread: Some(0),
            last_timestamp: Some(sort),
            sort_timestamp: Some(sort),
            contact: None,
        }
    }

    fn ids(page: &Page) -> Vec<&str> {
        page.chats.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn chats_are_newest_first_and_by_id_among_equals() {
        let rows = vec![row("b", 5), row("a", 9), row("c", 5), row("d", 1), row("", 20)];
        let page = page(rows, None, Limit::default_limit());
        assert_eq!(ids(&page), ["a", "b", "c", "d"]);
        assert_eq!(page.next, None);
    }

    #[test]
    fn each_chat_is_on_one_page() {
        let rows = vec![row("b", 5), row("a", 9), row("c", 5), row("d", 1), row("e", 5)];
        let two = Limit::new(2).unwrap();
        let first = page(rows.clone(), None, two);
        assert_eq!(ids(&first), ["a", "b"]);
        let next = Cursor::parse(&first.next.unwrap().text()).unwrap();
        let second = page(rows.clone(), Some(&next), two);
        assert_eq!(ids(&second), ["c", "e"]);
        let third = page(rows, second.next.as_ref(), two);
        assert_eq!(ids(&third), ["d"]);
        assert_eq!(third.next, None);
    }

    #[test]
    fn a_chat_is_named_by_its_remark_else_its_name() {
        let named = |remark: Option<&str>, nick: Option<&str>| {
            let mut r = row("wxid_b", 1);
            r.contact = Some(Contact {
                remark: remark.map(String::from),
                nick_name: nick.map(String::from),
            });
            chat(r).name
        };
        assert_eq!(named(Some("Mum"), Some("Mary")), "Mum");
        assert_eq!(named(Some(""), Some("Mary")), "Mary");
        assert_eq!(named(None, None), "");
        assert_eq!(chat(row("wxid_b", 1)).name, "");
    }

    #[test]
    fn a_chat_says_its_kind_times_and_unread() {
        let mut r = row("123@chatroom", 1_760_000_000);
        r.unread = Some(4);
        let c = chat(r);
        assert_eq!((c.kind, c.unread), (Kind::Group, 4));
        assert_eq!(c.last_message_at.as_deref(), Some("2025-10-09T08:53:20Z"));
        let mut r = row("wxid_b", 0);
        r.unread = Some(-1);
        let c = chat(r);
        assert_eq!((c.kind, c.unread, c.last_message_at), (Kind::Direct, 0, None));
    }

    #[test]
    fn times_are_rfc3339_in_utc() {
        assert_eq!(rfc3339(1).as_deref(), Some("1970-01-01T00:00:01Z"));
        assert_eq!(rfc3339(951_782_400).as_deref(), Some("2000-02-29T00:00:00Z"));
        assert_eq!(rfc3339(253_402_300_799).as_deref(), Some("9999-12-31T23:59:59Z"));
        assert_eq!(rfc3339(253_402_300_800), None);
        assert_eq!(rfc3339(-5), None);
    }

    #[test]
    fn a_limit_is_one_to_the_most() {
        assert_eq!(Limit::new(0), None);
        assert_eq!(Limit::new(MAX_LIMIT + 1), None);
        assert_eq!(Limit::new(MAX_LIMIT).map(Limit::get), Some(MAX_LIMIT));
    }

    #[test]
    fn a_cursor_wxread_did_not_write_is_refused_with_its_fix() {
        for text in ["", "x", "12", "a:b", "12:"] {
            let why = Cursor::parse(text).unwrap_err();
            assert!(why.contains("leave --cursor out"), "{why}");
        }
    }

    proptest! {
        #[test]
        fn paging_lists_every_chat_once_in_order(
            sorts in proptest::collection::vec(0i64..4, 0..30),
            limit in 1usize..7,
        ) {
            let rows: Vec<Row> = sorts.iter().enumerate().map(|(i, &s)| row(&format!("c{i:02}"), s)).collect();
            let whole = page(rows.clone(), None, Limit::new(MAX_LIMIT).unwrap());
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = page(rows.clone(), after.as_ref(), Limit::new(limit).unwrap());
                prop_assert!(p.chats.len() <= limit);
                paged.extend(p.chats);
                match p.next {
                    Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                    None => break,
                }
            }
            prop_assert_eq!(paged, whole.chats);
        }

        #[test]
        fn a_time_is_written_as_the_seconds_it_was(secs in 1i64..253_402_300_800) {
            let text = rfc3339(secs).unwrap();
            prop_assert_eq!(text.len(), 20);
            let (date, time) = text.split_once('T').unwrap();
            let parts: Vec<i64> = time.trim_end_matches('Z').split(':').map(|p| p.parse().unwrap()).collect();
            prop_assert_eq!(parts[0] * 3600 + parts[1] * 60 + parts[2], secs % 86_400);
            prop_assert!(date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-');
        }
    }
}
