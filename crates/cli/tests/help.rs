//! The help: `--help` is the guide an agent learns the tool from, `-h` one
//! screen, and the commands either names are exactly the commands there are.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

use crate::common::World;
use crate::synthetic::{Account, Contact, DATABASES, Group, Said, Session};
use wxread_doctor::{DATA_DIR_FLAG, DATA_DIR_VAR, KEYS_FLAG, KEYS_VAR};

/// What the binary prints for `args`, which must exit 0 and leave stderr empty.
fn asked(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_wxread")).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?} exits 0");
    assert!(out.stderr.is_empty(), "{args:?} prints to stdout only");
    String::from_utf8(out.stdout).unwrap()
}

/// The lines of `text` after the one starting with `from` up to the one
/// starting with `to`.
fn between<'a>(text: &'a str, from: &str, to: &str) -> Vec<&'a str> {
    let after = text.lines().skip_while(|line| !line.starts_with(from)).skip(1);
    after.take_while(|line| !line.starts_with(to)).collect()
}

/// The subcommands the parser has: the ones that exist.
fn existing() -> BTreeSet<String> {
    wxread::commands().into_iter().collect()
}

/// The subcommands the guide sorts into groups: its lines of bare names.
fn grouped(long: &str) -> BTreeSet<String> {
    let lines = between(long, "THE COMMANDS", "RULES").into_iter();
    let names = lines.filter(|line| line.starts_with("      ")).flat_map(str::split_whitespace);
    names.map(str::to_string).collect()
}

/// The subcommands the screen lists: the first word of each line of its
/// `Commands:` block.
fn listed(short: &str) -> BTreeSet<String> {
    let lines = between(short, "Commands:", "Options:").into_iter();
    lines.filter_map(|line| line.split_whitespace().next()).map(str::to_string).collect()
}

/// The word after `wxread ` in every command line `text` shows, an
/// indented line or a quoted one, that could be a command's name.
fn run_in(text: &str) -> BTreeSet<String> {
    let text = text.replace("`wxread ", "\n  wxread ").replace("`sudo wxread ", "\n  wxread ");
    let lines = text.lines().filter(|line| line.starts_with("  "));
    let rest = lines.filter_map(|line| line.trim_start().strip_prefix("wxread "));
    let words = rest.filter_map(|rest| rest.split([' ', '`']).next());
    let words =
        words.filter(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()));
    words.map(str::to_string).collect()
}

#[test]
fn the_guide_and_the_screen_name_exactly_the_commands_there_are() {
    let existing = existing();
    let names =
        ["avatar", "chats", "contact", "contacts", "doctor", "file", "group", "keys", "messages"];
    assert_eq!(existing, names.into_iter().map(String::from).collect());
    let long = asked(&["--help"]);
    let short = asked(&["-h"]);
    assert_eq!(grouped(&long), existing);
    assert_eq!(listed(&short), existing);
    for text in [&long, &short] {
        let run = run_in(text);
        assert!(run.is_subset(&existing), "{run:?} beyond {existing:?}");
    }
}

#[test]
fn the_guide_and_the_screen_are_the_embedded_files() {
    assert_eq!(asked(&["--help"]), wxread::GUIDE);
    assert_eq!(asked(&["-h"]), wxread::SCREEN);
    assert!(wxread::SCREEN.lines().count() <= 24, "-h is one screen");
    for section in ["WHAT IT NEEDS", "RUN FIRST", "WHAT A COMMAND ANSWERS", "THE COMMANDS", "RULES"]
    {
        assert!(wxread::GUIDE.contains(&format!("\n{section}\n")), "the guide says {section}");
    }
}

#[test]
fn each_command_has_its_own_guide_whatever_else_its_line_holds() {
    for command in existing() {
        let long = asked(&[&command, "--help"]);
        assert!(long.contains("WHEN TO RUN IT") && long.contains("WHAT IT ANSWERS"), "{command}");
        assert_eq!(asked(&[&command, "-h"]), long);
        assert_eq!(asked(&["--keys", "/nowhere", &command, "--help", "--nope"]), long);
        let run = run_in(&long);
        assert!(run.is_subset(&existing()), "{command}: {run:?}");
    }
}

#[test]
fn the_help_names_the_options_and_variables_wxread_reads() {
    for text in [asked(&["--help"]), asked(&["-h"]), asked(&["doctor", "--help"])] {
        for name in [DATA_DIR_FLAG, DATA_DIR_VAR, KEYS_FLAG, KEYS_VAR] {
            assert!(text.contains(name), "{name} in {text}");
        }
    }
}

/// The keys `doctor --help` lists under WHAT IT ANSWERS: the first word of
/// each line indented four.
fn answered(command: &str) -> BTreeSet<String> {
    let long = asked(&[command, "--help"]);
    let lines = between(&long, "WHAT IT ANSWERS", "TRIPS YOU UP").into_iter();
    let keys = lines.filter(|line| line.len() - line.trim_start().len() == 4);
    keys.filter_map(|line| line.split_whitespace().next()).map(str::to_string).collect()
}

