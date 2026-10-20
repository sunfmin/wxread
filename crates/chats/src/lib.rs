//! What `wxread chats` answers of the rows it read: each chat with its
//! name, kind, times and members, newest first, a page at a time; or with
//! `--with`, only the chats that hold the contacts given, the smallest
//! first. The effect crate reads the rows out of the session and contact
//! databases; nothing here reads or writes.

use std::collections::{BTreeMap, BTreeSet};

use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_contacts::{QUERY_FLAG, display_name, unknown};

/// The option that finds the chats holding a contact.
pub const WITH_FLAG: &str = "--with";

/// The members of each group the account keeps a record of, by the
/// group's id: each member's id, as [`wxread_contacts::members`] reads
/// them.
pub type Rooms = BTreeMap<String, BTreeSet<String>>;

/// What the account calls each contact, by its id.
pub type Contacts = BTreeMap<String, Contact>;

/// How many are in a direct chat: the account and the contact.
pub const DIRECT_MEMBERS: usize = 2;

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
    /// The account's remark for it, else its own name; "" for neither.
    pub name: String,
    pub kind: Kind,
    /// When its last message was (RFC 3339, UTC), or None for none.
    pub last_message_at: Option<String>,
    pub unread: u64,
    /// How many are in it, the account included: [`DIRECT_MEMBERS`] for a
    /// direct chat; None for a group WeChat keeps no record of.
    pub members: Option<usize>,
}

/// Where a chat is in a list: by its size (0 for every chat, when the
/// list is not by size), then newest first by where WeChat sorts it, then
/// by id among those sorted alike, so a place is one chat's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    size: usize,
    sort: std::cmp::Reverse<i64>,
    id: String,
}

/// Where a page ended: the place of the last chat it listed. Its text is
/// the `next` an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    place: Place,
}

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        let mut parts = text.splitn(3, ':');
        let mut next = || parts.next();
        let (size, sort, id) = (next(), next(), next());
        size.and_then(|size| {
            let place = Place {
                size: size.parse().ok()?,
                sort: std::cmp::Reverse(sort?.parse().ok()?),
                id: id?.to_string(),
            };
            Some(Cursor { place })
        })
        .filter(|c| !c.place.id.is_empty())
        .ok_or_else(|| {
            format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread chats answered: pass the `next` of the answer before, with the same {WITH_FLAG}, or leave {CURSOR_FLAG} out to start at the first chat"
            )
        })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        let Place { size, sort, id } = &self.place;
        format!("{size}:{}:{id}", sort.0)
    }
}

/// One page of the chats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub chats: Vec<Chat>,
    /// Where the next page starts, when there are chats past this one.
    pub next: Option<Cursor>,
}

/// The page of the chats in `rows`, the members of whose groups are in
/// `rooms`, that starts after `after` (at the newest, for none) and holds
/// at most `limit`: newest first by where WeChat sorts them, and by id
/// among those sorted alike, so a cursor names one place and each chat is
/// on one page. A row with no id is no chat.
pub fn page(rows: Vec<Row>, rooms: &Rooms, after: Option<&Cursor>, limit: Limit) -> Page {
    paged(rows, rooms, false, after, limit)
}

/// The page of the chats that hold every contact `with` names: the direct
/// chat with the one, when it names one, and every group of `rooms` they
/// are all members of, whether or not `rows` has its chat; the smallest
/// first, then as [`page`] orders them. `contacts` names the chats `rows`
/// lacks. An id `with` names that is no contact, or a group, is refused
/// with its fix.
pub fn holding(
    rows: Vec<Row>,
    contacts: &Contacts,
    rooms: &Rooms,
    with: &[String],
    after: Option<&Cursor>,
    limit: Limit,
) -> Result<Page, String> {
    let mut faults = Vec::new();
    for id in with {
        if !contacts.contains_key(id) {
            faults.push(unknown(id));
        } else if Kind::of(id) == Kind::Group {
            faults.push(format!(
                "{id} is a group, and {WITH_FLAG} takes a person or an official account: `wxread group {id}` answers its members, and `wxread contacts {QUERY_FLAG} <name>` finds a person's id"
            ));
        }
    }
    if !faults.is_empty() {
        return Err(faults.join("; "));
    }
    let with: BTreeSet<&String> = with.iter().collect();
    let mut held: BTreeSet<&String> = rooms
        .iter()
        .filter(|(_, members)| with.iter().all(|w| members.contains(*w)))
        .map(|(id, _)| id)
        .collect();
    if let [one] = with.iter().collect::<Vec<_>>()[..] {
        held.insert(one);
    }
    let mut chats: Vec<Row> = rows.into_iter().filter(|r| held.remove(&r.id)).collect();
    chats.extend(held.into_iter().map(|id| Row {
        id: id.clone(),
        unread: None,
        last_timestamp: None,
        sort_timestamp: None,
        contact: contacts.get(id).cloned(),
    }));
    Ok(paged(chats, rooms, true, after, limit))
}

