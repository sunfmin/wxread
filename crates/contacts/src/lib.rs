//! What `wxread contacts`, `contact` and `group` answer of the rows they
//! read: each contact named the way WeChat shows it, found by `--query`, a
//! page at a time, and a group with its owner, its notice and every member
//! with their name in it. The effect crate reads the rows out of the
//! contact and head image databases; nothing here reads or writes.

use std::collections::BTreeMap;

use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};

/// The option that finds contacts.
pub const QUERY_FLAG: &str = "--query";

/// What a contact is: a person, a group, or an official account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Person,
    Group,
    OfficialAccount,
}

impl Kind {
    /// The kind of the contact `id`: a group's id is `<n>@chatroom`, an
    /// official account's starts `gh_`.
    pub fn of(id: &str) -> Kind {
        if id.ends_with("@chatroom") {
            Kind::Group
        } else if id.starts_with("gh_") {
            Kind::OfficialAccount
        } else {
            Kind::Person
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Person => "person",
            Kind::Group => "group",
            Kind::OfficialAccount => "official_account",
        }
    }
}

/// The name WeChat shows for a contact: the account's `remark` for them,
/// else their name in the group it is shown in (`in_group`), else their
/// own `name`; "" for none of them. An empty one is none.
pub fn display_name(remark: Option<&str>, in_group: Option<&str>, name: Option<&str>) -> String {
    [remark, in_group, name].into_iter().flatten().find(|s| !s.is_empty()).unwrap_or("").to_string()
}

/// One row of `contact`, and whether `head_image` holds its avatar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    /// `username`: a wxid, a group's `<n>@chatroom`, an official account's
    /// `gh_…`.
    pub id: String,
    /// The WeChat ID they chose (`alias`, 微信号).
    pub alias: Option<String>,
    /// What they call themselves (`nick_name`).
    pub name: Option<String>,
    /// What the account calls them (`remark`).
    pub remark: Option<String>,
    /// Their signature (`description`).
    pub signature: Option<String>,
    pub has_avatar: bool,
}

/// A contact as `wxread contacts` and `contact` answer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub id: String,
    /// As WeChat shows it: [`display_name`] of the remark and the name.
    pub display_name: String,
    /// Their own name; "" for none.
    pub name: String,
    /// None for none, as for an empty one.
    pub remark: Option<String>,
    pub alias: Option<String>,
    pub signature: Option<String>,
    pub kind: Kind,
    pub has_avatar: bool,
}

/// `text`, or None for none or an empty one.
fn given(text: Option<String>) -> Option<String> {
    text.filter(|s| !s.is_empty())
}

/// The contact `row` is.
pub fn contact(row: Row) -> Contact {
    Contact {
        display_name: display_name(row.remark.as_deref(), None, row.name.as_deref()),
        name: row.name.unwrap_or_default(),
        remark: given(row.remark),
        alias: given(row.alias),
        signature: given(row.signature),
        kind: Kind::of(&row.id),
        has_avatar: row.has_avatar,
        id: row.id,
    }
}

/// Whether `row` is found by `query`: its id, WeChat ID, name or remark
/// holds it, whatever their case. Every contact is found by "".
pub fn matches(row: &Row, query: &str) -> bool {
    let query = query.to_lowercase();
    let id = Some(row.id.as_str());
    [id, row.alias.as_deref(), row.name.as_deref(), row.remark.as_deref()]
        .into_iter()
        .flatten()
        .any(|text| text.to_lowercase().contains(&query))
}

/// Where a page of contacts ended: the id of the last it listed. Its text
/// is the `next` an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    id: String,
}

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        if text.is_empty() {
            return Err(format!(
                "{CURSOR_FLAG} \"\" is not a `next` that wxread contacts answered: pass the `next` of the answer before, or leave {CURSOR_FLAG} out to start at the first contact"
            ));
        }
        Ok(Cursor { id: text.to_string() })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        self.id.clone()
    }
}

/// One page of the contacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub contacts: Vec<Contact>,
    /// Where the next page starts, when there are contacts past this one.
    pub next: Option<Cursor>,
}

/// The account's contacts, by id: the rows of `contact` that have one,
/// the first of rows of one id.
pub type Contacts = BTreeMap<String, Row>;

