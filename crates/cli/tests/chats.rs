//! `wxread chats`: the account's chats, read from its encrypted databases
//! and their WAL, a page at a time, with nothing decrypted written.

use serde_json::{Value, json};
use wxread_schema::{CONTACT_DB, SESSION_DB};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, DATABASES, Group, Session};

/// An account with a friend, a group and a chat with someone it has no
/// contact of; one more chat only in its WAL, and one in frames no commit
/// ended.
fn account(world: &World, id: &'static str) -> Account {
    let mut account = Account::new(&world.data_dir(), id);
    account.contacts = vec![
        Contact("wxid_amy", "Amy", "Amy W"),
        Contact("wxid_bob", "Bob", ""),
        Contact("123@chatroom", "Hiking", ""),
    ];
    account.sessions = vec![
        Session("wxid_amy", 1_760_000_300, 2),
        Session("123@chatroom", 1_760_000_200, 0),
        Session("wxid_stranger", 1_760_000_100, 0),
    ];
    account.recent = vec![Session("wxid_bob", 1_760_000_400, 1)];
    account.uncommitted = vec![Session("wxid_carl", 1_760_000_500, 9)];
    account.write();
    account
}

fn ids(json: &Value) -> Vec<String> {
    json["chats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn it_lists_every_chat_newest_first_the_wals_included() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = world.file("keys.json", &account.keys(&DATABASES));
    let json = world.run(&["chats", "--keys", keys.to_str().unwrap()], &[]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "chats": [
                {"id": "wxid_bob", "name": "Bob", "kind": "direct", "read_with": "wxread messages --chat wxid_bob", "last_message_at": "2025-10-09T09:00:00Z", "unread": 1, "members": 2},
                {"id": "wxid_amy", "name": "Amy W", "kind": "direct", "read_with": "wxread messages --chat wxid_amy", "last_message_at": "2025-10-09T08:58:20Z", "unread": 2, "members": 2},
                {"id": "123@chatroom", "name": "Hiking", "kind": "group", "read_with": "wxread messages --chat 123@chatroom", "last_message_at": "2025-10-09T08:56:40Z", "unread": 0, "members": null},
                {"id": "wxid_stranger", "name": "", "kind": "direct", "read_with": "wxread messages --chat wxid_stranger", "last_message_at": "2025-10-09T08:55:00Z", "unread": 0, "members": 2},
            ],
            "next": null,
        })
    );
}

/// A group of `members`, the account among them, none named in it.
fn group(id: &'static str, members: &[&'static str]) -> Group {
    Group {
        id,
        owner: "wxid_me",
        members: members.iter().map(|m| (*m, "")).collect(),
        notice: None,
    }
}

/// The account, Amy and Bob: a chat with Amy; a big group of all three
/// and a stranger, whose chat is in the WAL; a small one of the three with
/// no chat; and one of the account and Amy alone.
fn with_groups(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", "Amy W"),
        Contact("wxid_bob", "Bob", ""),
        Contact("1@chatroom", "Big", ""),
        Contact("2@chatroom", "Small", ""),
        Contact("3@chatroom", "Amy's", ""),
    ];
    account.groups = vec![
        group("1@chatroom", &["wxid_me", "wxid_amy", "wxid_bob", "wxid_stranger"]),
        group("2@chatroom", &["wxid_me", "wxid_amy", "wxid_bob"]),
        group("3@chatroom", &["wxid_me", "wxid_amy"]),
    ];
    account.sessions =
        vec![Session("wxid_amy", 1_760_000_300, 2), Session("3@chatroom", 1_760_000_200, 0)];
    account.recent = vec![Session("1@chatroom", 1_760_000_400, 5)];
    account.write();
    account
}

#[test]
fn with_two_contacts_it_answers_only_the_groups_holding_both_smallest_first() {
    let world = World::new();
    let account = with_groups(&world);
    let keys = account.keys(&DATABASES);
    let args = ["chats", "--with", "wxid_amy", "--with", "wxid_bob"];
    let json = world.run(&args, &[("WXREAD_KEYS", &keys)]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "chats": [
                {"id": "2@chatroom", "name": "Small", "kind": "group", "read_with": "wxread messages --chat 2@chatroom", "last_message_at": null, "unread": 0, "members": 3},
                {"id": "1@chatroom", "name": "Big", "kind": "group", "read_with": "wxread messages --chat 1@chatroom", "last_message_at": "2025-10-09T09:00:00Z", "unread": 5, "members": 4},
            ],
            "next": null,
        })
    );
}

