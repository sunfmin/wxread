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

/// An account in group "Ads" with Amy, who said the last things in it;
/// Amy and Bob are in "Hiking" without the account; and two groups the
/// account is in show as "Twins".
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

/// The one group of an answer whose id is `id`.
fn group_of<'a>(json: &'a Value, id: &str) -> &'a Value {
    json["groups"].as_array().unwrap().iter().find(|g| g["id"] == id).expect(id)
}

/// The strings of a JSON array.
fn strings(json: &Value) -> Vec<&str> {
    json.as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()
}

#[test]
fn without_yes_it_answers_the_plan_and_changes_nothing() {
    let world = World::new();
    let account = account(&world);
    let before = snapshot(&world.root);
    let json = world.run(&["group", "leave", GROUP], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(snapshot(&world.root), before, "nothing was written");
    assert_eq!(json["account"], "wxid_me_1a2b");
    let group = group_of(&json, GROUP);
    assert_eq!(
        (&group["id"], &group["display_name"], &group["name"], &group["members"]),
        (&json!(GROUP), &json!("Ads"), &json!("Ads"), &json!(2))
    );
    assert_eq!((&group["outcome"], &group["confirmed_in"]), (&json!("planned"), &Value::Null));
    assert_eq!(group["last_message"]["content"], "cheaper flats", "its newest message");
    let plan = strings(&group["plan"]);
    assert!(
        plan.iter().any(|s| s.contains("title shows 2 members, and most of the 2 newest messages")),
        "{plan:?}"
    );
    assert!(plan.iter().any(|s| s.contains("click Leave")), "{plan:?}");
    assert_eq!(
        (&group["identity"], &group["seconds"], &group["error"]),
        (&Value::Null, &Value::Null, &Value::Null),
        "no chat was opened"
    );
    assert!(json["seconds"]["read"].as_f64().unwrap() >= 0.0, "{json}");
    assert_eq!(json["seconds"]["idle"], 0.0, "the Mac was not waited for");
    // No WeChat is logged in as a synthetic account: that blocks --yes.
    let blockers = strings(&json["blockers"]);
    assert!(
        blockers.iter().any(|b| b.starts_with("WeChat is not running")
            || b.contains("not wxid_me_1a2b")
            || b.starts_with("WeChat is running, logged in as no account")),
        "{blockers:?}"
    );
    assert!(json["next"].as_str().unwrap().starts_with("nothing was changed"), "{json}");
}

#[test]
fn several_groups_are_each_planned_in_the_order_given() {
    let world = World::new();
    let account = account(&world);
    let args = ["group", "leave", "6@chatroom", GROUP, "6@chatroom"];
    let json = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).ok();
    let ids: Vec<&Value> = json["groups"].as_array().unwrap().iter().map(|g| &g["id"]).collect();
    assert_eq!(ids, [&json!("6@chatroom"), &json!(GROUP)], "each once, in the order given");
    assert!(
        strings(&group_of(&json, "6@chatroom")["plan"])
            .iter()
            .any(|s| s.contains("search for Left"))
    );
    assert!(strings(&group_of(&json, GROUP)["plan"]).iter().any(|s| s.contains("search for Ads")));
}

#[test]
fn the_ids_can_be_given_on_stdin() {
    use std::io::Write as _;
    let world = World::new();
    let account = account(&world);
    let mut command = world.command(&["group", "leave", "-"]);
    command.env("WXREAD_KEYS", keys(&account));
    command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(format!("{GROUP}\n6@chatroom\n").as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<&Value> = json["groups"].as_array().unwrap().iter().map(|g| &g["id"]).collect();
    assert_eq!(ids, [&json!(GROUP), &json!("6@chatroom")]);
}

#[test]
fn no_id_on_stdin_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let error = world.run(&["group", "leave", "-"], &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(error.starts_with("no group's id was given on stdin"), "{error}");
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
fn with_dry_run_what_blocks_it_is_refused_before_the_window_is_touched() {
    let world = World::new();
    let account = account(&world);
    let before = snapshot(&world.root);
    let args = ["group", "leave", GROUP, "--dry-run"];
    let error = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(error.starts_with("nothing was touched: "), "{error}");
    assert!(error.contains("--dry-run"), "says to run --dry-run again: {error}");
    assert_eq!(snapshot(&world.root), before);
}

#[test]
fn dry_run_and_yes_together_are_refused() {
    let world = World::new();
    let account = account(&world);
    let args = ["group", "leave", GROUP, "--dry-run", "--yes"];
    let error = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(error.starts_with("--dry-run never leaves and --yes leaves"), "{error}");
}

#[test]
fn a_group_the_account_is_not_in_is_answered_as_nothing_to_leave() {
    let world = World::new();
    let account = account(&world);
    for yes in [&[][..], &["--yes"], &["--dry-run"]] {
        let mut args = vec!["group", "leave", "9@chatroom", "5@chatroom"];
        args.extend(yes);
        let json = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).ok();
        for id in ["9@chatroom", "5@chatroom"] {
            assert_eq!(group_of(&json, id)["outcome"], "not_member", "{json}");
        }
        assert_eq!(json["blockers"], json!([]), "nothing is to be left, so nothing blocks it");
        assert!(json["next"].as_str().unwrap().starts_with("nothing to leave"), "{json}");
    }
}

#[test]
fn every_group_that_cannot_be_left_is_named_at_once_and_nothing_is_touched() {
    let world = World::new();
    let account = account(&world);
    let args = ["group", "leave", GROUP, "wxid_bob", "wxid_amy", "--yes"];
    let error = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).error();
    assert!(error.contains("wxid_bob is a person, not a group"), "{error}");
    assert!(error.contains("wxid_amy is a person, not a group"), "{error}");
    assert!(!error.contains("nothing was touched: WeChat"), "the groups are named first: {error}");
}