/// `rows` as [`Contacts`].
pub fn by_id(rows: impl IntoIterator<Item = Row>) -> Contacts {
    let mut contacts = Contacts::new();
    for row in rows.into_iter().filter(|r| !r.id.is_empty()) {
        contacts.entry(row.id.clone()).or_insert(row);
    }
    contacts
}

/// The page of `contacts` that `query` finds (every one, for none), by
/// id, starting after `after` (at the first, for none) and holding at most
/// `limit`.
pub fn page(contacts: Contacts, query: Option<&str>, after: Option<&Cursor>, limit: Limit) -> Page {
    let mut found = contacts
        .into_values()
        .filter(|row| after.is_none_or(|c| row.id > c.id) && query.is_none_or(|q| matches(row, q)));
    let contacts: Vec<Contact> = found.by_ref().take(limit.get()).map(contact).collect();
    let more = found.next().is_some();
    let next = if more { contacts.last().map(|c| Cursor { id: c.id.clone() }) } else { None };
    Page { contacts, next }
}

/// What fixes asking for `id`, which is no contact of the account.
pub fn unknown(id: &str) -> String {
    format!(
        "no contact {id} is among the account's contacts: find its id with `wxread contacts {QUERY_FLAG} <its name, remark or WeChat ID>`"
    )
}

/// The contact `id` in `contacts`, or what fixes asking for it.
pub fn find(contacts: &Contacts, id: &str) -> Result<Contact, String> {
    contacts.get(id).cloned().map(contact).ok_or_else(|| unknown(id))
}

/// A group's own record, from `chat_room` and `chatroom_member`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Room {
    /// Its owner's id (`owner`).
    pub owner: Option<String>,
    /// `ext_buffer`: a protobuf of its members, see [`names_in_group`].
    pub ext_buffer: Option<Vec<u8>>,
    /// The id of each member `chatroom_member` lists.
    pub members: Vec<String>,
}

/// A group's notice (群公告), from `chat_room_info_detail`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoticeRow {
    /// What it says (`announcement_`).
    pub text: Option<String>,
    /// Who posted it, by id (`announcement_editor_`).
    pub editor: Option<String>,
    /// When, in seconds since the epoch (`announcement_publish_time_`).
    pub published: Option<i64>,
}

/// Someone a group names (its owner, who posted its notice): their id,
/// and their name as the group shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: String,
    pub display_name: String,
}

/// A group's notice as `wxread group` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    /// Who posted it; None when WeChat does not say.
    pub by: Option<Person>,
    /// When (RFC 3339, UTC); None when WeChat does not say.
    pub at: Option<String>,
}

/// A member of a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    /// As WeChat shows them in the group: [`display_name`] of the
    /// account's remark, their name in the group and their own name.
    pub display_name: String,
    /// Their own name; "" when the account has no contact of them.
    pub name: String,
    /// The name they set for themselves in this group.
    pub name_in_group: Option<String>,
    pub remark: Option<String>,
}

/// A group as `wxread group` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub display_name: String,
    /// Its own name; "" for none.
    pub name: String,
    pub remark: Option<String>,
    pub owner: Option<Person>,
    /// None when it has none.
    pub notice: Option<Notice>,
    /// Every member, by id.
    pub members: Vec<Member>,
}

