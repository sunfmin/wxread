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
mod capture;
pub mod chats;
pub mod contacts;
pub mod doctor;
pub mod favorites;
pub mod file;
pub mod keys;
#[cfg(target_os = "macos")]
pub mod leave;
pub mod media;
#[cfg(target_os = "macos")]
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

/// The guide `--help` prints, and the one screen `-h` prints: the Windows
/// text names this platform's data directory and the keys it must be given.
#[cfg(target_os = "windows")]
pub const GUIDE: &str = include_str!("help-windows.txt");
#[cfg(not(target_os = "windows"))]
pub const GUIDE: &str = include_str!("help.txt");
#[cfg(target_os = "windows")]
pub const SCREEN: &str = include_str!("short-windows.txt");
#[cfg(not(target_os = "windows"))]
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
        /// Only the chats that hold this contact, by id; again for each one more, smallest first
        #[arg(long, value_name = "ID")]
        with: Vec<String>,
        /// Only the chats whose id, name or own name holds it, whatever its case
        #[arg(long, value_name = "TEXT")]
        query: Option<String>,
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
    /// List what the account saved (收藏), each as its kind, newest first, a page at a time
    Favorites {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// How many favorites a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Get a media message's or a favorite's file: an image, video, file, sticker or voice message
    File {
        #[command(subcommand)]
        action: FileAction,
    },
    /// Answer a group: its name, notice, owner and every member; `group leave` leaves it
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Group {
        #[command(subcommand)]
        action: Option<GroupAction>,
        /// The group's id: <n>@chatroom
        #[arg(value_name = "ID", required = true)]
        id: Option<String>,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
    },
    /// Read a chat's messages, each as its kind, newest first (or oldest), a page at a time
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
        /// Read the chat forward, from its first message: oldest first
        #[arg(long)]
        oldest_first: bool,
    },
    /// Find messages across every chat, newest first, a page at a time
    Search {
        /// The text to find, in any case and with or without spaces between its words
        #[arg(value_name = "TEXT")]
        text: Option<String>,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// Only this chat's messages, by its id as `wxread chats` lists it
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
    /// Capture every database key from the running WeChat with Frida (macOS only)
    Keys {
        /// How long to capture for, 1 to 600 seconds (25 unless given)
        #[arg(long, value_name = "N")]
        seconds: Option<String>,
    },
    /// List the Moments WeChat has loaded, newest first, a page at a time
    Moments {
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// Only the Moments this contact posted, by their id
        #[arg(long, value_name = "ID")]
        contact: Option<String>,
        /// How many Moments a page holds, 1 to 1000 (100 unless given)
        #[arg(long, value_name = "N")]
        limit: Option<String>,
        /// Where the page starts: the `next` of the answer before
        #[arg(long, value_name = "NEXT")]
        cursor: Option<String>,
    },
    /// Re-sign WeChat for Mac with get-task-allow, macOS only; only with --yes
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
enum GroupAction {
    /// Leave groups through WeChat for Mac's window, macOS only; only with --yes
    Leave {
        /// Each group's id: <n>@chatroom; - for the ids on stdin
        #[arg(value_name = "ID", required = true)]
        ids: Vec<String>,
        /// The account to read, by its id, when several are logged in
        #[arg(long, value_name = "ID")]
        account: Option<String>,
        /// Leave them; without this, only say what it would do and change nothing
        #[arg(long)]
        yes: bool,
        /// Open each one's chat and say whether it is told to be the group; never leave
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum FileAction {
    /// Write a media message's or a Moment's file, or its thumbnail, as the file it is
    Get {
        /// The message's id, as `wxread messages` answers it, a favorite's file_id, or a Moment's medium's, as `wxread moments` does
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
        "doctor" => Some(doctor_help()),
        "favorites" => Some(include_str!("help/favorites.txt")),
        "group" => Some(include_str!("help/group.txt")),
        "group leave" => Some(leave_help()),
        "keys" => Some(keys_help()),
        "messages" => Some(include_str!("help/messages.txt")),
        "moments" => Some(include_str!("help/moments.txt")),
        "resign" => Some(resign_help()),
        "search" => Some(include_str!("help/search.txt")),
        "voice" => Some(include_str!("help/voice.txt")),
        _ => None,
    }
}

/// The help of `doctor`: on Windows it names this platform's data directory
/// and the empty permissions map.
#[cfg(target_os = "windows")]
fn doctor_help() -> &'static str {
    include_str!("help/doctor-windows.txt")
}
#[cfg(not(target_os = "windows"))]
fn doctor_help() -> &'static str {
    include_str!("help/doctor.txt")
}

/// The help of `keys`: on Windows it says capture is not available and how
/// to supply keys instead.
#[cfg(target_os = "windows")]
fn keys_help() -> &'static str {
    include_str!("help/keys-windows.txt")
}
#[cfg(not(target_os = "windows"))]
fn keys_help() -> &'static str {
    include_str!("help/keys.txt")
}

