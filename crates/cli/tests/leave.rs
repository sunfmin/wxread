//! `wxread group leave`, run against synthetic accounts. No WeChat is ever
//! logged in as one, so `--yes` is refused before the window is touched:
//! what the path does in WeChat's window is tested in the leave crate, on
//! recorded screens, and on a real Mac by hand (docs/leave-group.md).
#![cfg(target_os = "macos")]

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::{Value, json};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Group, Said};

const GROUP: &str = "123@chatroom";
const AT: i64 = 1_760_000_000;

/// An account in group "Ads" with Amy, who said the last thing in it; Amy
/// and Bob are in "Hiking" without the account; and two groups the account
/// is in show as "Twins".
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", ""),
        Contact("wxid_bob", "Bob", ""),
        Contact(GROUP, "Ads", ""),
        Contact("9@chatroom", "Hiking", ""),
        Contact("7@chatroom", "Twins", ""),
        Contact("8@chatroom", "Twins", ""),
        Contact("6@chatroom", "Left", ""),
        Contact("5@chatroom", "Left", ""),
    ];
    let group = |id, members: &[&'static str]| Group {
        id,
        owner: "wxid_amy",
        members: members.iter().map(|m| (*m, "")).collect(),
        notice: None,
    };
    account.groups = vec![
        group(GROUP, &["wxid_amy", "wxid_me"]),
        group("9@chatroom", &["wxid_amy", "wxid_bob"]),
        group("7@chatroom", &["wxid_amy", "wxid_me"]),
        group("8@chatroom", &["wxid_bob", "wxid_me"]),
        // "Left": the account is in one, and has left the other.
        group("6@chatroom", &["wxid_amy", "wxid_me"]),
        group("5@chatroom", &["wxid_amy"]),
    ];
    account.messages = vec![
        Said::text(GROUP, 1, AT, "wxid_amy", "cheap flats"),
        Said::text(GROUP, 2, AT + 60, "wxid_amy", "cheaper flats"),
    ];
    account.write();
    account
}

fn keys(account: &Account) -> String {
    account.all_keys()
}