#[test]
fn with_one_contact_it_answers_the_direct_chat_and_their_groups() {
    let world = World::new();
    let account = with_groups(&world);
    let keys = account.keys(&DATABASES);
    let json = world.run(&["chats", "--with", "wxid_amy"], &[("WXREAD_KEYS", &keys)]).ok();
    assert_eq!(ids(&json), ["wxid_amy", "3@chatroom", "2@chatroom", "1@chatroom"]);
    assert_eq!(json["chats"][0]["members"], json!(2));
    let mut paged = Vec::new();
    let mut args = vec!["chats", "--with", "wxid_amy", "--limit", "1"];
    let mut cursor;
    loop {
        let json = world.run(&args, &[("WXREAD_KEYS", &keys)]).ok();
        paged.extend(ids(&json));
        match json["next"].as_str() {
            Some(next) => {
                cursor = next.to_string();
                args = vec!["chats", "--with", "wxid_amy", "--limit", "1", "--cursor", &cursor];
            }
            None => break,
        }
    }
    assert_eq!(paged, ["wxid_amy", "3@chatroom", "2@chatroom", "1@chatroom"]);
}

#[test]
fn with_an_id_that_is_no_contact_it_names_the_fix() {
    let world = World::new();
    let account = with_groups(&world);
    let keys = account.keys(&DATABASES);
    let error = world.run(&["chats", "--with", "wxid_nobody"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("no contact wxid_nobody"), "{error}");
    assert!(error.contains("wxread contacts --query"), "{error}");
    let error = world.run(&["chats", "--with", "1@chatroom"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("`wxread group 1@chatroom`"), "{error}");
}

/// An account whose chat list holds WeChat's three folders beside a chat:
/// none of them a contact, as on a real account.
fn with_folders(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy", "")];
    account.sessions = vec![
        Session("@placeholder_foldgroup", 1_760_000_400, 0),
        Session("brandsessionholder", 1_760_000_300, 3),
        Session("brandservicesessionholder", 1_760_000_200, 0),
        Session("wxid_amy", 1_760_000_100, 0),
    ];
    account.write();
    account
}

#[test]
fn a_folder_is_listed_as_a_folder_named_as_wechat_names_it_with_what_reads_it() {
    let world = World::new();
    let account = with_folders(&world);
    let json = world.run(&["chats"], &[("WXREAD_KEYS", &account.keys(&DATABASES))]).ok();
    assert_eq!(
        json["chats"],
        json!([
            {"id": "@placeholder_foldgroup", "name": "折叠的群聊", "kind": "folder", "read_with": "wxread chats", "last_message_at": "2025-10-09T09:00:00Z", "unread": 0, "members": null},
            {"id": "brandsessionholder", "name": "订阅号消息", "kind": "folder", "read_with": "wxread articles", "last_message_at": "2025-10-09T08:58:20Z", "unread": 3, "members": null},
            {"id": "brandservicesessionholder", "name": "服务号", "kind": "folder", "read_with": "wxread articles", "last_message_at": "2025-10-09T08:56:40Z", "unread": 0, "members": null},
            {"id": "wxid_amy", "name": "Amy", "kind": "direct", "read_with": "wxread messages --chat wxid_amy", "last_message_at": "2025-10-09T08:55:00Z", "unread": 0, "members": 2},
        ])
    );
    let found = world
        .run(&["chats", "--query", "订阅号"], &[("WXREAD_KEYS", &account.keys(&DATABASES))])
        .ok();
    assert_eq!(ids(&found), ["brandsessionholder"]);
}

#[test]
fn with_hundreds_of_groups_it_answers_well_under_a_second() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let people: Vec<&'static str> = (0..300).map(|n| leak(format!("wxid_p{n}"))).collect();
    account.contacts = people.iter().map(|p| Contact(p, "P", "")).collect();
    for g in 0..600 {
        let id = leak(format!("{g}@chatroom"));
        account.contacts.push(Contact(id, "G", ""));
        let members: Vec<&'static str> =
            (0..40).map(|m| people[(g * 7 + m) % people.len()]).collect();
        account.groups.push(group(id, &members));
        account.sessions.push(Session(id, 1_760_000_000 + g as i64, 0));
    }
    account.write();
    let keys = account.keys(&DATABASES);
    let started = std::time::Instant::now();
    let json = world
        .run(&["chats", "--with", "wxid_p0", "--with", "wxid_p1"], &[("WXREAD_KEYS", &keys)])
        .ok();
    let took = started.elapsed();
    assert!(!ids(&json).is_empty(), "{json}");
    assert!(took < std::time::Duration::from_millis(1000), "took {took:?}");
}

#[test]
fn paging_with_next_returns_each_chat_once() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = account.keys(&DATABASES);
    let env = [("WXREAD_KEYS", keys.as_str())];
    let whole = ids(&world.run(&["chats"], &env).ok());
    for limit in ["1", "2", "3"] {
        let mut paged = Vec::new();
        let mut args = vec!["chats", "--limit", limit];
        let mut cursor;
        loop {
            let json = world.run(&args, &env).ok();
            let page = ids(&json);
            assert!(page.len() <= limit.parse().unwrap(), "{json}");
            paged.extend(page);
            match json["next"].as_str() {
                Some(next) => {
                    cursor = next.to_string();
                    args = vec!["chats", "--limit", limit, "--cursor", &cursor];
                }
                None => break,
            }
        }
        assert_eq!(paged, whole, "--limit {limit}");
    }
}

