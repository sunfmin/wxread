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
                {"id": "wxid_bob", "name": "Bob", "kind": "direct", "last_message_at": "2025-10-09T09:00:00Z", "unread": 1, "members": 2},
                {"id": "wxid_amy", "name": "Amy W", "kind": "direct", "last_message_at": "2025-10-09T08:58:20Z", "unread": 2, "members": 2},
                {"id": "123@chatroom", "name": "Hiking", "kind": "group", "last_message_at": "2025-10-09T08:56:40Z", "unread": 0, "members": null},
                {"id": "wxid_stranger", "name": "", "kind": "direct", "last_message_at": "2025-10-09T08:55:00Z", "unread": 0, "members": 2},
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
                {"id": "2@chatroom", "name": "Small", "kind": "group", "last_message_at": null, "unread": 0, "members": 3},
                {"id": "1@chatroom", "name": "Big", "kind": "group", "last_message_at": "2025-10-09T09:00:00Z", "unread": 5, "members": 4},
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
