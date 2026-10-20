//! The effect layer: reads the world, asks the pure crates what to make of
//! it, and prints the answer (docs/adr/0002-pure-crates-checked-by-flux.md).
//! Every command prints one JSON object on stdout; `--help`, `-h` and
//! `--version` print text.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

pub mod account;
pub mod articles;
pub mod chats;
pub mod contacts;
pub mod doctor;
pub mod file;
pub mod keys;
pub mod media;
mod memory;
pub mod messages;
pub mod resign;
pub mod voice;
pub mod world;

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("WXREAD_VERSION");

/// The guide `--help` prints, and the one screen `-h` prints.
pub const GUIDE: &str = include_str!("help.txt");
pub const SCREEN: &str = include_str!("short.txt");

#[derive(Parser)]
#[command(name = "wxread", version = VERSION, disable_help_flag = true, disable_help_subcommand = true)]
struct Cli {
    /// WeChat's data directory, its xwechat_files (also WXREAD_DATA_DIR)
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    /// The file the keys of the account's databases are in (or their contents as WXREAD_KEYS)
    #[arg(long, global = true, value_name = "FILE")]
    keys: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the articles official accounts sent, newest first, a page at a time
    Articles {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// Only the articles of this official account, by its gh_ id
        #[arg(long, value_name = "ID")]
        official_account: Option<String>,
        /// How many articles a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Write the avatar WeChat holds of a contact, as the image it is
    Avatar {
        /// The contact's id: a wxid, a group's <n>@chatroom, a gh_ official account
        #[arg(value_name = "ID")]
        id: String,
        /// The file to write it to
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
    },
    /// List the account's chats, newest first, a page at a time
    Chats {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// How many chats a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Answer one contact in full, by its id
    Contact {
        /// The contact's id: a wxid, a group's <n>@chatroom, a gh_ official account
        #[arg(value_name = "ID")]
        id: String,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
    },
    /// List the account's contacts by id, a page at a time
    Contacts {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// Only the contacts whose id, WeChat ID, name or remark holds it
        #[arg(long, value_name = "TEXT")]
        query: Option<String>,
        /// How many contacts a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Find WeChat's accounts and the keys supplied; change nothing, name each fix
    Doctor,
    /// Get a media message's file: an image, video, file, sticker or voice message
    File {
        #[command(subcommand)]
        action: FileAction,
    },
    /// Answer a group: its name, notice, owner and every member
    Group {
        /// The group's id: <n>@chatroom
        #[arg(value_name = "ID")]
        id: String,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
    },
    /// Read a chat's messages, each as its kind, newest first, a page at a time
    Messages {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// The chat to read, by its id as `wxread chats` lists it
        #[arg(long, value_name = "ID")]
        chat: Option<String>,
        /// The first time read: RFC 3339, or a date
        #[arg(long, value_name = "TIME")]
        from: Option<String>,
        /// The time reading stops before: RFC 3339, or a date
        #[arg(long, value_name = "TIME")]
        to: Option<String>,
        /// How many messages a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Read every database key from the running WeChat
    Keys,
    /// Re-sign WeChat with get-task-allow so `sudo wxread keys` can read it; only with --yes
    Resign {
        /// The WeChat app to re-sign (/Applications/WeChat.app, else Weixin.app, unless given)
        #[arg(long, value_name = "PATH")]
        app: Option<PathBuf>,
        /// Re-sign it; without this, only say what it would do and change nothing
        #[arg(long)]
        yes: bool,
    },
    /// Write a voice message as a file that plays, and say what WeChat heard in it
    Voice {
        /// The voice message's id, as `wxread messages` answers it
        #[arg(value_name = "ID")]
        id: String,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// The chat the message is in, by its id as `wxread chats` lists it
        #[arg(long, value_name = "ID")]
        chat: Option<String>,
        /// The file to write: a WAV, or with --raw the SILK as WeChat keeps it
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Write the SILK as WeChat keeps it, byte for byte
        #[arg(long)]
        raw: bool,
    },
}

#[derive(Subcommand)]
enum FileAction {
    /// Write a media message's file, or its thumbnail, as the file it is
    Get {
        /// The message's id, as `wxread messages` answers it
        #[arg(value_name = "ID")]
        id: String,
        /// The thumbnail WeChat keeps of an image or a video, not the file
        #[arg(long)]
        thumbnail: bool,
        /// The file to write it to
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
    },
}

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

/// The help file of subcommand `name`: what `wxread <name> --help` prints.
fn help_of(name: &str) -> Option<&'static str> {
    match name {
        "articles" => Some(include_str!("help/articles.txt")),
        "avatar" => Some(include_str!("help/avatar.txt")),
        "file" => Some(include_str!("help/file.txt")),
        "chats" => Some(include_str!("help/chats.txt")),
        "contact" => Some(include_str!("help/contact.txt")),
        "contacts" => Some(include_str!("help/contacts.txt")),
        "doctor" => Some(include_str!("help/doctor.txt")),
        "group" => Some(include_str!("help/group.txt")),
        "keys" => Some(include_str!("help/keys.txt")),
        "messages" => Some(include_str!("help/messages.txt")),
        "resign" => Some(include_str!("help/resign.txt")),
        "voice" => Some(include_str!("help/voice.txt")),
        _ => None,
    }
}

/// The subcommands there are, as the parser knows them.
pub fn commands() -> Vec<String> {
    Cli::command().get_subcommands().map(|c| c.get_name().to_string()).collect()
}

/// Runs the command `args` names: prints its one JSON object, or for a
/// failure `{"error": …}`, and says how it ended.
pub fn main(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    if let Some(text) = help(&args) {
        print!("{text}");
        return ExitCode::SUCCESS;
    }
    let cli = match Cli::try_parse_from(&args) {
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
        Command::Articles { account, official_account, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                articles::run(&w, &articles::Asked { account, official_account, limit, cursor })
            })
        }
        Command::Avatar { id, out, account } => out_of(out).and_then(|out| {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| file::avatar(&w, account.as_deref(), &id, &out))
        }),
        Command::File { action: FileAction::Get { id, thumbnail, out, account } } => out_of(out)
            .and_then(|out| {
                world::World::from_env(cli.data_dir, cli.keys)
                    .and_then(|w| file::get(&w, &file::Asked { id, thumbnail, out, account }))
            }),
        Command::Chats { account, limit, cursor } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| chats::run(&w, &chats::Asked { account, limit, cursor })),
        Command::Contact { id, account } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| contacts::one(&w, account.as_deref(), &id)),
        Command::Contacts { account, query, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                contacts::list(&w, &contacts::Asked { account, query, limit, cursor })
            })
        }
        Command::Doctor => world::World::from_env(cli.data_dir, cli.keys).map(|w| doctor::run(&w)),
        Command::Group { id, account } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| contacts::group(&w, account.as_deref(), &id)),
        Command::Messages { account, chat, from, to, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                messages::run(&w, &messages::Asked { account, chat, from, to, limit, cursor })
            })
        }
        Command::Keys => keys::run(cli.data_dir),
        Command::Resign { app, yes } => resign::run(app, yes),
        Command::Voice { id, account, chat, out, raw } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| voice::run(&w, &voice::Asked { id, account, chat, out, raw }))
        }
    };
    answer(result, 1)
}

/// The help `args` ask for, if they ask for any: `--help` or `-h` before
/// any `--`, of the first subcommand named, or of wxread itself. It is
/// answered before anything is parsed, so a command's help is printed
/// whatever else its line lacks.
fn help(args: &[OsString]) -> Option<&'static str> {
    let words: Vec<&str> =
        args.iter().skip(1).map(|a| a.to_str().unwrap_or("")).take_while(|a| *a != "--").collect();
    let long = words.contains(&"--help");
    if !long && !words.contains(&"-h") {
        return None;
    }
    match words.iter().find_map(|w| help_of(w)) {
        Some(text) => Some(text),
        None if long => Some(GUIDE),
        None => Some(SCREEN),
    }
}

/// Prints `result` as the one JSON object of the run; a failure exits `code`.
fn answer(result: anyhow::Result<Value>, code: u8) -> ExitCode {
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{}", json!({"error": format!("{e:#}")}));
            ExitCode::from(code)
        }
    }
}