/// Group `id` of the account, whose contacts are `contacts`, with its
/// `room` and `notice` as WeChat keeps them; or what fixes asking for it:
/// `id` is no contact, or one that is not a group. Its members are those
/// `chatroom_member` lists and those its `ext_buffer` does, each once.
pub fn group(
    id: &str,
    contacts: &Contacts,
    room: Room,
    notice: Option<NoticeRow>,
) -> Result<Group, String> {
    let row = contacts.get(id).ok_or_else(|| unknown(id))?;
    let kind = Kind::of(id);
    if kind != Kind::Group {
        return Err(format!(
            "{id} is a {}, not a group: `wxread contact {id}` answers it, and `wxread contacts {QUERY_FLAG} <name>` finds a group, whose id ends in @chatroom",
            kind.name().replace('_', " ")
        ));
    }
    let mut in_group: BTreeMap<String, Option<String>> =
        room.members.iter().map(|m| (m.clone(), None)).collect();
    for (member, name) in names_in_group(room.ext_buffer.as_deref().unwrap_or_default()) {
        let name = Some(name).filter(|n| !n.is_empty());
        let slot = in_group.entry(member).or_default();
        if name.is_some() {
            *slot = name;
        }
    }
    in_group.remove("");
    let person = |who: &str| Person {
        id: who.to_string(),
        display_name: shown(contacts, who, in_group.get(who).cloned().flatten().as_deref()),
    };
    let owner = given(room.owner).map(|o| person(&o));
    let notice = notice.and_then(|n| {
        Some(Notice {
            text: given(n.text)?,
            by: given(n.editor).map(|e| person(&e)),
            at: n.published.and_then(rfc3339),
        })
    });
    let members = in_group
        .iter()
        .map(|(member, name)| {
            let row = contacts.get(member);
            Member {
                id: member.clone(),
                display_name: shown(contacts, member, name.as_deref()),
                name: row.and_then(|r| r.name.clone()).unwrap_or_default(),
                name_in_group: name.clone(),
                remark: given(row.and_then(|r| r.remark.clone())),
            }
        })
        .collect();
    Ok(Group {
        id: id.to_string(),
        display_name: display_name(row.remark.as_deref(), None, row.name.as_deref()),
        name: row.name.clone().unwrap_or_default(),
        remark: given(row.remark.clone()),
        owner,
        notice,
        members,
    })
}

/// How a group shows `id`, whose name in it is `in_group`.
fn shown(contacts: &Contacts, id: &str, in_group: Option<&str>) -> String {
    let row = contacts.get(id);
    display_name(
        row.and_then(|r| r.remark.as_deref()),
        in_group,
        row.and_then(|r| r.name.as_deref()),
    )
}

/// Each member a group's `ext_buffer` lists, in order, and their name in
/// the group ("" for none set): the protobuf holds a field 1 a member,
/// in which field 1 is their id and field 2 that name. What cannot be
/// read is left out, and a buffer cut short gives the members before it.
pub fn names_in_group(ext_buffer: &[u8]) -> Vec<(String, String)> {
    let text =
        |m: &[u8], n: u64| fields(m, n).first().map(|b| String::from_utf8_lossy(b).into_owned());
    fields(ext_buffer, 1)
        .into_iter()
        .filter_map(|m| Some((text(m, 1)?, text(m, 2).unwrap_or_default())))
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

/// The bytes of every length-delimited field numbered `number` in the
/// protobuf `bytes`, in order: up to the first field that cannot be read.
fn fields(bytes: &[u8], number: u64) -> Vec<&[u8]> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some((tag, value, end)) = field(bytes, at) {
        if let Some(value) = value
            && tag >> 3 == number
        {
            found.push(value);
        }
        at = end;
    }
    found
}

/// The field of `bytes` at `at`: its tag, its bytes when it is
/// length-delimited, and where the next starts; None at the end, and for
/// a field that cannot be read.
fn field(bytes: &[u8], at: usize) -> Option<(u64, Option<&[u8]>, usize)> {
    let (tag, from) = varint(bytes, at)?;
    let (value, end) = match tag & 7 {
        0 => (None, varint(bytes, from)?.1),
        1 => (None, from.checked_add(8)?),
        5 => (None, from.checked_add(4)?),
        2 => {
            let (len, from) = varint(bytes, from)?;
            let end = from.checked_add(usize::try_from(len).ok()?)?;
            (Some(bytes.get(from..end)?), end)
        }
        _ => return None,
    };
    (end <= bytes.len()).then_some((tag, value, end))
}

