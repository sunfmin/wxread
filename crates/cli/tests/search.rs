//! `wxread search`: messages found across every chat, through WeChat's own
//! full-text index for what it has indexed and message by message for the
//! rest, a page at a time.

use serde_json::{Value, json};
use wxread_schema::{CONTACT_DB, MESSAGE_FTS_DB};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Group, Indexed, Said, Stale};

const GROUP: &str = "123@chatroom";
const T: i64 = 1_760_000_000;

/// An app message's `local_type` of subtype `sub`.
fn app(sub: i64) -> i64 {
    49 | sub << 32
}

/// A message `n` of `chat` in database `db`, said by `sender` at `T + n`,
/// of `local_type` with `content` (a group's prefixed with its sender, as
/// WeChat keeps it), and the text WeChat's index holds of it.
fn said(
    db: &'static str,
    chat: &'static str,
    n: i64,
    sender: &'static str,
    local_type: i64,
    content: &str,
    indexed: Option<&str>,
) -> Said {
    let mut said = Said::text(chat, 9000 + n, T + n, sender, content);
    said.db = db;
    said.local_type = local_type;
    if chat == GROUP {
        said.content = format!("{sender}:\n{content}");
    }
    said.indexed = indexed.map(String::from);
    said
}

/// An account whose chats say "crag" in a text, a link's title and
/// summary, a file's name and a notice, some of it in what WeChat's index
/// covers and some not (the WAL, a database it has no range of), with an
/// index entry whose message is gone; with the index when `indexed`.
fn account(world: &World, indexed: bool) -> Account {
    let account = unwritten(world, indexed);
    account.write();
    account
}

/// [`account`], its databases not yet written.
fn unwritten(world: &World, indexed: bool) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", "Amy W"),
        Contact("wxid_bob", "Bob", ""),
        Contact(GROUP, "Hiking", ""),
    ];
    account.groups = vec![Group {
        id: GROUP,
        owner: "wxid_bob",
        members: vec![("wxid_bob", "Bobby")],
        notice: None,
    }];
    let link = |title: &str, des: &str| {
        format!(
            "<msg><appmsg><title>{title}</title><des>{des}</des><type>5</type><url>https://e.com/{title}</url></appmsg></msg>"
        )
    };
    let file = |name: &str| {
        format!(
            "<msg><appmsg><title>{name}</title><type>6</type><appattach><totallen>9</totallen><fileext>pdf</fileext></appattach></appmsg></msg>"
        )
    };
    account.messages = vec![
        said(
            "message_0",
            "wxid_amy",
            1,
            "wxid_amy",
            1,
            "Saturday at the crag?",
            Some("Saturday at the crag?"),
        ),
        said(
            "message_0",
            "wxid_amy",
            2,
            "wxid_me",
            app(5),
            &link("Crag topo", "for Saturday"),
            Some("Crag topo for Saturday"),
        ),
        said(
            "message_0",
            "wxid_amy",
            3,
            "wxid_amy",
            app(6),
            &file("crag-map.pdf"),
            Some("crag-map.pdf"),
        ),
        said("message_0", "wxid_amy", 4, "wxid_amy", 3, r#"<msg><img md5="crag"/></msg>"#, None),
        said("message_0", GROUP, 5, "wxid_bob", 1, "crag closed", Some("crag closed")),
        said("message_1", "wxid_bob", 8, "wxid_bob", 1, "CRAG again", Some("CRAG again")),
    ];
    let mut notice = said("message_0", GROUP, 9, "wxid_bob", 10000, "Bobby named it Crag", None);
    notice.sender = None;
    notice.content = "Bobby named it Crag".into();
    account.recent_messages = vec![
        said("message_0", GROUP, 6, "wxid_bob", app(5), &link("Rope guide", "for the CRAG"), None),
        said("message_0", GROUP, 7, "wxid_bob", app(6), &file("Crag-rules.pdf"), None),
        notice,
    ];
    if indexed {
        // Amy's chat and the group in message_0, up to what the WAL holds;
        // message_1 has no range: Bob's chat there is read message by
        // message.
        account.index = Some(vec![
            Indexed { chat: "wxid_amy", db: "message_0", first: 1, last: 99 },
            Indexed { chat: GROUP, db: "message_0", first: 1, last: 1 },
        ]);
        account.stale = vec![Stale("wxid_amy", 50, "crag, deleted since", T + 50)];
    }
    account
}

