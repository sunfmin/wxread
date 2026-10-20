//! The search, apart from any database: what each kind is searched by,
//! which messages WeChat's index answers for, and the paging across chats.

use proptest::prelude::*;

use super::*;

fn row(db: &str, local_id: i64, create_time: i64, local_type: i64, content: &str) -> Row {
    Row {
        db: db.into(),
        local_id,
        server_id: local_id + 1000,
        local_type,
        sort_seq: create_time * 1000,
        create_time,
        sender: Some("wxid_amy".into()),
        content: content.as_bytes().to_vec(),
        compressed: None,
        packed: Vec::new(),
    }
}

fn query(text: &str) -> Query {
    Query::parse(text).unwrap()
}

fn matched(local_type: i64, content: &str) -> Option<String> {
    found(&query("x"), "wxid_me", "wxid_amy", row("message_0", 1, 10, local_type, content))
        .map(|f| f.matched)
}

#[test]
fn a_query_finds_its_text_whatever_the_case() {
    let q = query("  Topo ");
    assert!(q.finds("the route TOPO is here"));
    assert!(!q.finds("top o"));
    assert!(query("攀岩").finds("周六去攀岩吗"));
    for blank in ["", "  "] {
        let why = Query::parse(blank).unwrap_err();
        assert!(why.contains("`wxread search <text>`"), "{why}");
    }
}