/// How many are in the chat `id`, the account included.
fn members(rooms: &Rooms, id: &str) -> Option<usize> {
    match Kind::of(id) {
        Kind::Direct => Some(DIRECT_MEMBERS),
        Kind::Group => rooms.get(id).map(BTreeSet::len),
    }
}

/// The page of `rows` after `after` holding at most `limit`, by size first
/// when `by_size`.
fn paged(
    rows: Vec<Row>,
    rooms: &Rooms,
    by_size: bool,
    after: Option<&Cursor>,
    limit: Limit,
) -> Page {
    let place = |r: &Row| Place {
        size: if by_size { members(rooms, &r.id).unwrap_or(0) } else { 0 },
        sort: std::cmp::Reverse(r.sort_timestamp.unwrap_or(0)),
        id: r.id.clone(),
    };
    let mut rows: Vec<(Place, Row)> = rows
        .into_iter()
        .filter(|r| !r.id.is_empty())
        .map(|r| (place(&r), r))
        .filter(|(p, _)| after.is_none_or(|c| *p > c.place))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let more = rows.len() > limit.get();
    rows.truncate(limit.get());
    let next = if more { rows.last().map(|(p, _)| Cursor { place: p.clone() }) } else { None };
    Page { chats: rows.into_iter().map(|(_, r)| chat(r, rooms)).collect(), next }
}