#[test]
fn no_decrypted_byte_is_written() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = account.keys(&DATABASES);
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let before = snapshot(&account.dir());
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    world.run(&["chats"], &env).ok();
    world.run(&["chats", "--limit", "1"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}

#[test]
fn with_no_keys_it_names_the_databases_and_the_fix() {
    let world = World::new();
    account(&world, "wxid_me_1a2b");
    let error = world.run(&["chats"], &[]).error();
    assert!(error.contains(SESSION_DB) && error.contains(CONTACT_DB), "{error}");
    assert!(error.contains("sudo wxread keys") && error.contains("--keys <file>"), "{error}");
}

#[test]
fn a_database_the_keys_lack_is_named_with_the_fix() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = account.keys(&[SESSION_DB]);
    let error = world.run(&["chats"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains(&format!("no key of {CONTACT_DB}")), "{error}");
    assert!(error.contains("sudo wxread keys"), "{error}");
}

#[test]
fn a_key_that_no_longer_opens_its_database_is_named_with_the_fix() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let other = Account::new(&world.root, "wxid_other_9");
    let keys = account.keys(&[CONTACT_DB]).replace(
        &format!("\"{CONTACT_DB}\""),
        &format!(
            "\"{SESSION_DB}\": {{\"enc_key\": \"{}\"}}, \"{CONTACT_DB}\"",
            crate::synthetic::hex(&other.key(SESSION_DB))
        ),
    );
    let error = world.run(&["chats"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains(&format!("the key of {SESSION_DB} does not open it")), "{error}");
    assert!(error.contains("sudo wxread keys"), "{error}");
}

#[test]
fn keys_that_do_not_parse_name_every_fault() {
    let world = World::new();
    account(&world, "wxid_me_1a2b");
    let bad = r#"{"session/session.db": {"enc_key": "short"}, "contact/contact.db": {}}"#;
    let error = world.run(&["chats"], &[("WXREAD_KEYS", bad)]).error();
    assert!(error.contains("the key of session/session.db is not 64 hex digits"), "{error}");
    assert!(error.contains("the key of contact/contact.db has no enc_key"), "{error}");
}

#[test]
fn with_several_accounts_account_picks_one() {
    let world = World::new();
    let me = account(&world, "wxid_me_1a2b");
    let mut other = Account::new(&world.data_dir(), "wxid_other_3c4d");
    other.sessions = vec![Session("wxid_zed", 1_760_000_000, 0)];
    other.write();
    let error = world.run(&["chats"], &[("WXREAD_KEYS", &me.keys(&DATABASES))]).error();
    assert!(
        error.contains("wxid_me_1a2b, wxid_other_3c4d") && error.contains("--account <id>"),
        "{error}"
    );
    let json = world
        .run(
            &["chats", "--account", "wxid_other_3c4d"],
            &[("WXREAD_KEYS", &other.keys(&DATABASES))],
        )
        .ok();
    assert_eq!(
        (json["account"].clone(), ids(&json)),
        (json!("wxid_other_3c4d"), vec!["wxid_zed".to_string()])
    );
    let error = world.run(&["chats", "--account", "wxid_nobody"], &[]).error();
    assert!(
        error.contains("no account wxid_nobody") && error.contains("wxid_me_1a2b, wxid_other_3c4d"),
        "{error}"
    );
}

#[test]
fn no_account_points_at_doctor_s_fix() {
    let world = World::new();
    let error = world.run(&["chats"], &[]).error();
    assert!(
        error.contains("does not exist") && error.contains("install WeChat for Mac 4.x"),
        "{error}"
    );
}

#[test]
fn a_limit_or_cursor_that_is_not_one_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = account.keys(&DATABASES);
    for args in
        [&["chats", "--limit", "0"][..], &["chats", "--limit", "1001"], &["chats", "--limit", "x"]]
    {
        let error = world.run(args, &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains("from 1 to 1000"), "{args:?}: {error}");
    }
    let error = world.run(&["chats", "--cursor", "nope"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("leave --cursor out"), "{error}");
}

#[test]
fn an_unnamed_group_is_named_by_its_members_as_wechat_shows_it() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_qin", "秦俊斌 @qinjunbin", ""),
        Contact("wxid_sun", "Sun", "孙凤君"),
        Contact("1@chatroom", "", ""),
        Contact("2@chatroom", "Hiking", ""),
        Contact("3@chatroom", "", "Ours"),
    ];
    let three = vec![("wxid_sun", ""), ("wxid_me", ""), ("wxid_qin", "")];
    let group = |id| Group { id, owner: "wxid_me", members: three.clone(), notice: None };
    account.groups = vec![group("1@chatroom"), group("2@chatroom"), group("3@chatroom")];
    account.sessions = vec![
        Session("1@chatroom", 1_760_000_300, 0),
        Session("2@chatroom", 1_760_000_200, 0),
        Session("3@chatroom", 1_760_000_100, 0),
    ];
    account.write();
    let json = world.run(&["chats"], &[("WXREAD_KEYS", &account.keys(&DATABASES))]).ok();
    let names: Vec<&str> =
        json["chats"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["孙凤君、秦俊斌 @qinjunbin", "Hiking", "Ours"]);
}

