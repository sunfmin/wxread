//! What `wxread chats` answers of the rows it read: each chat with its
//! name, kind, times and members, newest first, a page at a time; or with
//! `--with`, only the chats that hold the contacts given, the smallest
//! first; with `--query`, only the chats whose id or name holds a text.
//! The effect crate reads the rows out of the session and contact
//! databases; nothing here reads or writes.

use std::collections::{BTreeMap, BTreeSet};

use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_contacts::{QUERY_FLAG, shown_name, unknown};

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

/// What a contact is called: the account's remark for it, its own name,
/// and for a group with neither, the names of its members besides the
/// account, in the order WeChat lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub remark: Option<String>,
    pub nick_name: Option<String>,
    pub members: Vec<String>,
}

/// One of WeChat's folders: an entry of its chat list that holds chats,
/// or official accounts' messages, rather than being a chat; no message
/// is ever its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// The groups the account folded (折叠的群聊), each a chat of its own.
    FoldedGroups,
    /// What subscription accounts sent (订阅号消息).
    Subscriptions,
    /// The service accounts (服务号).
    Services,
}

impl Folder {
    pub const ALL: [Folder; 3] = [Folder::FoldedGroups, Folder::Subscriptions, Folder::Services];

    /// The folder whose id in the chat list is `id`, when it is one.
    pub fn of(id: &str) -> Option<Folder> {
        Folder::ALL.into_iter().find(|f| f.id() == id)
    }

    /// Its id in WeChat's chat list (`SessionTable`).
    pub fn id(self) -> &'static str {
        match self {
            Folder::FoldedGroups => "@placeholder_foldgroup",
            Folder::Subscriptions => "brandsessionholder",
            Folder::Services => "brandservicesessionholder",
        }
    }

    /// What WeChat names it in its chat list.
    pub fn name(self) -> &'static str {
        match self {
            Folder::FoldedGroups => "折叠的群聊",
            Folder::Subscriptions => "订阅号消息",
            Folder::Services => "服务号",
        }
    }

    /// The command that reads what it holds.
    pub fn read_with(self) -> &'static str {
        match self {
            Folder::FoldedGroups => "wxread chats",
            Folder::Subscriptions | Folder::Services => "wxread articles",
        }
    }

    /// What fixes asking for its messages, as though it were a chat.
    pub fn not_a_chat(self) -> String {
        let (id, name, with) = (self.id(), self.name(), self.read_with());
        match self {
            Folder::FoldedGroups => format!(
                "{id} is WeChat's folder {name}, not a chat, and has no messages of its own: `{with}` lists each group folded into it as a chat of its own, and `wxread messages --chat <its id>` reads one"
            ),
            Folder::Subscriptions | Folder::Services => format!(
                "{id} is WeChat's folder {name}, not a chat, and has no messages of its own: `{with}` reads what its official accounts sent, `--official-account <gh_... id>` one account's"
            ),
        }
    }
}

/// Whether a chat is with one contact, in a group, or one of WeChat's
/// folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Direct,
    Group,
    Folder(Folder),
}

impl Kind {
    /// The kind of the chat `id`: a folder when it is one's, in a group
    /// when its contact is one.
    pub fn of(id: &str) -> Kind {
        if let Some(folder) = Folder::of(id) {
            return Kind::Folder(folder);
        }
        match wxread_contacts::Kind::of(id) {
            wxread_contacts::Kind::Group => Kind::Group,
            wxread_contacts::Kind::Person | wxread_contacts::Kind::OfficialAccount => Kind::Direct,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Direct => "direct",
            Kind::Group => "group",
            Kind::Folder(_) => "folder",
        }
    }
}

/// A chat as `wxread chats` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: String,
    /// The account's remark for it, else its own name, else (a group) its
    /// members' names besides the account's, as WeChat joins them; a
    /// folder's, as WeChat names it; "" for none.
    pub name: String,
    pub kind: Kind,
    /// The command that reads what it holds: its messages, or for a
    /// folder, the chats or articles in it.
    pub read_with: String,
    /// When its last message was (RFC 3339, UTC), or None for none.
    pub last_message_at: Option<String>,
    pub unread: u64,
    /// How many are in it, the account included: [`DIRECT_MEMBERS`] for a
    /// direct chat; None for a group WeChat keeps no record of, and for a
    /// folder.
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
                "{CURSOR_FLAG} {text} is not a `next` that wxread chats answered: pass the `next` of the answer before, with the same {QUERY_FLAG} and {WITH_FLAG}, or leave {CURSOR_FLAG} out to start at the first chat"
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
/// `rooms`, that `query` finds (every one, for none), that starts after
/// `after` (at the newest, for none) and holds at most `limit`: newest
/// first by where WeChat sorts them, and by id among those sorted alike,
/// so a cursor names one place and each chat is on one page. A row with
/// no id is no chat.
pub fn page(
    rows: Vec<Row>,
    rooms: &Rooms,
    query: Option<&str>,
    after: Option<&Cursor>,
    limit: Limit,
) -> Page {
    paged(rows, rooms, false, query, after, limit)
}