#[test]
fn each_kind_is_searched_by_the_text_wechat_indexes() {
    let app = |sub: i64| 49 | sub << 32;
    assert_eq!(matched(1, "x marks"), Some("x marks".into()));
    assert_eq!(
        matched(
            app(5),
            "<msg><appmsg><title>Route x</title><des>North</des><type>5</type></appmsg></msg>"
        ),
        Some("Route x\nNorth".into())
    );
    assert_eq!(
        matched(app(6), "<msg><appmsg><title>x.pdf</title><type>6</type></appmsg></msg>"),
        Some("x.pdf".into())
    );
    assert_eq!(
        matched(48, r#"<msg><location x="1" y="2" label="1 x Rd" poiname="Gym"/></msg>"#),
        Some("Gym\n1 x Rd".into())
    );
    assert_eq!(matched(42, r#"<msg username="wxid_x" nickname="Rex"/>"#), Some("Rex".into()));
    let record = r#"<recordinfo><datalist><dataitem datatype="8" dataid="b"><datafmt>.htm</datafmt></dataitem><dataitem datatype="1" dataid="p"><datadesc>Meet x</datadesc></dataitem><dataitem datatype="6" dataid="l"><locitem><poiname>Gate</poiname><label>1 x Rd</label></locitem></dataitem><dataitem datatype="8" dataid="f"><datatitle>x.gpx</datatitle></dataitem><dataitem datatype="2" dataid="i"></dataitem></datalist></recordinfo>"#;
    let note = format!(
        "<msg><appmsg><type>24</type><recorditem><![CDATA[{record}]]></recorditem></appmsg></msg>"
    );
    assert_eq!(matched(app(24), &note), Some("Meet x\nGate\n1 x Rd\nx.gpx".into()));
    // Media and notices are not searched, whatever their XML holds.
    assert_eq!(matched(3, r#"<msg><img md5="x"/></msg>"#), None);
    assert_eq!(matched(10000, "x joined"), None);
    let call = r#"<voipmsg type="VoIPBubbleMsg"><VoIPBubbleMsg><msg><![CDATA[x 已取消]]></msg><room_type>1</room_type></VoIPBubbleMsg></voipmsg>"#;
    assert_eq!(matched(50, call), None);
    // A text that does not hold the query is not found.
    assert_eq!(matched(1, "nothing"), None);
}

fn sessions() -> BTreeMap<i64, String> {
    [(1, "wxid_amy"), (2, "9@chatroom"), (3, "wxid_bob")].map(|(n, id)| (n, id.to_string())).into()
}

fn dbs() -> Vec<Db> {
    let db = |name: &str, stamp: Option<i64>, chats: &[&str]| Db {
        name: name.into(),
        stamp,
        chats: chats.iter().map(|c| c.to_string()).collect(),
    };
    vec![
        db("message_0", Some(100), &["wxid_amy", "9@chatroom"]),
        db("message_1", Some(200), &["wxid_amy"]),
        db("biz_message_0", Some(100), &["9@chatroom"]),
        db("message_2", None, &["wxid_bob"]),
    ]
}

#[test]
fn a_range_covers_the_one_database_of_its_stamp_that_holds_its_chat() {
    let ranges = [
        Indexed { session: 1, stamp: 100, first: 1, last: 5 },
        Indexed { session: 1, stamp: 200, first: 3, last: 9 },
        // Two databases of stamp 100 hold the group: neither is covered.
        Indexed { session: 2, stamp: 100, first: 1, last: 5 },
        // A database with no stamp, a session no chat, an empty range.
        Indexed { session: 3, stamp: 300, first: 1, last: 5 },
        Indexed { session: 9, stamp: 100, first: 1, last: 5 },
        Indexed { session: 1, stamp: 999, first: 5, last: 1 },
    ];
    let index = Index::new(sessions(), &ranges, &dbs());
    assert_eq!(index.covered("wxid_amy", "message_0"), Some((1, 5)));
    assert_eq!(index.covered("wxid_amy", "message_1"), Some((3, 9)));
    assert_eq!(index.covered("9@chatroom", "message_0"), None);
    assert_eq!(index.covered("9@chatroom", "biz_message_0"), None);
    assert_eq!(index.covered("wxid_bob", "message_2"), None);
    assert_eq!(Index::default().covered("wxid_amy", "message_0"), None);
}

fn entry(text: &str, session: i64, local_id: i64, create_time: i64) -> Entry {
    Entry { text: text.into(), session, local_id, sort_seq: create_time * 1000, create_time }
}

#[test]
fn the_index_answers_what_it_covers_newest_first() {
    let ranges = [
        Indexed { session: 1, stamp: 100, first: 1, last: 5 },
        Indexed { session: 1, stamp: 200, first: 3, last: 9 },
    ];
    let index = Index::new(sessions(), &ranges, &dbs());
    let entries = vec![
        entry("rope x", 1, 2, 10),
        entry("x again", 1, 4, 30),
        entry("nothing", 1, 3, 20),
        // Past the range: read with the rows the index does not hold.
        entry("x late", 1, 12, 40),
        // A session no chat is.
        entry("x", 7, 1, 50),
    ];
    let hits = index.hits(&query("X"), entries.clone(), None, Range::default(), None);
    let found: Vec<(&str, &str, i64)> =
        hits.iter().map(|h| (h.chat.as_str(), h.db.as_str(), h.local_id)).collect();
    // local_id 4 is in both ranges: it may be in either, and the row read
    // in each tells.
    assert_eq!(
        found,
        [("wxid_amy", "message_1", 4), ("wxid_amy", "message_0", 4), ("wxid_amy", "message_0", 2)]
    );
    assert_eq!(hits[0].matched, "x again");
    let mut there = row("message_1", 4, 30, 1, "x again");
    assert!(hits[0].is(&there));
    there.sort_seq += 1;
    assert!(!hits[0].is(&there));
    let range = Range { from: Some(5), to: Some(20) };
    assert_eq!(index.hits(&query("x"), entries.clone(), None, range, None).len(), 1);
    assert!(index.hits(&query("x"), entries, Some("wxid_bob"), Range::default(), None).is_empty());
}

#[test]
fn a_cursor_is_written_and_read_back_and_bounds_each_table() {
    let c = Cursor::parse("10.-3.message_0.4.wxid.a.b").unwrap();
    assert_eq!(c.text(), "10.-3.message_0.4.wxid.a.b");
    assert_eq!(c.below_in("wxid.a.b", "message_0"), (10, -3, 4));
    assert_eq!(c.below_in("wxid.a.b", "message_1"), (10, -3, i64::MIN));
    assert_eq!(c.below_in("wxid.a.a", "message_9"), (10, -3, i64::MAX));
    assert_eq!(c.below_in("wxid.a.c", "message_0"), (10, -3, i64::MIN));
    for bad in ["", "10.3.message_0.4", "10.3..4.c", "10.3.m.4.", "x.3.m.4.c"] {
        let why = Cursor::parse(bad).unwrap_err();
        assert!(why.contains("leave --cursor out"), "{why}");
    }
}

fn found_at(chat: &str, db: &str, local_id: i64, create_time: i64) -> Found {
    Found { chat: chat.into(), row: row(db, local_id, create_time, 1, "x"), matched: "x".into() }
}

fn all() -> Vec<Found> {
    vec![
        found_at("wxid_amy", "message_0", 1, 10),
        found_at("wxid_bob", "message_0", 1, 10),
        found_at("wxid_amy", "message_1", 1, 10),
        found_at("9@chatroom", "message_0", 7, 30),
        found_at("wxid_amy", "message_0", 2, 20),
        found_at("wxid_amy", "message_0", 2, 20),
    ]
}

fn place(f: &Found) -> (String, String, i64) {
    (f.chat.clone(), f.row.db.clone(), f.row.local_id)
}

#[test]
fn a_page_is_newest_first_across_chats_each_once() {
    let whole = page(all(), Range::default(), None, Limit::default_limit());
    let places: Vec<_> = whole.found.iter().map(place).collect();
    let p = |c: &str, d: &str, l: i64| (c.to_string(), d.to_string(), l);
    assert_eq!(
        places,
        [
            p("9@chatroom", "message_0", 7),
            p("wxid_amy", "message_0", 2),
            p("wxid_bob", "message_0", 1),
            p("wxid_amy", "message_1", 1),
            p("wxid_amy", "message_0", 1),
        ]
    );
    assert_eq!(whole.next, None);
    let range = Range { from: Some(15), to: Some(30) };
    assert_eq!(page(all(), range, None, Limit::default_limit()).found.len(), 1);
}

#[test]
fn an_answer_is_the_message_with_its_chat_and_what_matched() {
    let f = found_at("wxid_amy", "message_0", 1, 1_760_000_000);
    let mut names = Names::default();
    names.contacts.insert(
        "wxid_amy".into(),
        wxread_messages::Contact { remark: Some("Amy W".into()), nick_name: None },
    );
    let value = answer(&f, "wxid_me", &names, Some("Amy W"), &none_held);
    assert_eq!(
        value,
        json!({
            "chat": "wxid_amy", "chat_name": "Amy W", "matched": "x",
            "id": "1001", "at": "2025-10-09T08:53:20Z", "sender": "wxid_amy", "sender_name": "Amy W",
            "kind": "text", "text": {"text": "x"}, "type": 1, "subtype": 0, "content": "x",
        })
    );
    assert_eq!(answer(&f, "wxid_me", &names, Some(""), &none_held)["chat_name"], Value::Null);
}

proptest! {
    #[test]
    fn paging_returns_each_message_once(limit in 1usize..7, picks in prop::collection::vec((0usize..3, 0usize..2, 1i64..4, 1i64..4), 0..24)) {
        let chats = ["wxid_amy", "wxid_bob", "9@chatroom"];
        let dbs = ["message_0", "message_1"];
        let found: Vec<Found> = picks
            .iter()
            .map(|&(c, d, l, t)| found_at(chats[c], dbs[d], l, t))
            .collect();
        let limit = Limit::new(limit).unwrap();
        let whole = page(found.clone(), Range::default(), None, Limit::new(MAX).unwrap());
        let mut paged = Vec::new();
        let mut after: Option<Cursor> = None;
        loop {
            let p = page(found.clone(), Range::default(), after.as_ref(), limit);
            prop_assert!(p.found.len() <= limit.get());
            paged.extend(p.found.iter().map(place));
            match p.next {
                Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                None => break,
            }
        }
        prop_assert_eq!(paged, whole.found.iter().map(place).collect::<Vec<_>>());
    }
}

const MAX: usize = wxread_answer::MAX_LIMIT;

/// WeChat holds no media message's file or thumbnail.
fn none_held(_: &Media, _: &Row) -> Held {
    Held { file: false, thumbnail: None }
}