/// The keys the guide's row for `command` names.
fn guide_row(command: &str) -> BTreeSet<String> {
    let row = between(wxread::GUIDE, "WHAT A COMMAND ANSWERS", "THE COMMANDS")
        .into_iter()
        .find(|line| line.starts_with(&format!("    {command} ")))
        .expect("the guide has a row for the command");
    row.split_whitespace().skip(1).map(str::to_string).collect()
}

#[test]
fn doctor_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let json = World::new().run(&["doctor"], &[]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("doctor"), keys);
    assert_eq!(guide_row("doctor"), keys);
}

#[test]
fn chats_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.sessions = vec![Session("wxid_amy", 1_760_000_000, 0)];
    account.write();
    let json = world.run(&["chats"], &[("WXREAD_KEYS", &account.keys(&DATABASES))]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("chats"), keys);
    assert_eq!(guide_row("chats"), keys);
    let fields: BTreeSet<String> = json["chats"][0].as_object().unwrap().keys().cloned().collect();
    let help = asked(&["chats", "--help"]);
    for field in &fields {
        assert!(
            help.contains(&format!("              {field} ")),
            "chats --help says what {field} is"
        );
    }
}

#[test]
fn messages_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.messages = vec![Said::text("wxid_amy", 1, 1_760_000_000, "wxid_amy", "hi")];
    account.write();
    let json = world
        .run(&["messages", "--chat", "wxid_amy"], &[("WXREAD_KEYS", &account.all_keys())])
        .ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("messages"), keys);
    assert_eq!(guide_row("messages"), keys);
    let help = asked(&["messages", "--help"]);
    let fields: BTreeSet<String> =
        json["messages"][0].as_object().unwrap().keys().cloned().collect();
    for field in fields.iter().filter(|f| *f != "text") {
        assert!(
            help.contains(&format!("              {field} ")),
            "messages --help says what {field} is"
        );
    }
    // Every kind there is, and only those, has its line in the help.
    let kinds: BTreeSet<String> = between(&help, "THE KINDS AND THEIR FIELDS", "TRIPS YOU UP")
        .into_iter()
        .filter(|line| line.len() - line.trim_start().len() == 2)
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| word.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        .map(str::to_string)
        .collect();
    let all: BTreeSet<String> =
        wxread_messages::Kind::ALL.iter().map(|k| k.name().to_string()).collect();
    assert_eq!(kinds, all);
}

/// Every key of each object in `value`, and of the objects and lists of
/// objects inside it, after the first level.
fn fields(value: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut stack: Vec<&Value> = value.as_object().unwrap().values().collect();
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(o) => {
                found.extend(o.keys().cloned());
                stack.extend(o.values());
            }
            Value::Array(a) => stack.extend(a),
            _ => {}
        }
    }
    found
}

#[test]
fn contacts_contact_and_group_answer_exactly_the_keys_their_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts =
        vec![Contact("wxid_amy", "Amy", "Auntie"), Contact("1@chatroom", "Hiking", "")];
    account.groups = vec![Group {
        id: "1@chatroom",
        owner: "wxid_amy",
        members: vec![("wxid_amy", "A")],
        notice: Some(("hi", "wxid_amy", 1_760_000_000)),
    }];
    account.write();
    let keys = account.keys(&DATABASES);
    for args in [&["contacts"][..], &["contact", "wxid_amy"], &["group", "1@chatroom"]] {
        let json = world.run(args, &[("WXREAD_KEYS", &keys)]).ok();
        let top: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
        assert_eq!(answered(args[0]), top, "{args:?}");
        assert_eq!(guide_row(args[0]), top, "{args:?}");
        let help = asked(&[args[0], "--help"]);
        for field in fields(&json) {
            assert!(
                help.contains(&format!("              {field} ")),
                "{} --help says what {field} is",
                args[0]
            );
        }
    }
}

#[test]
fn file_get_and_avatar_answer_exactly_the_keys_their_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy", ""), Contact("wxid_bob", "Bob", "")];
    account.avatars = vec!["wxid_amy"];
    let mut image = Said::text("wxid_amy", 7, 1_760_000_000, "wxid_amy", "<msg><img/></msg>");
    image.local_type = 3;
    image.packed = crate::synthetic::field(3, &crate::synthetic::field(4, b"gone"));
    let mut video = image.clone();
    (video.server_id, video.local_type) = (8, 43);
    video.packed = crate::synthetic::field(4, &crate::synthetic::field(8, b"v"));
    account.messages = vec![image, video];
    account.files = vec![("msg/video/2025-10/v.mp4".into(), b"\0\0\0\x18ftypmp42".to_vec())];
    account.write();
    let keys = account.all_keys();
    let out = world.root.join("out");
    let out = out.to_str().unwrap();
    for (command, args) in [
        ("file", [&["file", "get", "8", "--out", out][..], &["file", "get", "7", "--out", out]]),
        (
            "avatar",
            [&["avatar", "wxid_amy", "--out", out][..], &["avatar", "wxid_bob", "--out", out]],
        ),
    ] {
        let mut keys_answered = BTreeSet::new();
        for args in args {
            let json = world.run(args, &[("WXREAD_KEYS", &keys)]).ok();
            keys_answered.extend(json.as_object().unwrap().keys().cloned());
        }
        assert_eq!(answered(command), keys_answered, "{command}");
        assert_eq!(guide_row(command), keys_answered, "{command}");
    }
}