fn ids(json: &Value) -> Vec<String> {
    let found = json["messages"].as_array().unwrap_or_else(|| panic!("{json}"));
    found.iter().map(|m| m["id"].as_str().unwrap().to_string()).collect()
}

fn search(world: &World, account: &Account, args: &[&str]) -> Value {
    let mut all = vec!["search"];
    all.extend_from_slice(args);
    world.run(&all, &[("WXREAD_KEYS", &account.all_keys())]).ok()
}

/// Every message "crag" finds, newest first.
const CRAG: [&str; 7] = ["9008", "9007", "9006", "9005", "9003", "9002", "9001"];

#[test]
fn a_text_a_link_title_and_a_file_name_are_found_across_chats_through_wechats_index() {
    let world = World::new();
    let account = account(&world, true);
    let json = search(&world, &account, &["crag"]);
    assert_eq!(
        (&json["account"], &json["query"], &json["searched"], &json["next"]),
        (&json!("wxid_me_1a2b"), &json!("crag"), &json!("wechat_index"), &Value::Null)
    );
    assert_eq!(ids(&json), CRAG, "{json}");
    let by_id = |id: &str| {
        json["messages"].as_array().unwrap().iter().find(|m| m["id"] == id).unwrap().clone()
    };
    // From the index: what WeChat indexed of it is what matched.
    let topo = by_id("9002");
    assert_eq!(
        (&topo["chat"], &topo["sender"], &topo["kind"]),
        (
            &json!({"id": "wxid_amy", "display_name": "Amy W"}),
            &json!({"id": "wxid_me", "display_name": "Me"}),
            &json!("link")
        )
    );
    assert_eq!(topo["matched"], "Crag topo for Saturday");
    assert_eq!(topo["link"]["title"], "Crag topo");
    assert_eq!(topo["at"], "2025-10-09T08:53:22Z");
    // Read message by message: the text of its kind is what matched.
    let rope = by_id("9006");
    assert_eq!(
        (&rope["chat"], &rope["sender"]["display_name"], &rope["matched"]),
        (
            &json!({"id": GROUP, "display_name": "Hiking"}),
            &json!("Bobby"),
            &json!("Rope guide\nfor the CRAG")
        )
    );
    assert_eq!(
        (&by_id("9007")["kind"], &by_id("9007")["matched"]),
        (&json!("file"), &json!("Crag-rules.pdf"))
    );
    assert_eq!(by_id("9005")["content"], "crag closed");
    assert_eq!(by_id("9008")["matched"], "CRAG again");
    // A word, a link's title and a file's name, each alone.
    assert_eq!(ids(&search(&world, &account, &["saturday"])), ["9002", "9001"]);
    assert_eq!(ids(&search(&world, &account, &["topo"])), ["9002"]);
    assert_eq!(ids(&search(&world, &account, &["MAP.pdf"])), ["9003"]);
    assert_eq!(ids(&search(&world, &account, &["rules"])), ["9007"]);
}

#[test]
fn without_wechats_index_every_message_is_read_and_the_same_are_found() {
    let world = World::new();
    let account = account(&world, false);
    let json = search(&world, &account, &["crag"]);
    assert_eq!(json["searched"], "messages");
    assert_eq!(ids(&json), CRAG, "{json}");
    let topo = json["messages"].as_array().unwrap().iter().find(|m| m["id"] == "9002").unwrap();
    assert_eq!(topo["matched"], "Crag topo\nfor Saturday");
    assert_eq!(ids(&search(&world, &account, &["map.pdf"])), ["9003"]);
}

#[test]
fn a_channels_feed_is_found_by_its_description_when_wechat_has_not_indexed_it() {
    // WeChat's appmsg title of a feed is its placeholder for old clients;
    // the feed's text is only in its <finderFeed><desc>.
    const PLACEHOLDER: &str = "当前微信版本不支持展示该内容，请升级至最新版本。";
    let feed = format!(
        "<msg><appmsg><title>{PLACEHOLDER}</title><des>{PLACEHOLDER}</des><type>51</type><finderFeed><nickname>Crag TV</nickname><desc>周末抱石 at dawn</desc></finderFeed></appmsg></msg>"
    );
    for indexed in [false, true] {
        let world = World::new();
        let mut account = unwritten(&world, indexed);
        account.recent_messages.push(said(
            "message_0",
            GROUP,
            10,
            "wxid_bob",
            app(51),
            &feed,
            None,
        ));
        account.write();
        let json = search(&world, &account, &["周末"]);
        assert_eq!(ids(&json), ["9010"], "indexed {indexed}: {json}");
        let found = &json["messages"][0];
        assert_eq!(found["matched"], "周末抱石 at dawn\nCrag TV");
        assert_eq!(
            (&found["link"]["to"], &found["link"]["title"], &found["link"]["summary"]),
            (&json!("channels_feed"), &json!("周末抱石 at dawn"), &json!("Crag TV"))
        );
        assert_eq!(ids(&search(&world, &account, &["crag tv"])), ["9010"]);
    }
}