/// The keys of an answer, sorted.
fn keys_of(json: &Value) -> BTreeSet<String> {
    json.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn without_yes_it_answers_the_plan_and_changes_nothing() {
    let world = World::new();
    let account = account(&world);
    let before = snapshot(&world.root);
    let json = world.run(&["group", "leave", GROUP], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(snapshot(&world.root), before, "nothing was written");
    assert_eq!(json["account"], "wxid_me_1a2b");
    assert_eq!(json["applied"], false);
    assert_eq!(json["confirmed_in"], Value::Null);
    let group = &json["group"];
    assert_eq!(
        (&group["id"], &group["display_name"], &group["name"], &group["members"]),
        (&json!(GROUP), &json!("Ads"), &json!("Ads"), &json!(2))
    );
    assert_eq!(group["last_message"]["content"], "cheaper flats", "its newest message");
    let plan: Vec<&str> =
        json["plan"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    assert!(plan.iter().any(|s| s.contains("title reads Ads (2)")), "{plan:?}");
    assert!(plan.iter().any(|s| s.contains("Group Name reads Ads")), "{plan:?}");
    // No WeChat is logged in as a synthetic account: that blocks --yes.
    let blockers: Vec<&str> =
        json["blockers"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    assert!(
        blockers.iter().any(|b| b.starts_with("WeChat is not running")
            || b.contains("not wxid_me_1a2b")
            || b.starts_with("WeChat is running, logged in as no account")),
        "{blockers:?}"
    );
    assert!(json["next"].as_str().unwrap().starts_with("nothing was changed"), "{json}");
}

#[test]
fn with_yes_what_blocks_it_is_refused_before_the_window_is_touched() {
    let world = World::new();
    let account = account(&world);
    let before = snapshot(&world.root);
    let error =
        world.run(&["group", "leave", GROUP, "--yes"], &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(error.starts_with("nothing was touched: "), "{error}");
    assert!(error.contains("wxid_me_1a2b"), "names the account to log in as: {error}");
    assert_eq!(snapshot(&world.root), before);
}

#[test]
fn a_group_the_account_is_not_in_is_refused() {
    let world = World::new();
    let account = account(&world);
    for yes in [&[][..], &["--yes"]] {
        let mut args = vec!["group", "leave", "9@chatroom"];
        args.extend(yes);
        let error = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).error();
        assert_eq!(
            error,
            "the account is not a member of 9@chatroom (Hiking): there is nothing to leave; `wxread group 9@chatroom` answers who is"
        );
    }
}

#[test]
fn a_group_another_of_the_accounts_groups_is_shown_as_is_refused() {
    let world = World::new();
    let account = account(&world);
    let error =
        world.run(&["group", "leave", "7@chatroom"], &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(
        error.starts_with("more than one of the account's groups is shown as Twins (7@chatroom, 8@chatroom): nothing in WeChat's window tells them apart"),
        "{error}"
    );
}

#[test]
fn a_group_of_the_same_name_the_account_has_left_does_not_stop_it() {
    let world = World::new();
    let account = account(&world);
    let json =
        world.run(&["group", "leave", "6@chatroom"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(json["group"]["display_name"], "Left");
    assert_eq!(json["group"]["last_message"], Value::Null, "no message in it");
}

#[test]
fn an_id_that_is_no_group_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = keys(&account);
    let error = world.run(&["group", "leave", "wxid_amy"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("wxid_amy is a person, not a group"), "{error}");
    let error = world.run(&["group", "leave", "404@chatroom"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("`wxread contacts --query"), "{error}");
    let answer = world.run(&["group", "leave"], &[]);
    assert_eq!(answer.code, 2);
    assert!(answer.error().contains("not provided: <ID>"));
}

#[test]
fn group_still_answers_a_group() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["group", GROUP], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(json["group"]["members"].as_array().unwrap().len(), 2);
}

/// What the binary prints for `args`, which must exit 0.
fn asked(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_wxread")).args(args).output().unwrap();
    assert!(out.status.success() && out.stderr.is_empty(), "{args:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The lines of `text` after the one starting with `from` up to the one
/// starting with `to`.
fn between<'a>(text: &'a str, from: &str, to: &str) -> Vec<&'a str> {
    let after = text.lines().skip_while(|line| !line.starts_with(from)).skip(1);
    after.take_while(|line| !line.starts_with(to)).collect()
}

#[test]
fn its_help_says_it_cannot_be_undone_what_it_needs_and_that_it_takes_the_window() {
    let help = asked(&["group", "leave", "--help"]);
    assert_eq!(asked(&["group", "leave", "-h"]), help);
    assert_eq!(asked(&["--keys", "/nowhere", "group", "leave", "1@chatroom", "--help"]), help);
    assert_ne!(asked(&["group", "--help"]), help, "group has its own");
    let words = help.split_whitespace().collect::<Vec<_>>().join(" ");
    for said in [
        "It cannot be undone",
        "Accessibility",
        "Screen & System Audio Recording",
        "for about ten seconds",
        "Without --yes it says what it would do and changes nothing",
        "`wxread doctor` says whether each is allowed",
    ] {
        assert!(words.contains(said), "group leave --help says {said}");
    }
}

#[test]
fn it_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["group", "leave", GROUP], &[("WXREAD_KEYS", &keys(&account))]).ok();
    let help = asked(&["group", "leave", "--help"]);
    let lines = between(&help, "WHAT IT ANSWERS", "TRIPS YOU UP").into_iter();
    let keys = lines.filter(|line| line.len() - line.trim_start().len() == 4);
    let answered: BTreeSet<String> =
        keys.filter_map(|line| line.split_whitespace().next()).map(str::to_string).collect();
    assert_eq!(answered, keys_of(&json));
    let mut rows = between(wxread::GUIDE, "WHAT A COMMAND ANSWERS", "THE COMMANDS")
        .into_iter()
        .skip_while(|line| !line.starts_with("    group leave "));
    let row = rows.next().expect("the guide has a row for group leave");
    let more = rows.take_while(|line| line.starts_with(&" ".repeat(14)));
    let named: BTreeSet<String> = row
        .split_whitespace()
        .skip(2)
        .chain(more.flat_map(str::split_whitespace))
        .map(str::to_string)
        .collect();
    assert_eq!(named, keys_of(&json));
}

#[test]
fn the_guide_lists_it_apart_from_the_reading_commands() {
    let commands = between(wxread::GUIDE, "THE COMMANDS", "RULES");
    let at = commands.iter().position(|l| l.trim() == "group leave").expect("listed");
    let heading: Vec<&str> =
        commands[..at].iter().rev().take_while(|l| !l.starts_with("      ")).copied().collect();
    let heading = heading.into_iter().rev().collect::<Vec<_>>().join(" ");
    assert!(heading.contains("Leaving a group, which cannot be undone"), "{heading}");
    let reading: Vec<&&str> =
        commands.iter().skip_while(|l| !l.contains("Reading the account")).take(4).collect();
    assert!(reading.iter().all(|l| !l.contains("leave")), "{reading:?}");
}