fn chat(row: Row, rooms: &Rooms) -> Chat {
    let name = row.contact.map_or_else(String::new, |c| {
        display_name(c.remark.as_deref(), None, c.nick_name.as_deref())
    });
    Chat {
        kind: Kind::of(&row.id),
        name,
        last_message_at: row.last_timestamp.and_then(rfc3339),
        unread: row.unread.and_then(|n| u64::try_from(n).ok()).unwrap_or(0),
        members: members(rooms, &row.id),
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

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn named(id: &str, name: &str) -> (String, Contact) {
        (id.to_string(), Contact { remark: None, nick_name: Some(name.to_string()) })
    }

    /// The account, Amy and Bob: a chat with Amy, a big group of all three
    /// with a chat, a small one with none, a group with Amy alone, and a
    /// group WeChat keeps no record of.
    fn world() -> (Vec<Row>, Contacts, Rooms) {
        let rows = vec![
            row("wxid_amy", 9),
            row("1@chatroom", 8),
            row("3@chatroom", 7),
            row("4@chatroom", 6),
            row("wxid_bob", 5),
        ];
        let contacts = Contacts::from([
            named("wxid_amy", "Amy"),
            named("wxid_bob", "Bob"),
            named("gh_news", "News"),
            named("1@chatroom", "Big"),
            named("2@chatroom", "Small"),
            named("3@chatroom", "Amy's"),
        ]);
        let rooms = Rooms::from([
            ("1@chatroom".into(), set(&["wxid_me", "wxid_amy", "wxid_bob", "wxid_x"])),
            ("2@chatroom".into(), set(&["wxid_me", "wxid_amy", "wxid_bob"])),
            ("3@chatroom".into(), set(&["wxid_me", "wxid_amy"])),
        ]);
        (rows, contacts, rooms)
    }

    fn with(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn with_two_contacts_only_the_groups_holding_both_are_answered_the_smallest_first() {
        let (rows, contacts, rooms) = world();
        let page = holding(
            rows,
            &contacts,
            &rooms,
            &with(&["wxid_amy", "wxid_bob"]),
            None,
            Limit::default_limit(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["2@chatroom", "1@chatroom"]);
        let small = &page.chats[0];
        assert_eq!(
            (small.name.as_str(), small.members, small.last_message_at.as_deref()),
            ("Small", Some(3), None)
        );
        assert_eq!(page.chats[1].members, Some(4));
        assert_eq!(page.next, None);
    }

    #[test]
    fn with_one_contact_the_direct_chat_comes_first_then_their_groups() {
        let (rows, contacts, rooms) = world();
        let page = holding(
            rows.clone(),
            &contacts,
            &rooms,
            &with(&["wxid_amy"]),
            None,
            Limit::default_limit(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["wxid_amy", "3@chatroom", "2@chatroom", "1@chatroom"]);
        let page = holding(
            rows.clone(),
            &contacts,
            &rooms,
            &with(&["wxid_amy", "wxid_amy"]),
            None,
            Limit::default_limit(),
        )
        .unwrap();
        assert_eq!(ids(&page)[0], "wxid_amy");
        let page =
            holding(rows, &contacts, &rooms, &with(&["gh_news"]), None, Limit::default_limit())
                .unwrap();
        assert_eq!(ids(&page), ["gh_news"]);
        assert_eq!(page.chats[0].name, "News");
    }

    #[test]
    fn with_an_id_that_is_no_contact_or_a_group_is_refused_with_its_fix() {
        let (rows, contacts, rooms) = world();
        let why = holding(
            rows,
            &contacts,
            &rooms,
            &with(&["wxid_nobody", "1@chatroom"]),
            None,
            Limit::default_limit(),
        )
        .unwrap_err();
        assert!(
            why.contains("no contact wxid_nobody") && why.contains("wxread contacts --query"),
            "{why}"
        );
        assert!(
            why.contains("1@chatroom is a group") && why.contains("`wxread group 1@chatroom`"),
            "{why}"
        );
    }

    fn ids(page: &Page) -> Vec<&str> {
        page.chats.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn chats_are_newest_first_and_by_id_among_equals() {
        let rows = vec![row("b", 5), row("a", 9), row("c", 5), row("d", 1), row("", 20)];
        let page = page(rows, &Rooms::new(), None, Limit::default_limit());
        assert_eq!(ids(&page), ["a", "b", "c", "d"]);
        assert_eq!(page.next, None);
    }

    #[test]
    fn each_chat_is_on_one_page() {
        let rows = vec![row("b", 5), row("a", 9), row("c", 5), row("d", 1), row("e", 5)];
        let two = Limit::new(2).unwrap();
        let first = page(rows.clone(), &Rooms::new(), None, two);
        assert_eq!(ids(&first), ["a", "b"]);
        let next = Cursor::parse(&first.next.unwrap().text()).unwrap();
        let second = page(rows.clone(), &Rooms::new(), Some(&next), two);
        assert_eq!(ids(&second), ["c", "e"]);
        let third = page(rows, &Rooms::new(), second.next.as_ref(), two);
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
            chat(r, &Rooms::new()).name
        };
        assert_eq!(named(Some("Mum"), Some("Mary")), "Mum");
        assert_eq!(named(Some(""), Some("Mary")), "Mary");
        assert_eq!(named(None, None), "");
        assert_eq!(chat(row("wxid_b", 1), &Rooms::new()).name, "");
    }

    #[test]
    fn a_chat_says_its_kind_times_and_unread() {
        let mut r = row("123@chatroom", 1_760_000_000);
        r.unread = Some(4);
        let rooms = Rooms::from([("123@chatroom".into(), set(&["wxid_a", "wxid_b", "wxid_me"]))]);
        let c = chat(r.clone(), &rooms);
        assert_eq!((c.kind, c.unread, c.members), (Kind::Group, 4, Some(3)));
        assert_eq!(chat(r, &Rooms::new()).members, None);
        assert_eq!(c.last_message_at.as_deref(), Some("2025-10-09T08:53:20Z"));
        let mut r = row("wxid_b", 0);
        r.unread = Some(-1);
        let c = chat(r, &Rooms::new());
        assert_eq!((c.kind, c.unread, c.last_message_at), (Kind::Direct, 0, None));
        assert_eq!(c.members, Some(DIRECT_MEMBERS));
    }

    #[test]
    fn a_cursor_wxread_did_not_write_is_refused_with_its_fix() {
        for text in ["", "x", "12", "a:b", "12:", "12:wxid_b", "0:12:", "x:12:wxid_b"] {
            let why = Cursor::parse(text).unwrap_err();
            assert!(why.contains("leave --cursor out"), "{why}");
        }
    }

    proptest! {
        #[test]
        fn paging_with_lists_every_chat_holding_them_once_in_order(
            groups in proptest::collection::vec((0usize..4, 0i64..3), 0..20),
            limit in 1usize..5,
        ) {
            let mut rooms = Rooms::new();
            let mut contacts = Contacts::from([named("wxid_amy", "Amy")]);
            let mut rows = vec![row("wxid_amy", 1)];
            for (i, &(extra, sort)) in groups.iter().enumerate() {
                let id = format!("{i}@chatroom");
                let mut members = set(&["wxid_me", "wxid_amy"]);
                members.extend((0..extra).map(|n| format!("wxid_{n}")));
                rooms.insert(id.clone(), members);
                contacts.insert(id.clone(), Contact { remark: None, nick_name: None });
                if sort > 0 {
                    rows.push(row(&id, sort));
                }
            }
            let amy = with(&["wxid_amy"]);
            let whole = holding(rows.clone(), &contacts, &rooms, &amy, None, Limit::new(MAX_LIMIT).unwrap()).unwrap();
            prop_assert_eq!(whole.chats.len(), groups.len() + 1);
            let sizes: Vec<usize> = whole.chats.iter().map(|c| c.members.unwrap()).collect();
            prop_assert!(sizes.windows(2).all(|w| w[0] <= w[1]));
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = holding(rows.clone(), &contacts, &rooms, &amy, after.as_ref(), Limit::new(limit).unwrap()).unwrap();
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
        fn paging_lists_every_chat_once_in_order(
            sorts in proptest::collection::vec(0i64..4, 0..30),
            limit in 1usize..7,
        ) {
            let rows: Vec<Row> = sorts.iter().enumerate().map(|(i, &s)| row(&format!("c{i:02}"), s)).collect();
            let whole = page(rows.clone(), &Rooms::new(), None, Limit::new(MAX_LIMIT).unwrap());
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = page(rows.clone(), &Rooms::new(), after.as_ref(), Limit::new(limit).unwrap());
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