#[test]
fn chat_from_and_to_narrow_it() {
    let world = World::new();
    let account = account(&world, true);
    let run = |args: &[&str]| ids(&search(&world, &account, args));
    assert_eq!(run(&["crag", "--chat", GROUP]), ["9007", "9006", "9005"]);
    assert_eq!(run(&["crag", "--chat", "wxid_bob"]), ["9008"]);
    assert_eq!(
        run(&["crag", "--from", "2025-10-09T08:53:23Z"]),
        ["9008", "9007", "9006", "9005", "9003"]
    );
    assert_eq!(run(&["crag", "--to", "2025-10-09T08:53:23Z"]), ["9002", "9001"]);
    assert_eq!(
        run(&[
            "crag",
            "--chat",
            "wxid_amy",
            "--from",
            "2025-10-09T08:53:22Z",
            "--to",
            "2025-10-09T08:53:24Z"
        ]),
        ["9003", "9002"]
    );
}

#[test]
fn next_pages_without_repeats() {
    let world = World::new();
    let account = account(&world, true);
    for chat in [None, Some(GROUP)] {
        let whole = {
            let mut args = vec!["crag"];
            args.extend(chat.map(|c| ["--chat", c]).into_iter().flatten());
            ids(&search(&world, &account, &args))
        };
        for limit in ["1", "2", "3"] {
            let mut paged = Vec::new();
            let mut cursor: Option<String> = None;
            loop {
                let mut args = vec!["crag", "--limit", limit];
                args.extend(chat.map(|c| ["--chat", c]).into_iter().flatten());
                if let Some(c) = &cursor {
                    args.extend(["--cursor", c.as_str()]);
                }
                let json = search(&world, &account, &args);
                let page = ids(&json);
                assert!(page.len() <= limit.parse().unwrap(), "{json}");
                paged.extend(page);
                match json["next"].as_str() {
                    Some(next) => cursor = Some(next.to_string()),
                    None => break,
                }
            }
            assert_eq!(paged, whole, "--limit {limit}, --chat {chat:?}");
        }
    }
}

#[test]
fn a_query_matching_nothing_answers_an_empty_list() {
    for indexed in [true, false] {
        let world = World::new();
        let account = account(&world, indexed);
        let json = search(&world, &account, &["no such words"]);
        assert_eq!((&json["messages"], &json["next"]), (&json!([]), &Value::Null), "{json}");
    }
}

#[test]
fn no_decrypted_byte_is_written() {
    let world = World::new();
    let account = account(&world, true);
    let keys = account.all_keys();
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let before = snapshot(&account.dir());
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    world.run(&["search", "crag"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}

#[test]
fn what_is_not_a_search_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world, true);
    let keys = account.all_keys();
    let refused = |args: &[&str]| world.run(args, &[("WXREAD_KEYS", &keys)]).error();
    for args in [&["search"][..], &["search", "  "]] {
        let error = refused(args);
        assert!(error.contains("`wxread search <text>`"), "{error}");
    }
    let error = refused(&["search", "crag", "--chat", "wxid_nobody"]);
    assert!(
        error.contains("no messages of chat wxid_nobody") && error.contains("`wxread chats`"),
        "{error}"
    );
    assert!(refused(&["search", "crag", "--cursor", "nope"]).contains("leave --cursor out"));
    assert!(
        refused(&["search", "crag", "--limit", "0"]).contains("number of messages from 1 to 1000")
    );
    assert!(
        refused(&["search", "crag", "--from", "yesterday"])
            .contains("--from yesterday is not a time")
    );
    // The keys must open the index when the account has one.
    let without = account.keys(&[CONTACT_DB, "message/message_0.db", "message/message_1.db"]);
    let error = world.run(&["search", "crag"], &[("WXREAD_KEYS", &without)]).error();
    assert!(
        error.contains(&format!("no key of {MESSAGE_FTS_DB}"))
            && error.contains("sudo wxread keys"),
        "{error}"
    );
}