/// The page of the chats that hold every contact `with` names: the direct
/// chat with the one, when it names one, and every group of `rooms` they
/// are all members of, whether or not `rows` has its chat; the smallest
/// first, then as [`page`] orders them, and of those only what `query`
/// finds. `contacts` names the chats `rows` lacks. An id `with` names that
/// is no contact, or a group, is refused with its fix.
pub fn holding(
    rows: Vec<Row>,
    contacts: &Contacts,
    rooms: &Rooms,
    with: &[String],
    query: Option<&str>,
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
    let mut held: BTreeSet<&String> = BTreeSet::new();
    for (id, members) in rooms {
        if with.iter().all(|w| members.contains(*w)) {
            held.insert(id);
        }
    }
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
    Ok(paged(chats, rooms, true, query, after, limit))
}

/// How many are in the chat `id`, the account included.
fn members(rooms: &Rooms, id: &str) -> Option<usize> {
    match Kind::of(id) {
        Kind::Direct => Some(DIRECT_MEMBERS),
        Kind::Group => rooms.get(id).map(BTreeSet::len),
        Kind::Folder(_) => None,
    }
}

/// The page of the `rows` `query` finds after `after` holding at most
/// `limit`, by size first when `by_size`.
fn paged(
    rows: Vec<Row>,
    rooms: &Rooms,
    by_size: bool,
    query: Option<&str>,
    after: Option<&Cursor>,
    limit: Limit,
) -> Page {
    let place = |r: &Row| Place {
        size: if by_size { members(rooms, &r.id).unwrap_or(0) } else { 0 },
        sort: std::cmp::Reverse(r.sort_timestamp.unwrap_or(0)),
        id: r.id.clone(),
    };
    let mut placed: Vec<(Place, Row)> = Vec::new();
    for row in rows {
        let at = place(&row);
        let found = query.is_none_or(|q| matches(&row, q));
        if !row.id.is_empty() && found && after.is_none_or(|c| at > c.place) {
            placed.push((at, row));
        }
    }
    placed.sort_by(|a, b| a.0.cmp(&b.0));
    let more = placed.len() > limit.get();
    placed.truncate(limit.get());
    let next = if more { placed.last().map(|(p, _)| Cursor { place: p.clone() }) } else { None };
    Page { chats: placed.into_iter().map(|(_, r)| chat(r, rooms)).collect(), next }
}

/// Whether `row` is found by `query`: its id, the name WeChat shows it by
/// (an unnamed group's, its members' names, and a folder's included) or
/// its own name holds it, whatever their case. Every chat is found by "".
pub fn matches(row: &Row, query: &str) -> bool {
    let query = query.to_lowercase();
    let shown = name(row);
    let own = row.contact.as_ref().and_then(|c| c.nick_name.as_deref());
    [Some(row.id.as_str()), Some(shown.as_str()), own]
        .into_iter()
        .flatten()
        .any(|text| text.to_lowercase().contains(&query))
}

/// The name WeChat shows the chat of `row` by: a folder's own, else its
/// contact's shown name; "" for no contact.
fn name(row: &Row) -> String {
    match Folder::of(&row.id) {
        Some(folder) => folder.name().to_string(),
        None => row.contact.as_ref().map_or_else(String::new, |c| {
            shown_name(c.remark.as_deref(), c.nick_name.as_deref(), &c.members)
        }),
    }
}

/// The command that reads what the chat `id` holds.
fn read_with(id: &str) -> String {
    match Folder::of(id) {
        Some(folder) => folder.read_with().to_string(),
        None => format!("wxread messages --chat {id}"),
    }
}

