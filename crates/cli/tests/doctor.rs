//! `wxread doctor`: what it finds of WeChat's accounts and the keys, a fix
//! for each finding, and nothing written.

use std::os::unix::fs::PermissionsExt;

use serde_json::{Value, json};
use wxread_doctor::{ALLOW_READING, DATA_DIR_UNDER_HOME, Permission};

use crate::common::{World, snapshot};

const DATABASES: [&str; 3] = ["contact/contact.db", "message/message_0.db", "session/session.db"];

fn problems(json: &Value) -> Vec<String> {
    let findings = json["findings"].as_array().unwrap();
    for finding in findings {
        assert!(!finding["fix"].as_str().unwrap().is_empty(), "{finding}");
    }
    findings.iter().map(|f| f["problem"].as_str().unwrap().to_string()).collect()
}

#[test]
fn an_account_whose_every_database_has_a_key_is_ok() {
    let world = World::new();
    let storage = world.account(&world.data_dir(), "wxid_a_1234", &DATABASES);
    let keys = world.file("keys.json", &World::keys(&DATABASES));
    let mut json = world.run(&["doctor", "--keys", keys.to_str().unwrap()], &[]).ok();
    // What macOS allows is the host's, and no finding: its shape is
    // checked apart.
    permissions_are_answered(json.as_object_mut().unwrap().remove("permissions").unwrap());
    assert_eq!(
        json,
        json!({
            "ok": true,
            "data_dir": {"path": world.data_dir().display().to_string(), "from": "default"},
            "accounts": [{
                "id": "wxid_a_1234",
                "db_storage": storage.display().to_string(),
                "databases": 3,
                "keyed": 3,
                "unkeyed": [],
            }],
            "keys": {"supplied": true, "from": "flag", "looked": ["--keys <file>", "WXREAD_KEYS"], "databases": 3},
            "findings": [],
        })
    );
}

/// `permissions` says, for each permission `group leave` needs, whether it
/// is granted, what needs it, and the fix exactly when it is not granted.
fn permissions_are_answered(permissions: Value) {
    let names: Vec<&str> = Permission::ALL.iter().map(|p| p.name()).collect();
    let answered: Vec<&str> = permissions.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(answered, names);
    for p in Permission::ALL {
        let one = &permissions[p.name()];
        let granted = one["granted"].as_bool().expect("granted is true or false");
        assert_eq!(one["needed_for"], p.needed_for());
        let fix = if granted { Value::Null } else { Value::from(p.fix()) };
        assert_eq!(one["fix"], fix, "{one}");
    }
}

#[test]
fn with_no_keys_it_says_where_it_looked_and_points_at_the_keys_help() {
    let world = World::new();
    world.account(&world.data_dir(), "wxid_a_1234", &DATABASES);
    let json = world.run(&["doctor"], &[]).ok();
    assert_eq!(json["ok"], false);
    assert_eq!(json["keys"]["supplied"], false);
    assert_eq!(json["keys"]["from"], Value::Null);
    assert_eq!(json["accounts"][0]["unkeyed"], json!(DATABASES));
    assert_eq!(problems(&json), ["no keys were supplied: neither --keys nor WXREAD_KEYS is set"]);
    assert!(json["findings"][0]["fix"].as_str().unwrap().contains("`wxread keys --help`"));
}

#[test]
fn the_keys_variable_is_read_and_the_flag_comes_before_it() {
    let world = World::new();
    world.account(&world.data_dir(), "wxid_a_1234", &DATABASES);
    let all = World::keys(&DATABASES);
    let json = world.run(&["doctor"], &[("WXREAD_KEYS", &all)]).ok();
    assert_eq!(
        (json["ok"].clone(), json["keys"]["from"].clone()),
        (json!(true), json!("variable"))
    );
    let one = world.file("one.json", &World::keys(&DATABASES[..1]));
    let json =
        world.run(&["doctor", "--keys", one.to_str().unwrap()], &[("WXREAD_KEYS", &all)]).ok();
    assert_eq!(json["keys"]["from"], "flag");
    assert_eq!(json["accounts"][0]["keyed"], 1);
    assert_eq!(
        problems(&json),
        [
            "account wxid_a_1234 has 2 of 3 databases with no key: message/message_0.db, session/session.db"
        ]
    );
}

