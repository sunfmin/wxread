//! What `wxread chats` answers of the rows it read: each chat with its
//! name, kind and times, newest first, a page at a time. The effect crate
//! reads the rows out of the session and contact databases; nothing here
//! reads or writes.

use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_contacts::shown_name;

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

/// What a contact is called: the account's remark for it, its own name,
/// and for a group with neither, the names of its members besides the
/// account, in the order WeChat lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub remark: Option<String>,
    pub nick_name: Option<String>,
    pub members: Vec<String>,
}

/// Whether a chat is with one contact or in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Direct,
    Group,
}

impl Kind {
    /// The kind of the chat `id`: in a group when its contact is one.
    pub fn of(id: &str) -> Kind {
        match wxread_contacts::Kind::of(id) {
            wxread_contacts::Kind::Group => Kind::Group,
            wxread_contacts::Kind::Person | wxread_contacts::Kind::OfficialAccount => Kind::Direct,
        }
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
    /// The account's remark for it, else its own name, else (a group) its
    /// members' names besides the account's, as WeChat joins them; "" for
    /// none.
    pub name: String,
    pub kind: Kind,
    /// When its last message was (RFC 3339, UTC), or None for none.
    pub last_message_at: Option<String>,
    pub unread: u64,
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
    let name = row.contact.map_or_else(String::new, |c| {
        shown_name(c.remark.as_deref(), c.nick_name.as_deref(), &c.members)
    });
    Chat {
        kind: Kind::of(&row.id),
        name,
        last_message_at: row.last_timestamp.and_then(rfc3339),
        unread: row.unread.and_then(|n| u64::try_from(n).ok()).unwrap_or(0),
        id: row.id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use wxread_answer::MAX_LIMIT;

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
                members: Vec::new(),
            });
            chat(r).name
        };
        assert_eq!(named(Some("Mum"), Some("Mary")), "Mum");
        assert_eq!(named(Some(""), Some("Mary")), "Mary");
        assert_eq!(named(None, None), "");
        assert_eq!(chat(row("wxid_b", 1)).name, "");
    }

    #[test]
    fn an_unnamed_group_is_named_by_its_members_and_a_named_one_by_its_name() {
        let named = |remark: &str, nick: &str| {
            let mut r = row("9@chatroom", 1);
            r.contact = Some(Contact {
                remark: Some(remark.into()),
                nick_name: Some(nick.into()),
                members: vec!["孙凤君".into(), "秦俊斌 @qinjunbin".into()],
            });
            chat(r).name
        };
        assert_eq!(named("", ""), "孙凤君、秦俊斌 @qinjunbin");
        assert_eq!(named("", "Hiking"), "Hiking");
        assert_eq!(named("Ours", "Hiking"), "Ours");
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
    }
}