fn chat(row: Row, rooms: &Rooms) -> Chat {
    Chat {
        kind: Kind::of(&row.id),
        name: name(&row),
        read_with: read_with(&row.id),
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
        (
            id.to_string(),
            Contact { remark: None, nick_name: Some(name.to_string()), members: Vec::new() },
        )
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
            None,
            Limit::default_limit(),
        )
        .unwrap();
        assert_eq!(ids(&page)[0], "wxid_amy");
        let page = holding(
            rows,
            &contacts,
            &rooms,
            &with(&["gh_news"]),
            None,
            None,
            Limit::default_limit(),
        )
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
        let page = page(rows, &Rooms::new(), None, None, Limit::default_limit());
        assert_eq!(ids(&page), ["a", "b", "c", "d"]);
        assert_eq!(page.next, None);
    }

    #[test]
    fn each_chat_is_on_one_page() {
        let rows = vec![row("b", 5), row("a", 9), row("c", 5), row("d", 1), row("e", 5)];
        let two = Limit::new(2).unwrap();
        let first = page(rows.clone(), &Rooms::new(), None, None, two);
        assert_eq!(ids(&first), ["a", "b"]);
        let next = Cursor::parse(&first.next.unwrap().text()).unwrap();
        let second = page(rows.clone(), &Rooms::new(), None, Some(&next), two);
        assert_eq!(ids(&second), ["c", "e"]);
        let third = page(rows, &Rooms::new(), None, second.next.as_ref(), two);
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
            chat(r, &Rooms::new()).name
        };
        assert_eq!(named(Some("Mum"), Some("Mary")), "Mum");
        assert_eq!(named(Some(""), Some("Mary")), "Mary");
        assert_eq!(named(None, None), "");
        assert_eq!(chat(row("wxid_b", 1), &Rooms::new()).name, "");
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
            chat(r, &Rooms::new()).name
        };
        assert_eq!(named("", ""), "孙凤君、秦俊斌 @qinjunbin");
        assert_eq!(named("", "Hiking"), "Hiking");
        assert_eq!(named("Ours", "Hiking"), "Ours");
    }

    fn contact(remark: &str, nick: &str, members: &[&str]) -> Option<Contact> {
        Some(Contact {
            remark: Some(remark.into()),
            nick_name: Some(nick.into()),
            members: members.iter().map(|m| m.to_string()).collect(),
        })
    }

    #[test]
    fn a_query_finds_a_chat_by_its_id_its_shown_name_or_its_own_name_whatever_the_case() {
        let mut rows = vec![
            row("wxid_qin", 6),
            row("1@chatroom", 5),
            row("2@chatroom", 4),
            row("3@chatroom", 3),
            row("wxid_amy", 2),
            row("wxid_stranger", 1),
        ];
        rows[0].contact = contact("", "秦俊斌 @QinJunbin", &[]);
        rows[1].contact = contact("", "", &["孙凤君", "秦俊斌 @QinJunbin"]);
        rows[2].contact = contact("", "Hiking", &["秦俊斌 @QinJunbin"]);
        rows[3].contact = contact("Ours", "Climbing", &[]);
        rows[4].contact = contact("Auntie", "Amy", &[]);
        let found = |query: &str| {
            let p = page(rows.clone(), &Rooms::new(), Some(query), None, Limit::default_limit());
            ids(&p).into_iter().map(String::from).collect::<Vec<_>>()
        };
        assert_eq!(found("秦俊斌"), ["wxid_qin", "1@chatroom"]);
        assert_eq!(found("qinjunbin"), ["wxid_qin", "1@chatroom"]);
        assert_eq!(found("hIKING"), ["2@chatroom"]);
        assert_eq!(found("ours"), ["3@chatroom"]);
        assert_eq!(found("climb"), ["3@chatroom"]);
        assert_eq!(found("auntie"), ["wxid_amy"]);
        assert_eq!(found("amy"), ["wxid_amy"]);
        assert_eq!(found("STRANGER"), ["wxid_stranger"]);
        assert_eq!(found("nobody"), Vec::<String>::new());
        assert_eq!(found("").len(), rows.len());
    }

    #[test]
    fn with_a_query_only_the_chats_holding_them_that_it_finds_are_answered() {
        let (rows, contacts, rooms) = world();
        let page = holding(
            rows,
            &contacts,
            &rooms,
            &with(&["wxid_amy"]),
            Some("SMALL"),
            None,
            Limit::default_limit(),
        )
        .unwrap();
        assert_eq!(ids(&page), ["2@chatroom"]);
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

    #[test]
    fn a_folder_is_its_own_kind_named_as_wechat_names_it_saying_what_reads_it() {
        let mut r = row("brandsessionholder", 3);
        r.contact = contact("", "", &[]);
        let c = chat(r, &Rooms::new());
        assert_eq!(
            (c.kind, c.name.as_str(), c.members, c.read_with.as_str()),
            (Kind::Folder(Folder::Subscriptions), "订阅号消息", None, "wxread articles")
        );
        assert_eq!(c.kind.name(), "folder");
        let c = chat(row("@placeholder_foldgroup", 2), &Rooms::new());
        assert_eq!((c.kind, c.name.as_str()), (Kind::Folder(Folder::FoldedGroups), "折叠的群聊"));
        assert_eq!(c.read_with, "wxread chats");
        let c = chat(row("brandservicesessionholder", 1), &Rooms::new());
        assert_eq!((c.kind, c.name.as_str()), (Kind::Folder(Folder::Services), "服务号"));
        assert_eq!(c.read_with, "wxread articles");
        for f in Folder::ALL {
            assert_eq!(Folder::of(f.id()), Some(f));
            assert_eq!(Kind::of(f.id()), Kind::Folder(f));
        }
        assert_eq!(Folder::of("wxid_amy"), None);
    }

    #[test]
    fn a_chat_says_the_command_that_reads_its_messages() {
        assert_eq!(
            chat(row("wxid_b", 1), &Rooms::new()).read_with,
            "wxread messages --chat wxid_b"
        );
        assert_eq!(
            chat(row("9@chatroom", 1), &Rooms::new()).read_with,
            "wxread messages --chat 9@chatroom"
        );
    }

    #[test]
    fn a_query_finds_a_folder_by_wechats_name_for_it() {
        let rows = vec![row("brandsessionholder", 2), row("@placeholder_foldgroup", 1)];
        let p = page(rows, &Rooms::new(), Some("订阅号"), None, Limit::default_limit());
        assert_eq!(ids(&p), ["brandsessionholder"]);
    }

    #[test]
    fn a_folder_is_no_chat_and_its_refusal_names_what_reads_it() {
        let why = Folder::Subscriptions.not_a_chat();
        assert!(why.contains("brandsessionholder") && why.contains("订阅号消息"), "{why}");
        assert!(why.contains("`wxread articles`") && !why.contains("chats` lists"), "{why}");
        let why = Folder::FoldedGroups.not_a_chat();
        assert!(why.contains("`wxread chats`") && why.contains("--chat <its id>"), "{why}");
        assert!(Folder::Services.not_a_chat().contains("`wxread articles`"));
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
                contacts.insert(id.clone(), Contact { remark: None, nick_name: None, members: Vec::new() });
                if sort > 0 {
                    rows.push(row(&id, sort));
                }
            }
            let amy = with(&["wxid_amy"]);
            let whole = holding(rows.clone(), &contacts, &rooms, &amy, None, None, Limit::new(MAX_LIMIT).unwrap()).unwrap();
            prop_assert_eq!(whole.chats.len(), groups.len() + 1);
            let sizes: Vec<usize> = whole.chats.iter().map(|c| c.members.unwrap()).collect();
            prop_assert!(sizes.windows(2).all(|w| w[0] <= w[1]));
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = holding(rows.clone(), &contacts, &rooms, &amy, None, after.as_ref(), Limit::new(limit).unwrap()).unwrap();
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
        fn paging_lists_every_chat_the_query_finds_once_in_order(
            sorts in proptest::collection::vec(0i64..4, 0..30),
            limit in 1usize..7,
            query in proptest::option::of("C[0-2]"),
        ) {
            let rows: Vec<Row> = sorts.iter().enumerate().map(|(i, &s)| row(&format!("c{i:02}"), s)).collect();
            let query = query.as_deref();
            let whole = page(rows.clone(), &Rooms::new(), query, None, Limit::new(MAX_LIMIT).unwrap());
            let found = |c: &Chat| query.is_none_or(|q| c.id.contains(&q.to_lowercase()));
            prop_assert_eq!(whole.chats.len(), rows.iter().filter(|r| found(&chat((*r).clone(), &Rooms::new()))).count());
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = page(rows.clone(), &Rooms::new(), query, after.as_ref(), Limit::new(limit).unwrap());
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
