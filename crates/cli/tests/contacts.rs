//! `wxread contacts`, `contact` and `group`: contacts and groups as the
//! account sees them, read from its encrypted contact and head image
//! databases.

use serde_json::{Value, json};
use wxread_schema::{CONTACT_DB, HEAD_IMAGE_DB};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, DATABASES, Group, Profile};

/// An account with a friend it gave a remark, one it did not, an
/// official account, and a group of the three of them and a stranger,
/// with a notice.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me_1a2b", "Me", ""),
        Contact("wxid_amy", "Amy", "Auntie Amy"),
        Contact("wxid_bob", "Bob", ""),
        Contact("gh_news", "Daily News", ""),
        Contact("123@chatroom", "Hiking", "Weekend hikes"),
    ];
    account.profiles =
        vec![Profile("wxid_amy", "amy88", "Out walking"), Profile("wxid_bob", "", "")];
    account.avatars = vec!["wxid_amy", "123@chatroom"];
    account.groups = vec![Group {
        id: "123@chatroom",
        owner: "wxid_bob",
        members: vec![
            ("wxid_me_1a2b", ""),
            ("wxid_amy", "Amy W."),
            ("wxid_bob", "Bobby"),
            ("wxid_stranger", "Sam"),
        ],
        notice: Some(("Sunday 8am at the station", "wxid_bob", 1_760_000_000)),
    }];
    account.write();
    account
}

fn keys(account: &Account) -> String {
    account.keys(&DATABASES)
}

fn ids(json: &Value) -> Vec<&str> {
    json["contacts"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect()
}

#[test]
fn contacts_lists_every_contact_by_id_with_what_wechat_holds() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["contacts"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "contacts": [
                {"id": "123@chatroom", "display_name": "Weekend hikes", "name": "Hiking", "remark": "Weekend hikes", "alias": null, "signature": null, "kind": "group", "has_avatar": true},
                {"id": "gh_news", "display_name": "Daily News", "name": "Daily News", "remark": null, "alias": null, "signature": null, "kind": "official_account", "has_avatar": false},
                {"id": "wxid_amy", "display_name": "Auntie Amy", "name": "Amy", "remark": "Auntie Amy", "alias": "amy88", "signature": "Out walking", "kind": "person", "has_avatar": true},
                {"id": "wxid_bob", "display_name": "Bob", "name": "Bob", "remark": null, "alias": null, "signature": null, "kind": "person", "has_avatar": false},
                {"id": "wxid_me_1a2b", "display_name": "Me", "name": "Me", "remark": null, "alias": null, "signature": null, "kind": "person", "has_avatar": false},
            ],
            "next": null,
        })
    );
}

#[test]
fn query_finds_a_contact_by_remark_by_name_and_by_id() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    let found = |query: &str| {
        let json = world.run(&["contacts", "--query", query], &[("WXREAD_KEYS", &keys)]).ok();
        ids(&json).into_iter().map(String::from).collect::<Vec<_>>()
    };
    assert_eq!(found("auntie"), ["wxid_amy"]);
    assert_eq!(found("BOB"), ["wxid_bob"]);
    assert_eq!(found("wxid_am"), ["wxid_amy"]);
    assert_eq!(found("amy88"), ["wxid_amy"]);
    assert_eq!(found("hik"), ["123@chatroom"]);
    assert_eq!(found("nobody"), Vec::<String>::new());
}

#[test]
fn paging_with_next_returns_each_contact_once() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    let env = [("WXREAD_KEYS", keys.as_str())];
    let whole: Vec<String> =
        ids(&world.run(&["contacts"], &env).ok()).into_iter().map(String::from).collect();
    for limit in ["1", "2", "4"] {
        let mut paged = Vec::new();
        let mut cursor = None::<String>;
        loop {
            let mut args = vec!["contacts", "--limit", limit];
            if let Some(next) = &cursor {
                args.extend(["--cursor", next.as_str()]);
            }
            let json = world.run(&args, &env).ok();
            paged.extend(ids(&json).into_iter().map(String::from));
            match json["next"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        assert_eq!(paged, whole, "--limit {limit}");
    }
}

#[test]
fn contact_answers_one_contact_in_full() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["contact", "wxid_amy"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "contact": {"id": "wxid_amy", "display_name": "Auntie Amy", "name": "Amy", "remark": "Auntie Amy", "alias": "amy88", "signature": "Out walking", "kind": "person", "has_avatar": true},
        })
    );
}