#[test]
fn query_finds_the_chats_whose_id_or_name_holds_it() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_qin", "秦俊斌 @qinjunbin", ""),
        Contact("wxid_sun", "Sun", "孙凤君"),
        Contact("1@chatroom", "", ""),
        Contact("2@chatroom", "", ""),
        Contact("3@chatroom", "Hiking", ""),
        Contact("4@chatroom", "Climbing", "Ours"),
    ];
    let with_qin = vec![("wxid_sun", ""), ("wxid_me", ""), ("wxid_qin", "")];
    let without = vec![("wxid_sun", ""), ("wxid_me", "")];
    let group = |id, members: &Vec<(&'static str, &'static str)>| Group {
        id,
        owner: "wxid_me",
        members: members.clone(),
        notice: None,
    };
    account.groups = vec![
        group("1@chatroom", &with_qin),
        group("2@chatroom", &with_qin),
        group("3@chatroom", &with_qin),
        group("4@chatroom", &without),
    ];
    account.sessions = vec![
        Session("wxid_qin", 1_760_000_600, 0),
        Session("1@chatroom", 1_760_000_500, 0),
        Session("wxid_sun", 1_760_000_400, 0),
        Session("2@chatroom", 1_760_000_300, 0),
        Session("3@chatroom", 1_760_000_200, 0),
        Session("4@chatroom", 1_760_000_100, 0),
    ];
    account.write();
    let keys = account.keys(&DATABASES);
    let env = [("WXREAD_KEYS", keys.as_str())];
    let found = |args: &[&str]| {
        let json = world.run(&[&["chats", "--query"][..], args].concat(), &env).ok();
        (ids(&json), json["next"].as_str().map(String::from))
    };
    let all = |ids: &[&str]| (ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None);
    assert_eq!(found(&["秦俊斌"]), all(&["wxid_qin", "1@chatroom", "2@chatroom"]));
    assert_eq!(found(&["QINJUNBIN"]), all(&["wxid_qin", "1@chatroom", "2@chatroom"]));
    assert_eq!(found(&["孙凤君"]), all(&["1@chatroom", "wxid_sun", "2@chatroom"]));
    assert_eq!(found(&["hiking"]), all(&["3@chatroom"]));
    assert_eq!(found(&["ours"]), all(&["4@chatroom"]));
    assert_eq!(found(&["climbing"]), all(&["4@chatroom"]));
    assert_eq!(found(&["wxid_s"]), all(&["wxid_sun"]));
    assert_eq!(found(&["nobody"]), all(&[]));

    let (first, next) = found(&["秦俊斌", "--limit", "2"]);
    assert_eq!(first, ["wxid_qin", "1@chatroom"]);
    let (second, next) = found(&["秦俊斌", "--limit", "2", "--cursor", &next.unwrap()]);
    assert_eq!((second, next), all(&["2@chatroom"]));

    let json = world.run(&["chats", "--with", "wxid_qin", "--query", "孙凤君"], &env).ok();
    assert_eq!(ids(&json), ["1@chatroom", "2@chatroom"]);
}