/// The varint of `bytes` at `at`, and where it ends.
fn varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut n: u64 = 0;
    let mut i = at;
    let mut shift: u32 = 0;
    while shift < 64 {
        let b = *bytes.get(i)?;
        i = i.checked_add(1)?;
        n |= u64::from(b & 0x7f).checked_shl(shift)?;
        if b & 0x80 == 0 {
            return Some((n, i));
        }
        shift = shift.checked_add(7)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use wxread_answer::MAX_LIMIT;

    fn row(id: &str, name: &str, remark: &str) -> Row {
        let text = |s: &str| Some(s.to_string());
        Row { id: id.into(), name: text(name), remark: text(remark), ..Row::default() }
    }

    fn ids(page: &Page) -> Vec<&str> {
        page.contacts.iter().map(|c| c.id.as_str()).collect()
    }

    /// The protobuf field `number` of `bytes`, length-delimited.
    fn len(number: u8, bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![number << 3 | 2, bytes.len() as u8];
        out.extend_from_slice(bytes);
        out
    }

    fn member(id: &str, name: &str) -> Vec<u8> {
        let mut m = len(1, id.as_bytes());
        if !name.is_empty() {
            m.extend(len(2, name.as_bytes()));
        }
        len(1, &m)
    }

    #[test]
    fn a_name_is_the_remark_else_the_name_in_the_group_else_the_own_name() {
        assert_eq!(display_name(Some("Mum"), Some("Mary L."), Some("Mary")), "Mum");
        assert_eq!(display_name(Some(""), Some("Mary L."), Some("Mary")), "Mary L.");
        assert_eq!(display_name(None, Some(""), Some("Mary")), "Mary");
        assert_eq!(display_name(None, None, None), "");
    }

    #[test]
    fn a_kind_is_told_by_the_id() {
        assert_eq!(Kind::of("123@chatroom"), Kind::Group);
        assert_eq!(Kind::of("gh_abc"), Kind::OfficialAccount);
        assert_eq!(Kind::of("wxid_a"), Kind::Person);
        assert_eq!(Kind::OfficialAccount.name(), "official_account");
    }

    #[test]
    fn a_contact_says_what_it_has_and_none_for_what_it_lacks() {
        let mut r = row("wxid_a", "Amy", "");
        r.alias = Some("amy88".into());
        r.signature = Some(String::new());
        r.has_avatar = true;
        let c = contact(r);
        assert_eq!((c.display_name.as_str(), c.name.as_str()), ("Amy", "Amy"));
        assert_eq!((c.remark, c.alias.as_deref(), c.signature), (None, Some("amy88"), None));
        assert_eq!((c.kind, c.has_avatar), (Kind::Person, true));
    }

    #[test]
    fn a_query_finds_by_remark_name_id_and_wechat_id_whatever_the_case() {
        let mut r = row("wxid_amy", "Amy Wong", "Auntie");
        r.alias = Some("amy88".into());
        for query in ["auntie", "WONG", "wxid_am", "Y88", ""] {
            assert!(matches(&r, query), "{query}");
        }
        assert!(!matches(&r, "bob"));
    }

    #[test]
    fn a_page_is_by_id_and_each_contact_is_on_one_page() {
        let rows = vec![row("c", "", ""), row("a", "", ""), row("", "x", ""), row("b", "", "")];
        let two = Limit::new(2).unwrap();
        let first = page(by_id(rows.clone()), None, None, two);
        assert_eq!(ids(&first), ["a", "b"]);
        let next = Cursor::parse(&first.next.unwrap().text()).unwrap();
        let second = page(by_id(rows), None, Some(&next), two);
        assert_eq!(ids(&second), ["c"]);
        assert_eq!(second.next, None);
    }

    #[test]
    fn a_page_holds_only_what_the_query_finds() {
        let rows = vec![row("wxid_a", "Amy", ""), row("wxid_b", "Bob", "Amy's dad")];
        let page = page(by_id(rows), Some("amy"), None, Limit::default_limit());
        assert_eq!(ids(&page), ["wxid_a", "wxid_b"]);
    }

    #[test]
    fn an_empty_cursor_is_refused_with_its_fix() {
        assert!(Cursor::parse("").unwrap_err().contains("leave --cursor out"));
    }

    fn contacts() -> Contacts {
        by_id([
            row("1@chatroom", "Hiking", ""),
            row("wxid_me", "Me", ""),
            row("wxid_amy", "Amy", "Auntie"),
            row("wxid_bob", "Bob", ""),
        ])
    }

    #[test]
    fn a_group_names_its_members_as_it_shows_them() {
        let buffer =
            [member("wxid_amy", "Amy W."), member("wxid_bob", "Bobby"), member("wxid_x", "")]
                .concat();
        let room = Room {
            owner: Some("wxid_bob".into()),
            ext_buffer: Some(buffer),
            members: vec!["wxid_me".into(), "wxid_bob".into()],
        };
        let notice = NoticeRow {
            text: Some("Sunday 8am".into()),
            editor: Some("wxid_bob".into()),
            published: Some(1_760_000_000),
        };
        let g = group("1@chatroom", &contacts(), room, Some(notice)).unwrap();
        assert_eq!(
            (g.display_name.as_str(), g.name.as_str(), g.remark),
            ("Hiking", "Hiking", None)
        );
        let bobby = Person { id: "wxid_bob".into(), display_name: "Bobby".into() };
        assert_eq!(g.owner, Some(bobby.clone()));
        assert_eq!(
            g.notice,
            Some(Notice {
                text: "Sunday 8am".into(),
                by: Some(bobby),
                at: Some("2025-10-09T08:53:20Z".into())
            })
        );
        let shown: Vec<(&str, &str, Option<&str>)> = g
            .members
            .iter()
            .map(|m| (m.id.as_str(), m.display_name.as_str(), m.name_in_group.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [
                ("wxid_amy", "Auntie", Some("Amy W.")),
                ("wxid_bob", "Bobby", Some("Bobby")),
                ("wxid_me", "Me", None),
                ("wxid_x", "", None),
            ]
        );
        assert_eq!(g.members[0].remark.as_deref(), Some("Auntie"));
        assert_eq!(g.members[3].name, "");
    }

    #[test]
    fn a_group_with_no_record_or_notice_has_none() {
        let empty = NoticeRow { text: Some(String::new()), ..NoticeRow::default() };
        let g = group("1@chatroom", &contacts(), Room::default(), Some(empty)).unwrap();
        assert_eq!((g.owner, g.notice, g.members), (None, None, vec![]));
    }

    #[test]
    fn an_unknown_id_or_one_not_a_group_is_refused_with_its_fix() {
        let why = group("2@chatroom", &contacts(), Room::default(), None).unwrap_err();
        assert!(why.contains("no contact 2@chatroom") && why.contains("wxread contacts --query"));
        let why = find(&contacts(), "wxid_nobody").unwrap_err();
        assert!(why.contains("no contact wxid_nobody") && why.contains("wxread contacts --query"));
        let why = group("wxid_amy", &contacts(), Room::default(), None).unwrap_err();
        assert!(
            why.contains("is a person, not a group") && why.contains("`wxread contact wxid_amy`")
        );
    }

    #[test]
    fn members_are_read_past_other_fields_and_up_to_a_cut() {
        let mut buffer = member("wxid_a", "A");
        buffer.extend([0x18, 0x09]);
        buffer.extend(member("wxid_b", ""));
        buffer.extend(len(4, b"x"));
        let all =
            vec![("wxid_a".to_string(), "A".to_string()), ("wxid_b".to_string(), String::new())];
        assert_eq!(names_in_group(&buffer), all);
        assert_eq!(names_in_group(&buffer[..buffer.len() - 1]), all);
        assert_eq!(names_in_group(&[0xff; 12]), vec![]);
    }

    proptest! {
        #[test]
        fn paging_lists_every_contact_found_once_in_order(
            names in proptest::collection::vec("[ab]{0,2}", 0..30),
            limit in 1usize..7,
        ) {
            let rows: Vec<Row> = names.iter().enumerate().map(|(i, n)| row(&format!("c{i:02}"), n, "")).collect();
            let whole = page(by_id(rows.clone()), Some("a"), None, Limit::new(MAX_LIMIT).unwrap());
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = page(by_id(rows.clone()), Some("a"), after.as_ref(), Limit::new(limit).unwrap());
                prop_assert!(p.contacts.len() <= limit);
                paged.extend(p.contacts);
                match p.next {
                    Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                    None => break,
                }
            }
            prop_assert!(paged.iter().all(|c| c.name.contains('a')));
            prop_assert_eq!(paged, whole.contacts);
        }

        #[test]
        fn any_bytes_are_read_without_a_panic(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            names_in_group(&bytes);
        }
    }
}