#[test]
fn group_answers_its_name_notice_owner_and_every_member_named_in_it() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["group", "123@chatroom"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "group": {
                "id": "123@chatroom",
                "display_name": "Weekend hikes",
                "name": "Hiking",
                "remark": "Weekend hikes",
                "owner": {"id": "wxid_bob", "display_name": "Bobby"},
                "notice": {
                    "text": "Sunday 8am at the station",
                    "by": {"id": "wxid_bob", "display_name": "Bobby"},
                    "at": "2025-10-09T08:53:20Z",
                },
                "members": [
                    {"id": "wxid_amy", "display_name": "Auntie Amy", "name": "Amy", "name_in_group": "Amy W.", "remark": "Auntie Amy"},
                    {"id": "wxid_bob", "display_name": "Bobby", "name": "Bob", "name_in_group": "Bobby", "remark": null},
                    {"id": "wxid_me_1a2b", "display_name": "Me", "name": "Me", "name_in_group": null, "remark": null},
                    {"id": "wxid_stranger", "display_name": "Sam", "name": "", "name_in_group": "Sam", "remark": null},
                ],
            },
        })
    );
}

#[test]
fn a_group_with_no_notice_answers_null() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("9@chatroom", "Quiet", "")];
    account.write();
    let json = world.run(&["group", "9@chatroom"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(
        (&json["group"]["notice"], &json["group"]["owner"], &json["group"]["members"]),
        (&json!(null), &json!(null), &json!([]))
    );
}

#[test]
fn an_unknown_id_is_refused_with_a_fix_pointing_at_contacts() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    for args in [&["contact", "wxid_nobody"][..], &["group", "999@chatroom"]] {
        let error = world.run(args, &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains(&format!("no contact {}", args[1])), "{error}");
        assert!(error.contains("`wxread contacts --query"), "{error}");
    }
    let error = world.run(&["group", "wxid_amy"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("wxid_amy is a person, not a group"), "{error}");
    assert!(error.contains("`wxread contact wxid_amy`"), "{error}");
}

#[test]
fn a_command_line_without_an_id_is_refused_pointing_at_the_guide() {
    let world = World::new();
    for command in ["contact", "group"] {
        let answer = world.run(&[command], &[]);
        assert_eq!(answer.code, 2, "{command}");
        let error = answer.error();
        assert!(error.contains("not provided: <ID>"), "{error}");
        assert!(error.ends_with(": run `wxread --help`"), "{error}");
    }
}

#[test]
fn the_databases_the_keys_lack_are_named_with_the_fix() {
    let world = World::new();
    let account = account(&world);
    let error = world.run(&["contacts"], &[]).error();
    assert!(error.contains(CONTACT_DB) && error.contains(HEAD_IMAGE_DB), "{error}");
    let keys = account.keys(&[CONTACT_DB]);
    for args in [&["contacts"][..], &["contact", "wxid_amy"]] {
        let error = world.run(args, &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains(&format!("no key of {HEAD_IMAGE_DB}")), "{error}");
        assert!(error.contains("sudo wxread keys"), "{error}");
    }
    let json = world.run(&["group", "123@chatroom"], &[("WXREAD_KEYS", &keys)]).ok();
    assert_eq!(json["group"]["id"], "123@chatroom", "group reads the contact database alone");
}

#[test]
fn a_limit_or_cursor_that_is_not_one_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    let error = world.run(&["contacts", "--limit", "0"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("number of contacts from 1 to 1000"), "{error}");
    let error = world.run(&["contacts", "--cursor", ""], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("leave --cursor out"), "{error}");
}

#[test]
fn no_decrypted_byte_is_written() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let before = snapshot(&account.dir());
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    world.run(&["contacts"], &env).ok();
    world.run(&["contact", "wxid_amy"], &env).ok();
    world.run(&["group", "123@chatroom"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}
