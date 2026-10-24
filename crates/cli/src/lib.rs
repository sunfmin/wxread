//! The effect layer: reads the world, asks the pure crates what to make of
//! it, and prints the answer (docs/adr/0002-pure-crates-checked-by-flux.md).
//! Every command prints one JSON object on stdout; `--help`, `-h`,
//! `--version` and `wxread help` print text.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};
use serde_json::{Value, json};

use command::{Cli, Command, FileAction, GroupAction, Span, Which};

// The binary each topic's `wxread help <topic>` names.
agent_help::binary!("wxread");

pub mod account;
pub mod articles;
mod capture;
pub mod chats;
mod command;
pub mod contacts;
pub mod doctor;
pub mod favorites;
pub mod file;
pub mod keys;
#[cfg(target_os = "macos")]
pub mod leave;
pub mod media;
mod memory;
pub mod messages;
pub mod moments;
pub mod resign;
pub mod search;
pub mod voice;
#[cfg(target_os = "macos")]
mod window;
pub mod world;

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("WXREAD_VERSION");

/// What fixes a command that writes a file and was given no `--out`.
fn out_of(out: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    out.filter(|o| !o.as_os_str().is_empty()).ok_or_else(|| {
        anyhow::anyhow!(
            "no {} was given: pass {} <path>, the file to write it to",
            wxread_media::OUT_FLAG,
            wxread_media::OUT_FLAG
        )
    })
}

/// The manual `--help` answers, assembled from the help written on each
/// command and topic: what `agent_help::check` holds whole.
pub fn manual() -> agent_help::Help {
    command::manual()
}

/// The one screen `-h` answers.
pub fn screen() -> String {
    command::screen()
}

/// Every command there is, as it is run after `wxread`, at every depth:
/// `group leave` after `group`.
pub fn commands() -> Vec<String> {
    fn walk(command: &clap::Command, path: &str, all: &mut Vec<String>) {
        for sub in command.get_subcommands() {
            let path = format!("{path}{}", sub.get_name());
            all.push(path.clone());
            walk(sub, &format!("{path} "), all);
        }
    }
    let mut all = Vec::new();
    walk(&Cli::command(), "", &mut all);
    all
}

/// Runs the command `args` names: prints its one JSON object, or for a
/// failure `{"error": …}`, and says how it ended.
pub fn main(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    let manual = command::manual();
    if let Some(text) = help(&manual, &args) {
        return text_or(text);
    }
    let command = match manual.clap() {
        Ok(command) => command,
        Err(e) => return answer(Err(anyhow::anyhow!("{e}")), 1),
    };
    let parsed = command.try_get_matches_from(&args).and_then(|m| Cli::from_arg_matches(&m));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind::{
                DisplayHelpOnMissingArgumentOrSubcommand, DisplayVersion, MissingSubcommand,
            };
            let message = match e.kind() {
                DisplayVersion => {
                    print!("{e}");
                    return ExitCode::SUCCESS;
                }
                MissingSubcommand | DisplayHelpOnMissingArgumentOrSubcommand => {
                    "no command was given".to_string()
                }
                _ => {
                    // The first line, and when it ends in a colon the
                    // indented lines that finish it (the arguments missing).
                    let message = e.render().to_string();
                    let mut lines = message.lines();
                    let first = lines.next().unwrap_or_default();
                    let first = first.strip_prefix("error: ").unwrap_or(first);
                    match first.strip_suffix(':') {
                        Some(head) => {
                            let rest: Vec<&str> =
                                lines.take_while(|l| l.starts_with("  ")).map(str::trim).collect();
                            format!("{head}: {}", rest.join(", "))
                        }
                        None => first.to_string(),
                    }
                }
            };
            return answer(Err(anyhow::anyhow!("{message}: run `wxread --help`")), 2);
        }
    };
    let result = match cli.command {
        Command::Help { topic: None } => return text_or(manual.manual()),
        Command::Help { topic: Some(topic) } => return text_or(manual.topic_help(&topic)),
        Command::Articles { account: Which { account }, official_account, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                articles::run(&w, &articles::Asked { account, official_account, limit, cursor })
            })
        }
        Command::Avatar { id, out, account: Which { account } } => out_of(out).and_then(|out| {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| file::avatar(&w, account.as_deref(), &id, &out))
        }),
        Command::File {
            action: FileAction::Get { id, thumbnail, out, account: Which { account } },
        } => out_of(out).and_then(|out| {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| file::get(&w, &file::Asked { id, thumbnail, out, account }))
        }),
        Command::Chats { account: Which { account }, with, query, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| chats::run(&w, &chats::Asked { account, with, query, limit, cursor }))
        }
        Command::Contact { id, account: Which { account } } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| contacts::one(&w, account.as_deref(), &id))
        }
        Command::Contacts { account: Which { account }, query, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                contacts::list(&w, &contacts::Asked { account, query, limit, cursor })
            })
        }
        Command::Doctor => world::World::from_env(cli.data_dir, cli.keys).map(|w| doctor::run(&w)),
        Command::Favorites { account: Which { account }, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| favorites::run(&w, &favorites::Asked { account, limit, cursor }))
        }
        Command::Group {
            action: Some(GroupAction::Leave { ids, account: Which { account }, yes, dry_run }),
            ..
        } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| leave(&w, ids, account, yes, dry_run)),
        Command::Group { action: None, id, account: Which { account } } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| contacts::group(&w, account.as_deref(), &id.unwrap_or_default()))
        }
        Command::Messages {
            account: Which { account },
            chat,
            span: Span { from, to },
            limit,
            cursor,
            oldest_first,
        } => world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
            let asked = messages::Asked { account, chat, from, to, limit, cursor, oldest_first };
            messages::run(&w, &asked)
        }),
        Command::Search {
            text,
            account: Which { account },
            chat,
            span: Span { from, to },
            limit,
            cursor,
        } => world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
            let asked = search::Asked { text, account, chat, from, to, limit, cursor };
            search::run(&w, &asked)
        }),
        Command::Keys { seconds } => keys::run(cli.data_dir, seconds),
        Command::Moments { account: Which { account }, contact, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| moments::run(&w, &moments::Asked { account, contact, limit, cursor }))
        }
        Command::Resign { app, yes } => resign::run(app, yes),
        Command::Voice { id, account: Which { account }, chat, out, raw } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| voice::run(&w, &voice::Asked { id, account, chat, out, raw }))
        }
    };
    answer(result, 1)
}