/// The help of `resign`, which only a Mac build can do.
#[cfg(target_os = "windows")]
fn resign_help() -> &'static str {
    include_str!("help/resign-windows.txt")
}
#[cfg(not(target_os = "windows"))]
fn resign_help() -> &'static str {
    include_str!("help/resign.txt")
}

/// The help of `group leave`, which only a Mac build can do.
#[cfg(target_os = "windows")]
fn leave_help() -> &'static str {
    include_str!("help/group-leave-windows.txt")
}
#[cfg(not(target_os = "windows"))]
fn leave_help() -> &'static str {
    include_str!("help/group-leave.txt")
}

/// The subcommands there are, as the parser knows them.
pub fn commands() -> Vec<String> {
    Cli::command().get_subcommands().map(|c| c.get_name().to_string()).collect()
}

/// The subcommands of subcommand `command`, as the parser knows them:
/// `group leave` is `group`'s.
pub fn subcommands(command: &str) -> Vec<String> {
    let cli = Cli::command();
    let of = cli.get_subcommands().find(|c| c.get_name() == command);
    of.map(|c| c.get_subcommands().map(|s| s.get_name().to_string()).collect()).unwrap_or_default()
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
        Command::Chats { account, with, query, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| chats::run(&w, &chats::Asked { account, with, query, limit, cursor }))
        }
        Command::Contact { id, account } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| contacts::one(&w, account.as_deref(), &id)),
        Command::Contacts { account, query, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                contacts::list(&w, &contacts::Asked { account, query, limit, cursor })
            })
        }
        Command::Doctor => world::World::from_env(cli.data_dir, cli.keys).map(|w| doctor::run(&w)),
        Command::Favorites { account, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| favorites::run(&w, &favorites::Asked { account, limit, cursor }))
        }
        Command::Group {
            action: Some(GroupAction::Leave { ids, account, yes, dry_run }), ..
        } => world::World::from_env(cli.data_dir, cli.keys)
            .and_then(|w| leave(&w, ids, account, yes, dry_run)),
        Command::Group { action: None, id, account } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| contacts::group(&w, account.as_deref(), &id.unwrap_or_default()))
        }
        Command::Messages { account, chat, from, to, limit, cursor, oldest_first } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                let asked =
                    messages::Asked { account, chat, from, to, limit, cursor, oldest_first };
                messages::run(&w, &asked)
            })
        }
        Command::Search { text, account, chat, from, to, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys).and_then(|w| {
                let asked = search::Asked { text, account, chat, from, to, limit, cursor };
                search::run(&w, &asked)
            })
        }
        Command::Keys { seconds } => keys::run(cli.data_dir, seconds),
        Command::Moments { account, contact, limit, cursor } => {
            world::World::from_env(cli.data_dir, cli.keys)
                .and_then(|w| moments::run(&w, &moments::Asked { account, contact, limit, cursor }))
        }
        Command::Resign { app, yes } => resign::run(app, yes),
        Command::Voice { id, account, chat, out, raw } => {
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
    let named = words.windows(2).find_map(|pair| help_of(&pair.join(" ")));
    match named.or_else(|| words.iter().find_map(|w| help_of(w))) {
        Some(text) => Some(text),
        None if long => Some(GUIDE),
        None => Some(SCREEN),
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
