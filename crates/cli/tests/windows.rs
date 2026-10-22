//! The Windows port's own smoke tests: the default data directory is the one
//! `%USERPROFILE%\xwechat_files` form, `doctor` reads it, and a chat is read
//! through a key supplied on the command line. The full macOS CLI suite lives
//! in `tests/main.rs`; this target is what runs on a Windows build.
#![cfg(windows)]

mod common;
mod synthetic;

use serde_json::json;

use common::World;
use synthetic::{Account, Contact, DATABASES, Session};

/// An account with one friend, written where WeChat for Windows 4.x keeps
/// its accounts.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy", "Amy W")];
    account.sessions = vec![Session("wxid_amy", 1_760_000_300, 2)];
    account.write();
    account
}

#[test]
fn doctor_finds_the_windows_home_data_directory() {
    let world = World::new();
    let account = account(&world);
    let keys = world.file("keys.json", &account.keys(&DATABASES));
    let json = world.run(&["--keys", keys.to_str().unwrap(), "doctor"], &[]).ok();

    assert_eq!(json["ok"], true, "{json}");
    assert_eq!(
        json["data_dir"],
        json!({"path": world.data_dir().display().to_string(), "from": "default"})
    );
    assert_eq!(json["accounts"][0]["id"], "wxid_me_1a2b");
    assert_eq!(json["accounts"][0]["keyed"], 3);
    assert_eq!(json["permissions"], json!({}));
}

#[test]
fn chats_are_read_with_a_supplied_key() {
    let world = World::new();
    let account = account(&world);
    let keys = world.file("keys.json", &account.keys(&DATABASES));
    let json = world.run(&["chats", "--keys", keys.to_str().unwrap()], &[]).ok();

    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "chats": [
                {"id": "wxid_amy", "display_name": "Amy W", "kind": "direct", "read_with": "wxread messages --chat wxid_amy", "last_message_at": "2025-10-09T08:58:20Z", "unread": 2, "members": 2, "recent": null, "account_spoke": null},
            ],
            "next": null,
        })
    );
}

#[test]
fn help_names_windows_and_its_data_directory() {
    let world = World::new();
    let out = world.command(&["--help"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("WeChat for Windows 4.x"), "{text}");
    assert!(text.contains("%USERPROFILE%\\xwechat_files"), "{text}");
}
