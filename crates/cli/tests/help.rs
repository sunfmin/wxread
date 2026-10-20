//! The help: `--help` is the guide an agent learns the tool from, `-h` one
//! screen, and the commands either names are exactly the commands there are.

use std::collections::BTreeSet;
use std::process::Command;

use crate::common::World;
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
    assert_eq!(existing, BTreeSet::from(["doctor".to_string(), "keys".to_string()]));
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

#[test]
fn doctor_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let json = World::new().run(&["doctor"], &[]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("doctor"), keys);
    let row = between(wxread::GUIDE, "WHAT A COMMAND ANSWERS", "THE COMMANDS")
        .into_iter()
        .find(|line| line.starts_with("    doctor "))
        .expect("the guide has a row for doctor");
    let row: BTreeSet<String> = row.split_whitespace().skip(1).map(str::to_string).collect();
    assert_eq!(row, keys);
}