/// What `group leave` answers: on macOS alone, whose window it drives
/// (docs/adr/0001-macos-only-and-apache-2.md).
fn leave(
    w: &world::World,
    ids: Vec<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<Value> {
    #[cfg(target_os = "macos")]
    return leave::run(w, &leave::Asked { ids, account, yes, dry_run });
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (w, ids, account, yes, dry_run);
        anyhow::bail!(
            "`wxread group leave` drives WeChat for Mac's window: run it on the Mac WeChat is logged in on"
        )
    }
}

/// The help `args` ask for, if they ask for any: `--help` or `-h` before
/// any `--`, of the command its words name (each a subcommand of the one
/// before), or of wxread itself: the whole manual for `--help`, one screen
/// for `-h`. It is answered before anything is parsed, so a command's help
/// is printed whatever else its line lacks.
fn help(manual: &agent_help::Help, args: &[OsString]) -> Option<Result<String, agent_help::Error>> {
    let words: Vec<&str> =
        args.iter().skip(1).map(|a| a.to_str().unwrap_or("")).take_while(|a| *a != "--").collect();
    let long = words.contains(&"--help");
    if !long && !words.contains(&"-h") {
        return None;
    }
    let mut command = Cli::command();
    let mut path = Vec::new();
    for word in words {
        if let Some(sub) = command.find_subcommand(word).cloned() {
            path.push(word);
            command = sub;
        }
    }
    Some(match path.as_slice() {
        [] if long => manual.manual(),
        [] => Ok(command::screen()),
        path => manual.command_help(path),
    })
}

/// Prints a text the manual answered, or fails naming what is wrong with
/// what was asked for.
fn text_or(text: Result<String, agent_help::Error>) -> ExitCode {
    match text {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => answer(Err(anyhow::anyhow!("{e}")), 1),
    }
}

/// A failure that answers more than its error: what was done before it,
/// its `error` among its keys.
#[derive(Debug)]
pub struct Failed(pub Value);

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0["error"].as_str().unwrap_or_default())
    }
}

impl std::error::Error for Failed {}

/// Prints `result` as the one JSON object of the run; a failure exits `code`.
fn answer(result: anyhow::Result<Value>, code: u8) -> ExitCode {
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            match e.downcast::<Failed>() {
                Ok(Failed(answer)) => println!("{answer}"),
                Err(e) => println!("{}", json!({"error": format!("{e:#}")})),
            }
            ExitCode::from(code)
        }
    }
}