#[test]
fn keys_that_cannot_be_read_name_every_fault() {
    let world = World::new();
    world.account(&world.data_dir(), "wxid_a_1234", &DATABASES);
    let json = world.run(&["doctor", "--keys", "/nowhere/keys.json"], &[]).ok();
    let problem = &problems(&json)[0];
    assert!(problem.starts_with("the keys file /nowhere/keys.json cannot be read"), "{problem}");
    let bad = r#"{"contact/contact.db": {"enc_key": "short"}, "message/message_0.db": {}}"#;
    let json = world.run(&["doctor"], &[("WXREAD_KEYS", bad)]).ok();
    assert_eq!(
        problems(&json),
        [
            "in WXREAD_KEYS, the key of contact/contact.db is not 64 hex digits",
            "in WXREAD_KEYS, the key of message/message_0.db has no enc_key",
        ]
    );
}

#[test]
fn each_account_is_found_by_its_db_storage() {
    let world = World::new();
    let dir = world.data_dir();
    world.account(&dir, "wxid_b_2", &DATABASES[..1]);
    world.account(&dir, "wxid_a_1", &DATABASES);
    // WeChat's own directories beside the accounts hold no db_storage.
    std::fs::create_dir_all(dir.join("all_users/login")).unwrap();
    std::fs::write(dir.join("all_users/login/x.db"), b"").unwrap();
    std::fs::write(dir.join("wxid_a_1/db_storage/message/message_0.db-wal"), b"").unwrap();
    let json = world
        .run(
            &["doctor", "--keys", world.file("k", &World::keys(&DATABASES)).to_str().unwrap()],
            &[],
        )
        .ok();
    let ids: Vec<&str> =
        json["accounts"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["wxid_a_1", "wxid_b_2"]);
    assert_eq!(json["accounts"][0]["databases"], 3);
    assert_eq!(json["accounts"][1]["databases"], 1);
    assert_eq!(json["ok"], true);
}

#[test]
fn no_data_directory_or_no_account_says_to_log_in() {
    let world = World::new();
    let json = world.run(&["doctor"], &[]).ok();
    let path = world.data_dir().display().to_string();
    assert_eq!(json["data_dir"], json!({"path": path, "from": "default"}));
    assert_eq!(json["accounts"], json!([]));
    assert_eq!(
        problems(&json),
        [
            "no keys were supplied: neither --keys nor WXREAD_KEYS is set".to_string(),
            format!("WeChat's data directory {path} does not exist"),
        ]
    );
    std::fs::create_dir_all(world.data_dir()).unwrap();
    let json = world.run(&["doctor"], &[]).ok();
    assert!(
        json["findings"][1]["fix"].as_str().unwrap().starts_with("log in to WeChat for Mac 4.x")
    );
}

#[test]
fn the_data_directory_is_the_flag_else_the_variable_else_under_home() {
    let world = World::new();
    let elsewhere = world.root.join("elsewhere");
    world.account(&elsewhere, "wxid_a_1", &DATABASES);
    let keys = World::keys(&DATABASES);
    let env = [("WXREAD_KEYS", keys.as_str()), ("WXREAD_DATA_DIR", elsewhere.to_str().unwrap())];
    let json = world.run(&["doctor"], &env).ok();
    assert_eq!(json["data_dir"]["from"], "variable");
    assert_eq!(json["ok"], true);
    let json = world.run(&["doctor", "--data-dir", "/nowhere"], &env).ok();
    assert_eq!(json["data_dir"], json!({"path": "/nowhere", "from": "flag"}));
    assert!(json["findings"][0]["fix"].as_str().unwrap().contains(DATA_DIR_UNDER_HOME));
}

#[test]
fn a_directory_it_may_not_read_is_a_finding_with_the_permission_to_grant() {
    let world = World::new();
    let storage = world.account(&world.data_dir(), "wxid_a_1", &DATABASES);
    let keys = World::keys(&DATABASES);
    for locked in [storage.join("message"), world.data_dir()] {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let json = world.run(&["doctor"], &[("WXREAD_KEYS", &keys)]).ok();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(json["findings"].as_array().unwrap().len(), 1, "{json}");
        assert_eq!(json["findings"][0]["fix"], ALLOW_READING);
    }
}

#[test]
fn it_writes_nothing() {
    let world = World::new();
    world.account(&world.data_dir(), "wxid_a_1", &DATABASES);
    let keys = world.file("keys.json", &World::keys(&DATABASES[..2]));
    let before = snapshot(&world.root);
    for args in [&["doctor"][..], &["doctor", "--keys", keys.to_str().unwrap()]] {
        world.run(args, &[]).ok();
    }
    assert_eq!(snapshot(&world.root), before);
}