#[test]
fn groups_shown_alike_are_each_planned_to_be_told_by_their_chats() {
    // docs/adr/0017: two groups of a name are told apart by their member
    // counts and messages, no longer refused.
    let world = World::new();
    let account = account(&world);
    let args = ["group", "leave", "7@chatroom", "8@chatroom"];
    let json = world.run(&args, &[("WXREAD_KEYS", &keys(&account))]).ok();
    for id in ["7@chatroom", "8@chatroom"] {
        let group = group_of(&json, id);
        assert_eq!(
            (&group["display_name"], &group["outcome"]),
            (&json!("Twins"), &json!("planned"))
        );
    }
}

#[test]
fn a_group_of_the_same_name_the_account_has_left_does_not_stop_it() {
    let world = World::new();
    let account = account(&world);
    let json =
        world.run(&["group", "leave", "6@chatroom"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    let group = group_of(&json, "6@chatroom");
    assert_eq!((&group["display_name"], &group["outcome"]), (&json!("Left"), &json!("planned")));
    assert_eq!(group["last_message"], Value::Null, "no message in it");
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

/// The name of each item of the list under What it answers in `help`,
/// `depth` levels deep: `- \`name\`: …`.
fn answered(help: &str, depth: usize) -> BTreeSet<String> {
    let marker = format!("{}- `", "  ".repeat(depth));
    let part = help.lines().skip_while(|line| *line != "**What it answers**").skip(1);
    let part = part.take_while(|line| !line.starts_with("**") && !line.starts_with('#'));
    let rest = part.filter_map(|line| line.strip_prefix(&marker));
    rest.filter_map(|rest| rest.split('`').next()).map(str::to_string).collect()
}

#[test]
fn its_help_says_it_cannot_be_undone_what_it_needs_and_that_it_takes_the_window() {
    let help = asked(&["group", "leave", "--help"]);
    assert_eq!(asked(&["group", "leave", "-h"]), help);
    assert_eq!(asked(&["--keys", "/nowhere", "group", "leave", "1@chatroom", "--help"]), help);
    assert_ne!(asked(&["group", "--help"]), help, "group has its own");
    for said in [
        "It cannot be undone",
        "Accessibility",
        "Screen & System Audio Recording",
        "a few seconds each",
        "Without --yes it says what it would do and changes nothing",
        "with --dry-run it opens each group's chat and says whether it is told to be the group, and leaves none",
        "its title shows the group's member count and most of the group's newest messages are in it",
        "`wxread doctor` says whether each is allowed",
        "stops at the first group it fails to leave",
        "the same command again",
        "wxread group leave - [--account <id>] [--dry-run | --yes]",
        "\n## Usage\n\n```\nwxread group leave [OPTIONS] <ID>...\n```\n",
    ] {
        assert!(help.contains(said), "group leave --help says {said}");
    }
}

#[test]
fn it_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["group", "leave", GROUP], &[("WXREAD_KEYS", &keys(&account))]).ok();
    let help = asked(&["group", "leave", "--help"]);
    assert_eq!(answered(&help, 0), keys_of(&json));
    assert_eq!(answered(&help, 1), keys_of(&json["groups"][0]), "each key of a group");
}

#[test]
fn the_manual_lists_it_apart_from_the_reading_commands() {
    let manual = asked(&["--help"]);
    let lines: Vec<&str> = manual.lines().collect();
    let at = lines.iter().position(|l| *l == "#### `wxread group leave`").expect("listed");
    let group = lines[..at].iter().rev().find(|l| l.starts_with("### ")).unwrap();
    assert!(group.contains("Leaving a group, which cannot be undone"), "{group}");
    let reading = lines.iter().skip_while(|l| !l.starts_with("### Reading the account"));
    let reading: Vec<&&str> = reading
        .skip(1)
        .take_while(|l| !l.starts_with("### "))
        .filter(|l| l.starts_with("#### "))
        .collect();
    assert!(!reading.is_empty() && reading.iter().all(|l| !l.contains("leave")), "{reading:?}");
}
