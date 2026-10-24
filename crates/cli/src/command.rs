//! The command line wxread parses, and its manual: each command's help is
//! its doc comment, Markdown as written, and [`manual`] assembles them,
//! with the topics and the values the help quotes from code, into what
//! `--help`, `<command> --help` and `wxread help <topic>` answer
//! (docs/adr/0018-the-help-is-one-manual-assembled-where-each-fact-lives.md).

use std::path::PathBuf;

use agent_help::{Help, Topic};
use clap::{CommandFactory, Parser, Subcommand};

/// Who runs each command, the group the manual lists it under.
const SETTING_UP: &str = "Setting up, run by you or an agent";
const READING: &str = "Reading the account, run by you or an agent";
const GETTING_FILES: &str = "Getting a media message's, a favorite's or a Moment's file, or an avatar, run by you or an agent";
const GETTING_KEYS: &str = "Getting the keys, run by the person whose Mac it is, with sudo";
const LETTING_KEYS_READ: &str = "Letting keys read WeChat, run by the person whose Mac it is, once";
const LEAVING: &str = "Leaving a group, which cannot be undone, run by the person whose Mac it is (or an agent they told to), only with --yes";

/// wxread reads WeChat for Mac 4.x directly: each command prints one JSON object; a failure is `{"error": "…"}`, non-zero, naming its fix.
///
/// It reads WeChat's own encrypted databases and files on this Mac, with no server in between. It is read by an agent: every failure names its fix.
///
/// **What it needs**
///
/// - WeChat for Mac 4.x, installed and logged in on this Mac. Its data directory is `~/{const:DATA_DIR}`; pass `--data-dir <dir>` (or set `WXREAD_DATA_DIR`) when WeChat keeps it elsewhere. Each account is a directory there that holds a `{const:DB_STORAGE}`.
/// - The keys of the account's databases. WeChat keys each database apart, and only the running WeChat holds the keys: `wxread keys --help` says how to get them. wxread keeps no keys: store them yourself (a keychain, mytokens) and supply them to every command, with `--keys <file>` or their contents as `WXREAD_KEYS`.
/// - The app running wxread (a terminal, an agent's host) allowed to read other apps' data, which macOS asks for: Full Disk Access.
///
/// **Run first**
///
/// - `wxread doctor`: what it found and what is missing, each with its fix.
/// - `wxread chats`: the account's chats, newest first, once doctor is ok.
/// - `wxread chats --with <id> [--with <id>]`: the chat with someone, and the groups that hold everyone given, smallest first: the way to find "the group of me, A and B".
/// - `wxread contacts`: the account's contacts and groups, with their ids; `--query <name>` finds someone's id.
/// - `wxread messages --chat <id>`: one chat's messages, each as its kind, newest first.
/// - `wxread search <text>`: the messages of every chat that hold a text.
/// - `wxread file get <id> --out <path>`: a media message's file, or a note's item's, a favorite's or a Moment's, written as the file it is.
/// - `wxread favorites`: what the account saved (收藏), each as its kind.
/// - `wxread voice <id> --chat <id> --out <path>`: a voice message as a file that plays, and its 转文字.
///
/// **What a command answers**
///
/// One JSON object on stdout. A failure is `{"error": "…"}` with a non-zero exit (2 for a command line wxread cannot parse, 1 otherwise), and it names every fault with its fix. Warnings go to stderr. Each command's help below says what every key of its answer means.
///
/// `wxread file get` and `wxread avatar` answer path, size, sha256 and content_type of the file they wrote; or, when WeChat does not hold it, missing (true), thumbnail and fix, exit 0. `wxread group leave` that stops at a group, or a `--dry-run` that does not tell every group's chat, answers its keys too, error added, exit 1: the groups it left or told, and those left to do.
///
/// **Rules**
///
/// - wxread reads and never writes WeChat's files, prompts for nothing, elevates nothing and keeps nothing between runs: every run reads the data directory and the keys again. The two exceptions each act only when told to with `--yes`: `wxread resign --yes` re-signs the WeChat app itself, and `wxread group leave <id>... --yes` leaves groups, through WeChat's own window, which cannot be undone.
/// - A database is decrypted into memory and read there: no decrypted byte is written to disk, and what is read is what WeChat wrote last, its WAL included. The one file written is the one `wxread file get`, `wxread avatar` and `wxread voice` are told to write, at `--out`.
/// - With several accounts logged in, a command that reads one is told which: `--account <id>`, as doctor names it.
/// - A contact, a group and a chat are named by their id: a wxid, a group's `<n>@chatroom`, an official account's `gh_...`. `wxread contacts --query <text>` finds an id by a name, a remark or a WeChat ID; `wxread chats --query <text>` finds a chat by its name.
/// - Every person, group, official account and chat an answer names is `{id, display_name}`, the same shape everywhere: [how each is named](help:names).
/// - A list answers a page, and `next`: pass it as `--cursor` for the page after; null on the last.
/// - A finding doctor makes is an answer, exit 0: read its `ok` and `findings`, and the exit code only to tell an answer from an error.
/// - Keys open the databases they were read from: after WeChat creates its databases again (logging out and in, some updates), read them again.
/// - `--help`, `-h`, `--version` and `wxread help` print text, not JSON: they are the only exceptions to one JSON object on stdout. `--help` prints this whole manual, `-h` one screen, `wxread <command> --help` one command's part of it, `wxread help <topic>` one topic.
#[derive(Parser)]
#[command(
    name = "wxread",
    version = crate::VERSION,
    verbatim_doc_comment,
    disable_help_flag = true,
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// WeChat's data directory, its xwechat_files (also WXREAD_DATA_DIR)
    ///
    /// Without it, `WXREAD_DATA_DIR` names it; without that, it is `~/{const:DATA_DIR}`.
    #[arg(long, global = true, value_name = "DIR", verbatim_doc_comment)]
    pub data_dir: Option<PathBuf>,
    /// The file the keys of the account's databases are in (or their contents as WXREAD_KEYS)
    ///
    /// Without it, the keys are read from `WXREAD_KEYS`, which holds the file's contents.
    #[arg(long, global = true, value_name = "FILE", verbatim_doc_comment)]
    pub keys: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Run first: find WeChat's accounts and the keys; change nothing, name each fix.
    ///
    /// **When to run it**
    ///
    /// First, and whenever a command fails for something missing. It reads and changes nothing, so it is always safe.
    ///
    /// **What it reads**
    ///
    /// - WeChat's data directory: `--data-dir`, else `WXREAD_DATA_DIR`, else `~/{const:DATA_DIR}`. Each directory there that holds a `{const:DB_STORAGE}` is an account, and every .db file under its `{const:DB_STORAGE}` is one of its databases.
    /// - The keys: the file `--keys` names, else the contents of `WXREAD_KEYS`.
    ///
    /// It writes nothing, anywhere.
    ///
    /// **What it answers**
    ///
    /// Exit 0 however many findings there are:
    ///
    /// - `ok`: true when there is no finding.
    /// - `data_dir`: `{path, from}`: the data directory, and what named it ("flag", "variable" or "default").
    /// - `accounts`: `[{id, db_storage, databases, keyed, unkeyed}]`: each account, by its directory's name; how many databases it has, how many of them the keys open, and the paths of those they do not.
    /// - `keys`: `{supplied, from, looked, databases}`: whether keys were supplied, from the "flag" or the "variable", the places looked in, and how many databases they open.
    /// - `findings`: `[{problem, fix}]`: everything wrong, each with the line that fixes it.
    /// - `permissions`: `{{const:PERMISSIONS}}`: each `{granted, needed_for, fix}`: whether macOS allows the app that runs it what `wxread group leave` needs, and when it does not, the line that allows it (fix, null when granted). Reading needs neither, so neither is a finding.
    /// - `capture`: `{uv}`: `{found, needed_for, fix}`: whether uv, which `sudo wxread keys` runs Frida through, is installed, and when it is not, the line that installs it. Reading with keys already stored needs no uv, so it is not a finding.
    ///
    /// **Trips you up**
    ///
    /// - The keys are one account's: with two accounts logged in, the other's databases are each named as having no key.
    /// - That macOS refuses reading WeChat's directory is a finding, not an error: its fix is Full Disk Access for the app running wxread.
    /// - The permissions are the app's that runs wxread: run from another terminal or agent host, the answer can differ.
    /// - Only a failure to start at all (HOME unset, a command line wxread cannot parse) is an error: `{"error": …}`, non-zero.
    #[command(next_help_heading = SETTING_UP, verbatim_doc_comment)]
    Doctor,
    /// Show one topic of this manual, or all of it.
    ///
    /// **When to run it**
    ///
    /// When a command's help links a topic, as `[…](help:names)` links how a person is named: `wxread help names` shows it.
    ///
    /// **What it answers**
    ///
    /// The topic's Markdown, as text, not JSON; with no topic, the whole manual, as `wxread --help` does. A topic that does not exist is an error naming those that do.
    #[command(next_help_heading = SETTING_UP, verbatim_doc_comment)]
    Help {
        /// The topic, such as `names`.
        #[arg(value_name = "TOPIC", verbatim_doc_comment)]
        topic: Option<String>,
    },
    /// The account's chats, newest first; `--query`, `--with` find one.
    ///
    /// A page at a time; with `--query`, only the chats whose name holds a text; with `--with`, only the chats that hold the contacts given, the smallest first.
    ///
    /// **When to run it**
    ///
    /// - To see which chats there are and pick one, before reading its messages.
    /// - To find a chat by its name without paging every one: `wxread chats --query <name>`.
    /// - To find the chat with someone, or the group of you, A and B: `wxread chats --with <A's id> --with <B's id>`. `wxread contacts --query <name>` finds each one's id.
    /// - To find the groups that are only ads, to suggest to the account's owner which to leave: a group whose recent.from_top_sender is nearly all of its recent.messages, and whose account_spoke is false. Read a few of its messages (`wxread messages --chat <id>`) before suggesting it; only the owner decides, and `wxread group leave` leaves them.
    ///
    /// It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's session/session.db and contact/contact.db under its db_storage, and, when the page holds a group, its message databases (message/message_<n>.db), each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `chats`: `[{id, display_name, kind, read_with, last_message_at, unread, members, recent, account_spoke}]`, newest first (with `--with`, the smallest first):
    ///   - `id`: its contact's id: a wxid, or a group's `<n>@chatroom`; a folder's id in WeChat's list (see kind).
    ///   - `display_name`: as WeChat shows the chat: the account's remark for it, else its own name; a group with neither is named as WeChat names it, by its members' display names, the account's left out, in the order WeChat lists them, joined by "、" (e.g. "孙凤君、秦俊斌"); "" when the account has no contact of it; a folder by WeChat's name for it.
    ///   - `kind`: "direct" (with one contact), "group", or "folder": an entry of WeChat's list that holds others and has no messages of its own, one of the folders below.
    ///   - `read_with`: the command that reads what it holds: "wxread messages --chat <id>" for a direct chat or a group; for a folder, the command its line below names.
    ///   - `last_message_at`: when its last message was, RFC 3339 in UTC; null for none.
    ///   - `unread`: its messages not yet read.
    ///   - `members`: how many are in it, the account included: {const:DIRECT_MEMBERS} for a direct chat; null for a group WeChat keeps no record of the members of, and for a folder.
    ///   - `recent`: of a group, who sent the most of its {const:RECENT} newest messages this Mac holds: `{messages, top_sender, from_top_sender}`: messages, how many of the {const:RECENT} name their sender (a system message names none); top_sender, who sent the most of those, `{id, display_name}` (of two who sent as many, the one who sent the newer); from_top_sender, how many they sent. null for a group with no such message, a direct chat and a folder.
    ///   - `account_spoke`: of a group, whether the account sent any of its messages this Mac holds: true or false; null for a direct chat and a folder.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **The folders**
    ///
    /// Each by its id, as WeChat names it in its list, then what it holds and the command that reads it:
    ///
    /// {const:FOLDERS}
    ///
    /// **Trips you up**
    ///
    /// - Paging is by where each chat was when the page was read: a chat with a new message since moves to the front, so it is not on a later page, and a fresh run from the newest shows it.
    /// - An official account (`gh_...`) is a "direct" chat.
    /// - A folder is no chat: `wxread messages --chat <folder>` is an error naming its read_with.
    /// - A `--query` is matched against each chat's text as it is; it does not match pinyin. A member's name finds the direct chat with them and every unnamed group named by them, not a group with a name of its own that they are in: `--with <their id>` finds those.
    /// - `--with` takes ids, not names: an id that is no contact is an error naming `wxread contacts --query`, and a group's id is an error naming `wxread group <id>`, which answers its members.
    /// - A group `--with` answers that has no chat in the list (it was deleted from WeChat's list, or never had a message) has last_message_at null and unread 0; `wxread messages --chat <id>` reads what it holds.
    /// - recent and account_spoke count only what WeChat on this Mac holds: a group whose messages were never synced to it has recent null and account_spoke false, whatever was said in it elsewhere.
    /// - The keys must open every database it reads. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Chats {
        #[command(flatten)]
        account: Which,
        /// Only the chats that hold this contact (a person or an official account, by id); again for each one more, and a chat must hold them all.
        ///
        /// With one, the direct chat with them first; then every group they are all members of, the smallest first, newest first among the same size, whether or not the group has a chat in the list.
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        with: Vec<String>,
        /// Only the chats whose id, display_name (as answered: the remark first, and an unnamed group's members' names) or own name (what a contact or group calls itself, remarked or not) holds it, whatever its case; with `--with`, only those of its chats.
        #[arg(long, value_name = "TEXT", verbatim_doc_comment)]
        query: Option<String>,
        /// How many chats a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`, asked with the same `--query` and `--with`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// A chat's messages, each as its kind, a page at a time.
    ///
    /// Newest first or, with `--oldest-first`, from the first forward.
    ///
    /// **When to run it**
    ///
    /// To read a conversation, once `wxread chats` has named the chat. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's message databases under its db_storage (message/message_<n>.db, and message/biz_message_<n>.db for official accounts) and contact/contact.db, for names (and message/media_<n>.db, for whether WeChat holds a voice message's sound), each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page; whether a media message's file and thumbnail are there is read from the files WeChat keeps under the account's directory. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `chat`: the chat read: `{id, display_name}`, as `wxread chats` names it.
    /// - `messages`: `[{id, at, sender, kind, <kind>, type, subtype, content}]`, newest first (oldest first with `--oldest-first`), those sent in one second in the order WeChat shows them:
    ///   - `id`: WeChat's server id of it, a string; null for one the server never saw (some notices).
    ///   - `at`: when it was sent, RFC 3339 in UTC.
    ///   - `sender`: who said it, `{id, display_name}`: id a wxid; display_name what WeChat shows them as: the account's remark for them, else their name in the group, else their own name; "" when nothing names them. A recall's is who recalled it; null for WeChat's other notices.
    ///   - `kind`: what it is, one of the kinds below.
    ///   - `<kind>`: an object named by its kind, of the fields below: "kind": "quote" comes with "quote": {…}.
    ///   - `type`: WeChat's number for it ({const:TEXT_TYPE} text, {const:IMAGE_TYPE} image, {const:APP_TYPE} an app message, {const:NOTICE_TYPE} a notice, …).
    ///   - `subtype`: an app message's own type ({const:WEB_PAGE_SUBTYPE} a web page, {const:QUOTE_SUBTYPE} a quote, …); 0 for none.
    ///   - `content`: what WeChat keeps of it, as it keeps it: a text's text, the XML of the rest; a group's without the "<sender>:\n" WeChat starts it with, decompressed when WeChat compressed it.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **The kinds and their fields**
    ///
    /// Every field is null when the message does not say it. Every person a kind names (a quote's sender, a card's contact, who nudged, joined, left, renamed or posted) is `{id, display_name}`, as sender is: shown as WeChat shows their contact, else by the name the message gives them; its id null when the message names them only by a name. Each media kind (image, video, file, sticker, voice) says whether WeChat on this Mac holds what `wxread file get <id>` writes: file_present its file, true or false (a sticker's, whether it names its address on WeChat's CDN, which is not fetched to tell), and thumbnail_present its thumbnail, an image's or a video's (null for the kinds that have none).
    ///
    /// {const:MESSAGE_KINDS}
    ///
    /// **Trips you up**
    ///
    /// - Messages are newest first: to read a conversation in the order it was said, pass `--oldest-first` and page forward with its `next`; there is no need to fetch every page and reverse it. A `next` keeps its order: one of an `--oldest-first` read given without it, or one of a newest-first read given with it, is an error naming the fix. Newest first, a message sent since the first page is not on a later one; a fresh run from the newest shows it. Oldest first, it is on the last.
    /// - The chat id is exact: a group's ends in @chatroom. A chat none of the account's message databases holds is an error pointing at `wxread chats`. A folder `wxread chats` lists (kind "folder") holds no messages of its own: it is an error naming the command that reads what it holds, its read_with.
    /// - The keys must open contact/contact.db and every message database, and when a page holds a voice message every message/media_<n>.db. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Media is named, not fetched: `wxread file get <id> --out <path>` writes a media message's file, and `wxread file get <file_id>` the file of a note's image or file, by the file_id under note.items (the note's own id is not one). Its file_present says whether WeChat holds it, which it may never have downloaded, or cleared. `wxread voice` writes a voice message as a file that plays.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Messages {
        #[command(flatten)]
        account: Which,
        /// The chat to read, its id as `wxread chats` lists it (a wxid, or a group's `<n>@chatroom`); required.
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        chat: Option<String>,
        #[command(flatten)]
        span: Span,
        /// How many messages a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`, with the same `--chat`, `--from`, `--to` and `--oldest-first`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
        /// Read the chat forward: oldest first, from its first message (from `--from`, when given), each `next` continuing forward; newest first unless given.
        #[arg(long, verbatim_doc_comment)]
        oldest_first: bool,
    },
    /// Find messages across every chat, newest first.
    ///
    /// The messages of every chat that hold a text, a page at a time.
    ///
    /// **When to run it**
    ///
    /// To find what was said without knowing the chat: a word, a link's title, a file's name. It needs the keys supplied, and changes nothing. `wxread messages --chat <id>` then reads the conversation around what it found.
    ///
    /// **What it reads**
    ///
    /// WeChat's own full-text index of the chats (message/message_fts.db under the account's db_storage) when the account has one, the message databases (message/message_<n>.db, and message/biz_message_<n>.db for official accounts) and contact/contact.db, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// What WeChat has indexed is searched in its index, by the text WeChat indexed of each message; what it has not indexed yet (the newest messages, a database it has no range of) is read message by message and searched by the same text, taken from its kind. Without an index, every message is read.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `query`: the text searched for, as given.
    /// - `searched`: how: "{const:BY_INDEX}" when WeChat's index was searched and what it has not indexed yet was read message by message; "{const:BY_MESSAGE}" when the account has no index and every message was read.
    /// - `messages`: `[{chat, matched, id, at, sender, kind, <kind>, type, subtype, content}]`, newest first across every chat:
    ///   - `chat`: the chat it was said in: `{id, display_name}`, as `wxread chats` names it.
    ///   - `matched`: the text it was found in: what WeChat's index holds of it, or the same text taken from its kind.
    ///
    ///   The rest, id, at, sender, kind, <kind>, type, subtype, content, are a message's, as `wxread messages --help` says.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **What a message is searched by**
    ///
    /// The text WeChat indexes of it, by its kind:
    ///
    /// {const:SEARCHED_KINDS}
    ///
    /// The rest are not searched: find them with `wxread messages`:
    ///
    /// {const:UNSEARCHED_KINDS}
    ///
    /// **Trips you up**
    ///
    /// - The text is found inside words, ignoring case: "crag" finds "Crags" and "CRAG". It is one text, not words to find apart.
    /// - Whitespace does not decide a match, in the text or in the message: "15万行" and "15 万行" both find "写了 15 万行" and "写了15万行". Spaces between Chinese and Latin text or digits are put in inconsistently, so none is needed and none is in the way.
    /// - A query that finds nothing is an empty list, exit 0, not an error.
    /// - The chat id is exact: a group's ends in @chatroom. A chat none of the account's message databases holds is an error pointing at `wxread chats`.
    /// - Paging is by where each message was when the page was read: one sent since is not on a later page, and a fresh run from the newest shows it.
    /// - The keys must open contact/contact.db, every message database and, when the account has one, message/message_fts.db. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Search {
        /// What to find, in any case and with or without spaces between its words: a message is found when the text it is searched by holds it; quote it when it holds a space.
        #[arg(value_name = "TEXT", verbatim_doc_comment)]
        text: Option<String>,
        #[command(flatten)]
        account: Which,
        /// Only this chat's messages, its id as `wxread chats` lists it (a wxid, or a group's `<n>@chatroom`).
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        chat: Option<String>,
        #[command(flatten)]
        span: Span,
        /// How many messages a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`, with the same text, `--chat`, `--from` and `--to`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// A voice message written as a file that plays; WeChat's 转文字.
    ///
    /// One voice message written as a file that plays, and what WeChat heard in it.
    ///
    /// **When to run it**
    ///
    /// To listen to a voice message, or to read what it says, once `wxread messages` has answered it as "kind": "voice" with its id. It needs the keys supplied, and changes nothing of WeChat's.
    ///
    /// **What it reads**
    ///
    /// The chat's message databases (message/message_<n>.db, and message/biz_message_<n>.db for official accounts), for the message itself, its contact/contact.db, for the chat's name, and the account's media databases (message/media_<n>.db), where WeChat keeps each voice message's sound by its chat and its id; each with the WAL beside it, with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there.
    ///
    /// **What it writes**
    ///
    /// The one file `--out` names, and nothing else: the sound decoded from WeChat's SILK inside wxread, with no other tool, as a WAV (16-bit mono PCM at {const:SAMPLE_RATE} Hz), which every player plays; with `--raw`, the SILK exactly as WeChat keeps it. A file already there is replaced.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `chat`: the chat read: `{id, display_name}`, as `wxread chats` names it.
    /// - `id`: the message's id, a string.
    /// - `path`: the file written, its absolute path.
    /// - `format`: "wav", or "silk" with `--raw`.
    /// - `duration_ms`: how long the sound lasts, in milliseconds, as decoded; null for a `--raw` SILK that does not decode.
    /// - `size`: the bytes written.
    /// - `length_ms`: how long WeChat says the message is, in milliseconds; null when it does not say.
    /// - `transcription`: the text WeChat heard in it, when the account converted it to text in WeChat (转文字); null when it did not, or WeChat heard nothing. `wxread messages` shows it too, as the voice's transcription.
    ///
    /// **Trips you up**
    ///
    /// - The id is the message's, as `wxread messages` answers it, and only finds it in the chat `--chat` names: an id of another chat, or of a message that is not voice, is an error saying so.
    /// - WeChat keeps the sound of a voice message only once it has it: one not downloaded yet, or one WeChat cleared, is an error whose fix is to play it once in WeChat on this Mac, then run this again.
    /// - wxread does not transcribe: transcription is WeChat's own, made only where the account chose 转文字 on the message in WeChat.
    /// - The keys must open contact/contact.db, every message database and every media database. One missing, or one that no longer opens its database, is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Voice {
        /// The voice message's id, as `wxread messages` answers it.
        #[arg(value_name = "ID", verbatim_doc_comment)]
        id: String,
        #[command(flatten)]
        account: Which,
        /// The chat it is in, its id as `wxread chats` lists it (a wxid, or a group's `<n>@chatroom`); required.
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        chat: Option<String>,
        /// The file to write: a WAV, or with `--raw` the SILK as WeChat keeps it; required.
        #[arg(long, value_name = "PATH", verbatim_doc_comment)]
        out: Option<PathBuf>,
        /// Write the SILK as WeChat keeps it, byte for byte, instead of a WAV.
        #[arg(long, verbatim_doc_comment)]
        raw: bool,
    },
    /// The account's contacts by id; `--query` finds one.
    ///
    /// A page at a time.
    ///
    /// **When to run it**
    ///
    /// To find a contact or a group, and its id, before `wxread contact <id>`, `wxread group <id>` or reading its chat. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's contact/contact.db and head_image/head_image.db under its db_storage, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `contacts`: `[{id, display_name, name, remark, alias, signature, kind, has_avatar}]`, by id:
    ///   - `id`: a wxid, a group's `<n>@chatroom`, or an official account's `gh_...`: what `wxread contact`, `wxread group` and `wxread chats` name it by.
    ///   - `display_name`: as WeChat shows it: the account's remark, else its name; a group with neither is named as WeChat names it, by its members' display names, the account's left out, in the order WeChat lists them, joined by "、" (e.g. "孙凤君、秦俊斌"); "" for none.
    ///   - `name`: what it calls itself; "" for none.
    ///   - `remark`: what the account calls it; null for none.
    ///   - `alias`: the WeChat ID it chose (微信号); null for none.
    ///   - `signature`: its signature; null for none.
    ///   - `kind`: "person", "group" or "official_account".
    ///   - `has_avatar`: whether WeChat holds its avatar.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **Trips you up**
    ///
    /// - The contacts are everyone WeChat knows of the account: friends, its groups, official accounts, and people it met only in a group, the account itself among them.
    /// - In a group a member is shown by their name in it when the account has no remark for them: `wxread group <id>` answers that name.
    /// - A `--query` is matched against each contact as it is, by its text; it does not match pinyin. A group with no name of its own is found by its members' names, the ones its display_name joins.
    /// - The keys must open both databases. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Contacts {
        #[command(flatten)]
        account: Which,
        /// Only the contacts whose id, WeChat ID, name or remark (or, for a group with no name of its own, a member's name it is shown by) holds it, whatever its case.
        #[arg(long, value_name = "TEXT", verbatim_doc_comment)]
        query: Option<String>,
        /// How many contacts a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// One contact in full, by its id.
    ///
    /// **When to run it**
    ///
    /// To see who one contact is: a person, a group or an official account. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's contact/contact.db and head_image/head_image.db under its db_storage, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `contact`: `{id, display_name, name, remark, alias, signature, kind, has_avatar}`:
    ///   - `id`: the id asked for.
    ///   - `display_name`: as WeChat shows it: the account's remark, else its name; a group with neither is named as WeChat names it, by its members' display names, the account's left out, in the order WeChat lists them, joined by "、" (e.g. "孙凤君、秦俊斌"); "" for none.
    ///   - `name`: what it calls itself; "" for none.
    ///   - `remark`: what the account calls it; null for none.
    ///   - `alias`: the WeChat ID it chose (微信号); null for none.
    ///   - `signature`: its signature; null for none.
    ///   - `kind`: "person", "group" or "official_account".
    ///   - `has_avatar`: whether WeChat holds its avatar.
    ///
    /// **Trips you up**
    ///
    /// - An id is matched exactly: a name, a remark or a WeChat ID is not one. An unknown id is an error whose fix is `wxread contacts --query`, which finds it by any of them.
    /// - A group's members and notice are `wxread group <id>`'s.
    /// - The keys must open both databases: one missing is an error naming it, whose fix is `sudo wxread keys`, then supplying the keys again.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Contact {
        /// The contact's id, as `wxread contacts` answers it: a wxid, a group's `<n>@chatroom`, an official account's `gh_...`.
        #[arg(value_name = "ID", verbatim_doc_comment)]
        id: String,
        #[command(flatten)]
        account: Which,
    },
    /// Official accounts' articles, newest first; one account's alone.
    ///
    /// The articles official accounts sent, a page at a time.
    ///
    /// **When to run it**
    ///
    /// To read what the official accounts (公众号) the account follows sent: each article's title, digest, url and cover. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's message/biz_message_<n>.db under its db_storage, WeChat's store of what official accounts sent apart from the chats (every one there is), and its contact/contact.db, for the official accounts' names, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `articles`: `[{official_account, sent_at, title, digest, url, cover}]`, newest first, a push's articles in its order:
    ///   - `official_account`: `{id, display_name}`: the official account's `gh_...` id, and its name as WeChat shows it (the account's remark for it, else its own name, else the name its push gives; "" for none).
    ///   - `sent_at`: when its push was sent, RFC 3339 in UTC; null for no time.
    ///   - `title`: the article's title.
    ///   - `digest`: what the push says it is about; "" for nothing.
    ///   - `url`: where the article is read.
    ///   - `cover`: its cover image's url; "" for none.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **Trips you up**
    ///
    /// - A push of several articles is listed as each of them, sharing its official account and sent_at.
    /// - Only what an official account pushed is listed: its other messages (a text, an image, a reply to the account) are not articles.
    /// - An official account that sent nothing, or an id that is no official account's, is an empty list, exit 0, not an error. So is an account WeChat keeps no store of official accounts for yet.
    /// - `--account` picks the account logged in, as for every command; `--official-account` picks whose articles.
    /// - Paging is by where each article was when the page was read: one pushed since is not on a later page, and a fresh run from the newest shows it.
    /// - The keys must open every one of those databases. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Articles {
        #[command(flatten)]
        account: Which,
        /// Only the articles of this official account, by its `gh_...` id, as `wxread contacts` names it.
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        official_account: Option<String>,
        /// How many articles a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// What the account saved (收藏), each as its kind.
    ///
    /// Newest first, a page at a time.
    ///
    /// **When to run it**
    ///
    /// To read what the account kept in WeChat's Favorites: texts, links, images, videos, files, voice messages, places, chat histories, notes. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's favorite/favorite.db under its db_storage, WeChat's store of its favorites, and its contact/contact.db, for the names of whom a favorite was saved from, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there, after its key is checked against its first page; whether WeChat holds a favorite's file and thumbnail is read from the files it keeps under the account's directory, {const:FAVORITE_SHELF}/. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `favorites`: `[{id, saved_at, kind, <kind>, source, tags, type, content}]`, the newest saved (or edited) first:
    ///   - `id`: WeChat's local id of it, a string.
    ///   - `saved_at`: when it was saved, or last edited (a note), RFC 3339 in UTC.
    ///   - `kind`: what it is, one of the kinds below.
    ///   - `<kind>`: an object named by its kind, of the fields below: "kind": "link" comes with "link": {…}.
    ///   - `source`: `{from, chat, at, message_id, link}`: who said what was saved (in a group the member) and the chat it was said in (a group, else the other person), each `{id, display_name}`: its id, and its name as WeChat shows it (the account's remark, else their own name; "" for none); when it was said (RFC 3339 in UTC); the id of the message it was saved from, as `wxread messages` answers ids; the page it was saved from. Each is null when WeChat does not say it.
    ///   - `tags`: the tags the account gave it, in its order.
    ///   - `type`: WeChat's number for it: {const:FAVORITE_TYPES}.
    ///   - `content`: what WeChat keeps of it, as it keeps it: its `<favitem>` XML.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **The kinds and their fields**
    ///
    /// Every field is null when the favorite does not say it. Each media kind ({const:FAVORITE_MEDIA_KINDS}) names its file and says whether WeChat on this Mac holds it: file_id what `wxread file get <file_id> --out <path>` writes ({const:FAVORITE_FILE}<favorite>:<item>); size its bytes; file_present its file, true or false; thumbnail_present its thumbnail, an image's or a video's (null for the kinds that have none).
    ///
    /// {const:FAVORITE_KINDS}
    ///
    /// **Trips you up**
    ///
    /// - Favorites are newest first, by when they were saved or last edited: an edited note moves to the top. Paging is by where each was when the page was read: one saved since is not on a later page, and a fresh run from the newest shows it.
    /// - Media is named, not fetched: `wxread file get <file_id> --out <path>` writes a favorite's file, unsealed. Its file_present says whether WeChat holds it; WeChat downloads a favorite's file only when it is opened in WeChat's Favorites, so most are not held.
    /// - The keys must open favorite/favorite.db and contact/contact.db. WeChat may hold the key of favorite/favorite.db only once its Favorites (收藏) has been opened: keys without it are an error saying to open Favorites in WeChat and run `sudo wxread keys` again. One that no longer opens its database (WeChat created its databases again: logging out and in, some updates) is an error naming it; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - An account that has no favorite/favorite.db has no favorites: an empty list, exit 0.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Favorites {
        #[command(flatten)]
        account: Which,
        /// How many favorites a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// The Moments WeChat loaded: text, media, likes, comments.
    ///
    /// The Moments (朋友圈) WeChat has loaded on this Mac, newest first, a page at a time.
    ///
    /// **When to run it**
    ///
    /// To read what the account's contacts, and the account itself, posted to Moments: each post's text, location, link and media, and who liked and commented on it. It needs the keys supplied, and changes nothing.
    ///
    /// **Only what WeChat has loaded**
    ///
    /// WeChat for Mac keeps only the Moments it loaded while Moments were viewed on this Mac, and only the photos and videos it loaded then: not everything anyone ever posted, nor everything a contact's Moments hold. To have more listed, scroll further through Moments in WeChat (or a contact's own Moments page), then run this again. Likes and comments are as WeChat last loaded them: those since are not there until WeChat shows the post again.
    ///
    /// **What it reads**
    ///
    /// The account's sns/sns.db under its db_storage, WeChat's store of the Moments it loaded, and its contact/contact.db, for names, each with the WAL beside it (what WeChat wrote last), with their keys: the file `--keys` names, else the contents of `WXREAD_KEYS`. Each is decrypted into memory and read there; no decrypted byte reaches the disk. For each medium it looks for the file WeChat cached under the account's directory (the one above db_storage), in cache/<YYYY-MM>/Sns, and reads none of it.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `moments`: `[{id, author, at, text, location, link, media, likes, comments, type, content}]`, newest first:
    ///   - `id`: WeChat's id of the Moment, digits.
    ///   - `author`: `{id, display_name}`: who posted it, by id, and their name as WeChat shows it (the account's remark for them, else their own name, else the name the Moment gives; "" for none).
    ///   - `at`: when it was posted, RFC 3339 in UTC; null for no time.
    ///   - `text`: what its author wrote; "" for nothing.
    ///   - `location`: `{name, address, city, latitude, longitude}`: where its author tagged it, each "" (or null, for the degrees) when not given; null for no place.
    ///   - `link`: `{title, description, url}`: the article, song or page it shares; null for none.
    ///   - `media`: `[{id, kind, url, thumbnail_url, width, height, file_present, thumbnail_present}]`, in its order:
    ///     - `id`: what `wxread file get` takes for it: <the Moment's id>:<its place, from 0>.
    ///     - `kind`: image, video, or other (a link's cover, a song: no file of it is kept).
    ///     - `url`: where WeChat's CDN served it; "" for none. The CDN's addresses expire, and nothing is fetched from them.
    ///     - `thumbnail_url`: where it served its thumbnail; "" for none.
    ///     - `width`: its width in pixels, when WeChat says; else null.
    ///     - `height`: its height, the same.
    ///     - `file_present`: whether WeChat holds its file on this Mac, which `wxread file get <id>` writes; null for other, and for one whose Moment gives no id of it.
    ///     - `thumbnail_present`: whether WeChat holds its thumbnail (an image's smaller copy, a video's cover), which `wxread file get <id> --thumbnail` writes; null as file_present is.
    ///   - `likes`: `[{by, at}]`: who liked it (`{id, display_name}`, named as author is) and when.
    ///   - `comments`: `[{by, at, text, reply_to}]`: who said what and when; reply_to the `{id, display_name}` of whom it answers, null for none. A deleted comment is not listed.
    ///   - `type`: WeChat's own type of the Moment, a number; null when it gives none.
    ///   - `content`: the Moment as WeChat keeps it: its XML.
    /// - `next`: pass it as `--cursor` for the next page; null on the last one.
    ///
    /// **Trips you up**
    ///
    /// - What is listed is what WeChat loaded, not everything posted: an empty list, or a contact with fewer posts than they made, is WeChat not having shown them on this Mac yet.
    /// - A contact that posted nothing WeChat loaded, an id that is nobody's, and an account WeChat keeps no sns/sns.db for yet, are an empty list, exit 0, not an error.
    /// - A media's file_present false is an answer: `wxread file get <id>` answers it missing, exit 0, with what makes WeChat load it.
    /// - WeChat often caches only an image's thumbnail, the copy it showed in the feed: file_present false with thumbnail_present true is that, and opening the photo in WeChat's Moments caches it whole.
    /// - A Moment whose XML wxread cannot read is still listed, with its id, author and content: read its content.
    /// - Paging is by where each Moment was when the page was read: one WeChat loaded since is not on a later page, and a fresh run from the newest shows it.
    /// - The keys must open sns/sns.db and contact/contact.db. One missing, or one that no longer opens its database (WeChat created its databases again: logging out and in, some updates), is an error naming the database; its fix is `sudo wxread keys`, then supplying the keys again.
    /// - Several accounts logged in is an error naming them: pass `--account`.
    #[command(next_help_heading = READING, verbatim_doc_comment)]
    Moments {
        #[command(flatten)]
        account: Which,
        /// Only the Moments this contact posted, by their id (a wxid, as `wxread contacts` names it); the account's own id for its own.
        #[arg(long, value_name = "ID", verbatim_doc_comment)]
        contact: Option<String>,
        /// How many Moments a page holds, 1 to {const:MAX_LIMIT}; {const:DEFAULT_LIMIT} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        limit: Option<String>,
        /// Start after the page that answered this `next`.
        #[arg(long, value_name = "NEXT", verbatim_doc_comment)]
        cursor: Option<String>,
    },
    /// `file get <id>`: write a message's, note's, favorite's or Moment's file.
    ///
    /// `wxread file get --help` says how.
    #[command(next_help_heading = GETTING_FILES, verbatim_doc_comment)]
    File {
        #[command(subcommand)]
        action: FileAction,
    },
    /// Write a contact's avatar, as the image it is (`--out`).
    ///
    /// **When to run it**
    ///
    /// To see or keep a contact's or a group's avatar, once `wxread contacts` has answered its id and said, in its has_avatar, that WeChat holds one. It needs the keys supplied.
    ///
    /// **What it reads**
    ///
    /// head_image/head_image.db, where WeChat keeps the avatars it shows, and contact/contact.db, each decrypted into memory with its key: the file `--keys` names, else the contents of `WXREAD_KEYS`.
    ///
    /// **What it writes**
    ///
    /// The one file, at `--out`, and nothing anywhere else. A file `--out` already holds with the same bytes is left as it is, so running it again changes nothing.
    ///
    /// **What it answers**
    ///
    /// Written:
    ///
    /// - `path`: where it was written: `--out`, made absolute.
    /// - `size`: its bytes.
    /// - `sha256`: the SHA-256 of them, in lowercase hex.
    /// - `content_type`: what its first bytes say it is: image/jpeg, image/png, …; application/octet-stream for what they do not say.
    ///
    /// When WeChat holds no avatar of the contact, the answer, exit 0:
    ///
    /// - `missing`: true.
    /// - `thumbnail`: null: an avatar has none.
    /// - `fix`: what makes WeChat fetch it: open their profile in WeChat, then run this again.
    ///
    /// **Trips you up**
    ///
    /// - The answer is written or missing: read its `missing` before its `path`; nothing is written when it is missing.
    /// - An id that is no contact of the account, and of which WeChat holds no avatar, is an error pointing at `wxread contacts --query`.
    /// - The keys must open head_image/head_image.db and contact/contact.db; one missing is an error naming it, fixed by `sudo wxread keys`, then supplying the keys again.
    #[command(next_help_heading = GETTING_FILES, verbatim_doc_comment)]
    Avatar {
        /// The contact's id: a wxid, a group's `<n>@chatroom`, an official account's `gh_...`.
        #[arg(value_name = "ID", verbatim_doc_comment)]
        id: String,
        /// The file to write; required. Its directory must exist.
        #[arg(long, value_name = "PATH", verbatim_doc_comment)]
        out: Option<PathBuf>,
        #[command(flatten)]
        account: Which,
    },
    /// Capture every database key from the running WeChat, with Frida (sudo).
    ///
    /// The keys of every database of the logged-in account, captured from the running WeChat as it decrypts its databases.
    ///
    /// **When to run it**
    ///
    /// Once, before any command reads a chat, and again when doctor names a database the keys no longer open. Run it as `sudo wxread keys`: it attaches Frida to WeChat, which only root may do.
    ///
    /// **How a key is captured**
    ///
    /// WeChat for Mac 4.x keeps no database key anywhere a memory scan can find it: the raw {const:KEY_BYTES}-byte key exists only for the instant WeChat hands it to CommonCrypto to decrypt a page. So keys attaches Frida to the running WeChat and hooks those calls, and for every page WeChat decrypts while it runs, captures the key and checks it against a database's first-page HMAC before reporting it — so every key it answers opens its database.
    ///
    /// Two things follow, and both matter:
    ///
    /// - WeChat must be a build Frida can attach to: one that carries get-task-allow. A WeChat installed from Tencent does not, so capture from a re-signed COPY (below) — the installed app is never touched.
    /// - A key is captured only while its database is being decrypted. So, while `sudo wxread keys` runs ({const:KEYS_SECONDS} seconds unless `--seconds` says otherwise), USE WeChat: open several chats and scroll up, type in search, open the contact list, favorites and files. A database nothing touches is not captured; `wxread doctor` then names the ones still unkeyed, and you run `sudo wxread keys` again, using the parts that hold them.
    /// - The one moment that decrypts every database at once is LOGGING IN: a fresh login, and the migration of a phone's history onto the Mac, opens all of them. So the surest single capture is to start the capture at the login screen and log in while it runs: run the copy, leave it at the QR screen, start `sudo wxread keys --seconds 300`, and only then scan the code and log in. Give `--seconds` enough to cover the login and migration.
    ///
    /// As it captures, keys prints each database it keys to stderr as that key is seen ("captured message/message_0.db (3 so far)"), so a long capture counts up as it runs; the keys themselves are the one JSON object on stdout, still printed once at the end. It does not run out the whole window: once the keys stop coming (nothing new for about {const:CAPTURE_IDLE_SECONDS} seconds), the burst is over and it stops and prints what it has. So `--seconds` is a cap, not a wait — set it long enough to cover a slow phone migration; a quick login ends in well under it.
    ///
    /// **What it needs**
    ///
    /// - uv, which keys runs Frida through (it supplies a pinned {const:FRIDA_SPEC}). If it is missing, keys says so and names `{const:UV_INSTALL}`. The first capture downloads Frida through uv; later ones use it from uv's cache.
    /// - The app running wxread granted Full Disk Access.
    /// - A running WeChat with get-task-allow: the re-signed copy.
    ///
    /// **Do it with a copy, never the installed app**
    ///
    /// Re-signing the WeChat in /Applications would strip its genuine signature and can stop it updating. So clone it and work on the clone; the real app keeps running untouched:
    ///
    /// ```sh
    /// cp -c -R /Applications/WeChat.app /tmp/WeChat-copy.app   # APFS clone, ~1s
    /// wxread resign --app /tmp/WeChat-copy.app --yes           # add get-task-allow
    /// pkill -x WeChat                                          # quit the real one
    /// open -n /tmp/WeChat-copy.app                             # run the copy (QR screen)
    /// sudo wxread keys --seconds 300                           # start capturing, THEN
    ///                                                          # log in while it runs
    /// ```
    ///
    /// Start the capture at the copy's login screen and log in while it runs: the login decrypts every database at once (see above). The copy shares the account's data by bundle id, so it logs in to the same account and opens the same chats. Afterwards quit it and reopen /Applications/WeChat.app. Delete the copy when done. `wxread resign --help` says the rest.
    ///
    /// **What it answers**
    ///
    /// One object of each database's path under db_storage to its key, in the shape `wechat-cli init` writes its all_keys.json:
    ///
    /// ```json
    /// {"message/message_0.db": {"enc_key": "<{const:KEY_HEX} hex>", "salt": "<{const:SALT_HEX} hex>"}}
    /// ```
    ///
    /// A key is answered only for a database a captured key opens, so a database WeChat did not decrypt while keys ran is simply absent — browse it and run keys again. With no account logged in, the answer is a failure saying so.
    ///
    /// **Storing and supplying them**
    ///
    /// It keeps no keys of its own. Store the answer where secrets go (a keychain, mytokens), then supply it to every command:
    ///
    /// ```sh
    /// wxread --keys <file> doctor
    /// ```
    ///
    /// or, holding the keys themselves:
    ///
    /// ```sh
    /// WXREAD_KEYS="$(cat <file>)" wxread doctor
    /// ```
    ///
    /// A key stays valid until WeChat creates its databases again (logging out and in, some updates); `wxread doctor` then names each database the keys no longer open, and `sudo wxread keys` captures them again.
    ///
    /// **Trips you up**
    ///
    /// - Run it with sudo, or it refuses: only root may attach Frida to WeChat.
    /// - "Frida could not attach" means the WeChat running has no get-task-allow: run the re-signed copy, not the installed app.
    /// - "no database key was captured" means nothing was decrypted while it ran: browse WeChat during the window, and run it again.
    /// - Under sudo it reads the databases of the user who ran sudo, not root's; pass `--data-dir <dir>` to read another.
    /// - `--seconds` caps how long to capture. It stops early once the keys stop coming, so this is a ceiling, not a wait — raise it (e.g. `--seconds 300`) only to cover a slow first-login migration.
    #[command(next_help_heading = GETTING_KEYS, verbatim_doc_comment)]
    Keys {
        /// How long to capture for, at most: 1 to {const:KEYS_MAX_SECONDS} seconds; {const:KEYS_SECONDS} unless given.
        #[arg(long, value_name = "N", verbatim_doc_comment)]
        seconds: Option<String>,
    },
    /// Re-sign WeChat so keys can read it; changes it only with `--yes`.
    ///
    /// It re-signs WeChat so `sudo wxread keys` can read its memory. Without `--yes` it says what it would do and changes nothing.
    ///
    /// **When to run it**
    ///
    /// Once, when `sudo wxread keys` says Frida cannot attach to WeChat: a WeChat installed from Tencent is signed with the hardened runtime and without get-task-allow, and macOS lets neither Frida attach nor root read it without it. Never on an agent's own initiative.
    ///
    /// **Re-sign a copy, not the installed app**
    ///
    /// Re-signing /Applications/WeChat.app would strip its genuine Tencent signature and can stop it updating. So clone it and re-sign the clone with `--app`; the installed app is never touched and keeps running:
    ///
    /// 1. `cp -c -R /Applications/WeChat.app /tmp/WeChat-copy.app` (APFS clone)
    /// 2. `wxread resign --app /tmp/WeChat-copy.app` (preview)
    /// 3. `wxread resign --app /tmp/WeChat-copy.app --yes` (re-sign it)
    /// 4. quit WeChat (`pkill -x WeChat`), then `open -n /tmp/WeChat-copy.app`, log in
    /// 5. grant the app running wxread (a terminal, an agent's host) Full Disk Access, in System Settings › Privacy & Security › Full Disk Access
    /// 6. run `sudo wxread keys`, and browse the copy while it captures
    ///
    /// The copy shares the account's data by bundle id, so it opens the same chats. Quit it and reopen /Applications/WeChat.app when done; delete the copy. (Without `--app`, resign acts on the installed app in place — undone only by reinstalling WeChat; the copy is why that is not the way to do it.)
    ///
    /// **What it reads and writes**
    ///
    /// The app: `--app <path>`, else {const:APPS}. It asks codesign which entitlements the app is signed with (`codesign -d --entitlements - --xml`) and, with `--yes` only, re-signs it ad-hoc with the same entitlements plus {const:GET_TASK_ALLOW}:
    ///
    /// ```sh
    /// codesign --force --sign - --entitlements <them> <app>
    /// ```
    ///
    /// Nothing else changes: WeChat's data, its databases and what WeChat does are untouched. An app that carries get-task-allow already is not signed again, so running it twice is running it once.
    ///
    /// **Undoing it**
    ///
    /// Reinstall WeChat from Tencent (the App Store, or weixin.qq.com): the reinstalled app is Tencent's signed build again. Re-signing can stop WeChat updating itself, and reinstalling is also how to update it; after an update, run `wxread resign` again.
    ///
    /// **What it answers**
    ///
    /// - `app`: the app it read, as its path.
    /// - `applied`: true when this run re-signed it; false for a preview, and when it carries get-task-allow already.
    /// - `get_task_allow`: whether the app carries get-task-allow now.
    /// - `entitlements`: `{kept, added}`: the entitlements re-signing keeps, by name, and the ones it adds ([] when there is nothing to add).
    /// - `next`: what to do now, in order: without `--yes` first the line that re-signs, then quitting and reopening WeChat, Full Disk Access, `sudo wxread keys`, and how to undo it.
    ///
    /// **Trips you up**
    ///
    /// - Without `--yes` nothing changes: read the answer, then add `--yes`.
    /// - macOS may refuse to change an app in /Applications: run it as `sudo wxread resign --yes`, and if it is still refused, allow the app running wxread in System Settings › Privacy & Security › App Management.
    /// - It needs Apple's codesign: `xcode-select --install` installs it.
    /// - It changes WeChat's signature, not SIP: wxread never asks you to turn System Integrity Protection off.
    #[command(next_help_heading = LETTING_KEYS_READ, verbatim_doc_comment)]
    Resign {
        /// The WeChat app to re-sign: {const:APPS}, unless given.
        #[arg(long, value_name = "PATH", verbatim_doc_comment)]
        app: Option<PathBuf>,
        /// Re-sign it; without this, only say what it would do and change nothing.
        #[arg(long, verbatim_doc_comment)]
        yes: bool,
    },
    /// A group's notice, owner, members; `group leave` leaves groups (`--yes`).
    ///
    /// One group of the account, with its notice, its owner and every member, each named as the group shows them. `wxread group leave <id>...` leaves groups: `wxread group leave --help` says how.
    ///
    /// **When to run it**
    ///
    /// To see who is in a group, what its notice (群公告) says, and what each member is called there. It needs the keys supplied, and changes nothing.
    ///
    /// **What it reads**
    ///
    /// The account's contact/contact.db under its db_storage, with the WAL beside it (what WeChat wrote last), with its key: the file `--keys` names, else the contents of `WXREAD_KEYS`. It is decrypted into memory and read there. It writes nothing, anywhere: no decrypted byte reaches the disk.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `group`: `{id, display_name, name, remark, owner, notice, members}`:
    ///   - `id`: the id asked for.
    ///   - `display_name`: as WeChat shows it: the account's remark, else its name; a group with neither is named as WeChat names it, by its members' display names, the account's left out, in the order WeChat lists them, joined by "、" (e.g. "孙凤君、秦俊斌"); "" for none.
    ///   - `name`: its own name; "" for none: a group with no name of its own answers "" here and its members' names in display_name.
    ///   - `remark`: what the account calls it; null for none.
    ///   - `owner`: `{id, display_name}`: who owns it; null when WeChat does not say.
    ///   - `notice`: `{text, by, at}`: its notice; null for none:
    ///     - `text`: what it says.
    ///     - `by`: `{id, display_name}`: who posted it; null when WeChat does not say.
    ///     - `at`: when, RFC 3339 in UTC; null when WeChat does not say.
    ///   - `members`: `[{id, display_name, name, name_in_group, remark}]`, by id, the account among them:
    ///     - `id`: their id, a wxid.
    ///     - `display_name`: as the group shows them: the account's remark for them, else their name in the group, else their name; "" for none.
    ///     - `name`: what they call themselves; "" when the account has no contact of them.
    ///     - `name_in_group`: the name they set in this group; null for none.
    ///     - `remark`: what the account calls them; null for none.
    ///
    /// **Trips you up**
    ///
    /// - An unknown id is an error whose fix is `wxread contacts --query`; an id that is not a group's is an error whose fix is `wxread contact`.
    /// - A member the account met only in this group is a member still, with a name of "" when WeChat keeps no contact of them.
    /// - A group WeChat keeps no record of (one the account left) answers no owner and no members.
    /// - Its key missing, or no longer opening its database, is an error naming contact/contact.db; its fix is `sudo wxread keys`, then supplying the keys again.
    #[command(
        next_help_heading = READING,
        verbatim_doc_comment,
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true
    )]
    Group {
        #[command(subcommand)]
        action: Option<GroupAction>,
        /// The group's id, `<n>@chatroom`, as `wxread contacts` answers it.
        #[arg(value_name = "ID", required = true, verbatim_doc_comment)]
        id: Option<String>,
        #[command(flatten)]
        account: Which,
    },
}

#[derive(Subcommand)]
pub(crate) enum GroupAction {
    /// Leave groups through WeChat's window, one after another, each checked before and after; only with `--yes`.
    ///
    /// It leaves the account's groups given, one after another, the way a person does, in WeChat's own window. It cannot be undone: to rejoin, a member of the group must invite the account back. Without --yes it says what it would do and changes nothing; with --dry-run it opens each group's chat and says whether it is told to be the group, and leaves none. It stops at the first group it fails to leave; run the same command again and it leaves only the rest.
    ///
    /// **When to run it**
    ///
    /// To leave groups the account no longer wants, found by reading them with wxread (ad groups: `wxread chats` answers of each group who sent most of its newest messages and whether the account spoke in it; then `wxread group <id>`, `wxread messages --chat <id>`). An agent suggests them; the person whose Mac it is picks which to leave. Run by that person, or by an agent they told to leave those groups: first `wxread group leave <id> <id>...` to see what it would do, then the same with `--dry-run` to check every group's chat is told apart, then with `--yes`. The ids can come on stdin, one a line, given as `-`:
    ///
    /// ```sh
    /// … | wxread group leave - [--account <id>] [--dry-run | --yes]
    /// ```
    ///
    /// **What it reads and does**
    ///
    /// - Each group, out of contact/contact.db (its display name, its own name, its members) and its {const:COMPARED} newest messages, out of the message databases, with their keys: the file `--keys` names, else `WXREAD_KEYS`. Every database is read once a run, before the window is touched. A group the account is not in is passed over (not_member): there is nothing to leave. An id that is no group refuses the whole run before anything is touched, every one named at once.
    /// - What stops `--dry-run` and `--yes` before the window is touched, answered as blockers: WeChat not running, or logged in as another account (told by which account's databases the running WeChat has open), and either permission below missing.
    /// - With `--dry-run` or `--yes`, WeChat's window, group after group in the order given, a few seconds each. Once a run, before the first group, it waits for the keyboard and mouse to have been left alone for {const:IDLE_SECONDS} s (at most {const:IDLE_WAIT_SECONDS} s) and brings WeChat to the front, where it stays until the last group is done; then the app that was in front gets it back. For each group: its search is given the group's name through the pasteboard (put back right after), and the chat it lists whose name reads most like it is opened. The name is only searched for: the chat is told to be the group when its title shows the group's member count and most of the group's newest messages are in it, in order (by their text, or, for an image or a sticker, by their sender's name or time). A chat that is not is passed for the next one the search lists, up to {const:MAX_MATCHES}. `--dry-run` clicks nothing more. `--yes` then opens its chat info (···), clicks Leave, and clicks OK once the confirmation quotes the name. Each step is checked on a screenshot of the window, by the text macOS recognizes in it, looked at again until the step's screen comes (a few seconds at most); the screenshot is kept only while it is read, in a directory only you can read. Clear Chat History is left unticked: the chat and its messages stay.
    /// - With `--yes`, then the databases, every {const:POLL_MILLIS} ms for at most {const:LEFT_WAIT_SECONDS} s, until the chat holds the notice WeChat writes when the account leaves (a system message of the account's own, written after OK, about a second after it), or the account is no longer among the group's members: only then does it answer that it left, and go on to the next group.
    ///
    /// **What macOS must allow**
    ///
    /// The app running wxread (a terminal, an agent's host) needs, in System Settings › Privacy & Security:
    ///
    /// - Accessibility, to press WeChat's menus and send its keys and clicks.
    /// - Screen & System Audio Recording (Screen Recording before macOS 15), to take the screenshot of WeChat's window; quit and reopen that app once it is allowed.
    ///
    /// `wxread doctor` says whether each is allowed.
    ///
    /// **What it answers**
    ///
    /// - `account`: the account read: its directory's name, as doctor's id.
    /// - `groups`: `[{id, display_name, name, members, last_message, outcome, confirmed_in, identity, seconds, error, plan}]`, in the order given:
    ///   - `id`: the group's id.
    ///   - `display_name`: what WeChat shows it as.
    ///   - `name`: its own name; "" for none.
    ///   - `members`: how many it had before, the account among them.
    ///   - `last_message`: its newest message as `wxread messages` answers it; null for none.
    ///   - `outcome`: "planned" (without `--yes` or `--dry-run`: it would be left), "not_member" (the account is not in it: nothing to leave), "identified" (`--dry-run` told its chat is the group), "not_identified" (`--dry-run` did not: error says what was read and what did not match), "left" (this run left it), "stopped" (leaving it failed: error says why) or "not_tried" (a group before it stopped the run).
    ///   - `confirmed_in`: the seconds after OK the databases took to show the account left it; null unless left.
    ///   - `identity`: what the chat opened showed of which group it is: `{members_shown (null when none was read), messages_found, messages_compared}`; null unless a chat was opened.
    ///   - `seconds`: how long it took, in seconds: `{search, identify, leave, confirm, total}`; null unless it was tried.
    ///   - `error`: why it was not identified or stopped the run, what was read and what did not match; null for none.
    ///   - `plan`: the steps it takes for it, in order; [] for one not_member.
    /// - `blockers`: `[line]`: what stops `--dry-run` and `--yes` before the window is touched, each with its fix; [] when nothing does, or nothing is to be left.
    /// - `next`: what to do now: after a stop, the command that leaves only the groups still to leave.
    /// - `seconds`: how long the run took, in seconds: `{read (the databases), idle (waiting for the Mac to be left alone), total}`.
    ///
    /// A run that stops at a group, or a `--dry-run` that does not tell every group, answers the same object, error added, and exits 1.
    ///
    /// **Trips you up**
    ///
    /// - Without `--yes` nothing changes: read plan and blockers, run it with `--dry-run`, then add `--yes`. `--dry-run` and `--yes` together are refused.
    /// - It stops at the first group it fails to leave: the groups before it are left, it is stopped, and the ones after it are not tried. Run the same command again (or the one next names) once the cause is fixed: a group the account has left is not_member, so only the rest are left. `--dry-run` tries every group.
    /// - Anything in the window that does not come to read as expected stops it where it is, naming the screen it stopped on and what it read there, with nothing clicked after it; a confirmation already up is cancelled. Running it again with the Mac left alone usually gets past a screen that was still drawing; WeChat itself can always leave the group.
    /// - A chat is told only by what it shows: a group whose newest messages WeChat on this Mac does not hold, or whose chat shows images only with members' names hidden, is not told, and is left in WeChat itself. Groups of the same name are told apart the same way.
    /// - The person at the Mac comes first: it waits for the keyboard and mouse to be left alone, and typing while it runs can stop it.
    /// - Opening the group's chat marks its messages read, as a person opening it would; the chat is left open after.
    /// - WeChat whose screens read otherwise than {const:SCREENS_OF}'s (another version, a new layout) is refused at the step that no longer matches.
    /// - A group with no name of its own is searched for by the names WeChat shows it by.
    /// - An answer that the leave is not confirmed means OK was clicked but the databases still list the account after {const:LEFT_WAIT_SECONDS} s, with no notice of it leaving: read `wxread group <id>` again, and look at WeChat's window.
    #[command(next_help_heading = LEAVING, verbatim_doc_comment)]
    Leave {
        /// Each group's id, `<n>@chatroom`, as `wxread contacts` and `wxread chats` answer it; one given twice is left once. `-` for the ids read from stdin, split by whitespace (one a line), in their place among the others.
        #[arg(value_name = "ID", required = true, verbatim_doc_comment)]
        ids: Vec<String>,
        #[command(flatten)]
        account: Which,
        /// Leave them; without it, nothing is changed.
        #[arg(long, verbatim_doc_comment)]
        yes: bool,
        /// Open each one's chat and say whether it is told to be the group; leave none, every one tried.
        #[arg(long, verbatim_doc_comment)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum FileAction {
    /// Write a media message's, a note's item's, a favorite's or a Moment's file, or its thumbnail, as the file it is.
    ///
    /// One media message's file (an image, a video, a file, a sticker or a voice message), or a note's image or file, or a favorite's, or a Moment's image or video.
    ///
    /// **When to run it**
    ///
    /// To see or keep what a message carries, once `wxread messages` has answered its id and said, in its file_present and thumbnail_present, that WeChat holds it; or what an item of a note sent into a chat holds, once `wxread messages` has answered its file_id under note.items and said the same; or what a favorite holds, once `wxread favorites` has answered its file_id and said the same; or what a Moment shows, once `wxread moments` has answered its media's ids and said the same. It needs the keys supplied.
    ///
    /// **What it reads**
    ///
    /// The message, by its id, in the account's message databases (message/message_<n>.db and message/biz_message_<n>.db), then its file where WeChat for Mac keeps it under the account's directory (the one above db_storage):
    ///
    /// - `image`: msg/attach/<md5 of the chat id>/<YYYY-MM>/Img/<name>_h.dat, else <name>.dat; its thumbnail <name>_t.dat. Sealed: the older XOR format, V1, or V2 under the account's image key, which wxread derives from WeChat's kvcomm files (key_<code>_….statistic, under WeChat's Documents/{const:KVCOMM_DIR}) and the account's id, the one that opens the account's first image. Decrypted in memory.
    /// - `video`: msg/video/<YYYY-MM>/<name>.mp4; its thumbnail <name>_thumb.jpg.
    /// - `file`: msg/file/<YYYY-MM>/<stored_as>, the one of the size the message says when several share its name.
    /// - `voice`: its SILK bytes in message/media_<n>.db.
    /// - `sticker`: fetched from WeChat's CDN at its url, with macOS's own curl, into memory, and checked against its md5.
    ///
    /// A note's item's file (its file_id {const:NOTE_FILE}<message>:<item>): the note's message, by its id, in the message databases, then the file under the note's own directory in the chat's, msg/attach/<md5 of the chat id>/<YYYY-MM>/Rec/<dir> (<dir> named in the message's packed_info_data), by the item's place in the note (0 its first):
    ///
    /// - `image`: Img/<place>; its thumbnail Img/<place>_t. Sealed as a message's image is, decrypted in memory.
    /// - `file`: F/<place>/<its name>; the note's own body (extension htm) Dat/<place>/<its name, else its place>. Not sealed.
    ///
    /// A favorite's file (its file_id {const:FAVORITE_FILE}<favorite>:<item>): the favorite in favorite/favorite.db, then its item's file under the account's directory, {const:FAVORITE_SHELF}/data/<h[..2]>/<h> (its thumbnail {const:FAVORITE_SHELF}/thumb/<h[..2]>/<h>), h the MD5 of the item's id, its MD5 and "_data" ("_thumb"), as WeChat names them. Sealed as an image is (V1, or V2 under the account's image key), whatever file it is, and unsealed in memory; one WeChat did not seal is written as it is.
    ///
    /// A Moment's medium, by its id (<the Moment's id>:<its place>), in the account's sns/sns.db, then its file WeChat cached while Moments were viewed, under the account's directory, k the MD5 in lowercase hex of <the Moment's id>_<the medium's id>_<n>, as the Moment's XML gives the two ids, in any month (WeChat files it under the month it was posted):
    ///
    /// - `image`: cache/<YYYY-MM>/Sns/Img/<k[..2]>/<k[2..]>, n {const:MOMENT_IMAGE}; its thumbnail n {const:MOMENT_IMAGE_THUMBNAILS}, the smaller copies WeChat keeps. Sealed as a message's image is.
    /// - `video`: cache/<YYYY-MM>/Sns/Video/<k[..2]>/<k[2..]>.mp4, n {const:MOMENT_VIDEO}; its thumbnail the .jpg beside it. Not sealed.
    ///
    /// Each database is decrypted into memory, with its key: the file `--keys` names, else the contents of `WXREAD_KEYS`.
    ///
    /// **What it writes**
    ///
    /// The one file, at `--out`, and nothing anywhere else: a decrypted image or a fetched sticker is never written but there. A file `--out` already holds with the same bytes is left as it is, so running it again changes nothing.
    ///
    /// **What it answers**
    ///
    /// Written:
    ///
    /// - `path`: where it was written: `--out`, made absolute.
    /// - `size`: its bytes.
    /// - `sha256`: the SHA-256 of them, in lowercase hex.
    /// - `content_type`: what its first bytes say it is: image/jpeg, image/png, image/gif, image/wxgf (WeChat's own image format, which few viewers read), video/mp4, audio/silk, application/pdf, …; application/octet-stream for what they do not say.
    ///
    /// When WeChat does not hold the file (it holds only the thumbnail, or cleared it, or never downloaded it), the answer, exit 0:
    ///
    /// - `missing`: true.
    /// - `thumbnail`: the thumbnail written at `--out` in its place, as `{path, size, sha256, content_type}`, when WeChat holds one; else null, and nothing is written.
    /// - `fix`: what makes WeChat fetch it: open the message (for a note's item, the note's message; for a favorite's, the favorite, in WeChat's Favorites) in WeChat, which downloads it, then run this again; for a Moment's, open the Moment in WeChat's Moments and let it load.
    ///
    /// **Trips you up**
    ///
    /// - The answer is written or missing: read its `missing` before its `path`. A missing image's `--out` holds its thumbnail, not the image.
    /// - Only images and videos have thumbnails: `--thumbnail` on another kind is an error saying to leave it out.
    /// - A message that carries no file (a text, a link, …) is an error naming its kind; so is a favorite's item that carries none, and a Moment's medium of kind other.
    /// - A note's body (extension htm), a favorite's or one sent into a chat, is HTML WeChat wrote without a header, so its content_type is application/octet-stream.
    /// - A note's own id is not a file's: `wxread file get <its id>` is an error saying to pass one of its items' file_id. A note's video or voice message has none: where WeChat keeps it is not known.
    /// - Images sealed under the account's key need the kvcomm files WeChat for Mac writes on this Mac: with none that opens them, the error says to log in to WeChat for Mac here and run again.
    /// - A sticker is fetched from WeChat's CDN, so it needs the network; one the CDN no longer serves is missing.
    /// - A voice message is SILK (audio/silk), as WeChat records it: convert it with a SILK decoder to play it elsewhere.
    /// - The keys must open every message database, and for a voice message every message/media_<n>.db, or for a favorite's file favorite/favorite.db, or for a Moment's medium sns/sns.db and contact/contact.db; one missing is an error naming it, fixed by `sudo wxread keys`, then supplying the keys again.
    #[command(verbatim_doc_comment)]
    Get {
        /// The message's id, its "id" as `wxread messages` answers it; a note's item's file_id as `wxread messages` answers it under note.items ({const:NOTE_FILE}<message>:<item>); a favorite's file_id as `wxread favorites` answers it ({const:FAVORITE_FILE}<favorite>:<item>); or a Moment's medium's, its "id" under media as `wxread moments` answers it.
        #[arg(value_name = "ID", verbatim_doc_comment)]
        id: String,
        /// The thumbnail WeChat keeps of an image or a video, instead of its file.
        #[arg(long, verbatim_doc_comment)]
        thumbnail: bool,
        /// The file to write; required. Its directory must exist.
        #[arg(long, value_name = "PATH", verbatim_doc_comment)]
        out: Option<PathBuf>,
        #[command(flatten)]
        account: Which,
    },
}

/// The account a command reads, when several are logged in.
#[derive(clap::Args)]
pub(crate) struct Which {
    /// The account to read, as `wxread doctor` names it; with only one logged in, that one.
    #[arg(long, value_name = "ID", verbatim_doc_comment)]
    pub account: Option<String>,
}

/// The times a command reads the messages between.
#[derive(clap::Args)]
pub(crate) struct Span {
    /// The first time read, itself included.
    ///
    /// A time is RFC 3339 with its offset (2026-10-20T09:30:00+08:00, 2026-10-20T01:30:00Z), or a date (2026-10-20), its midnight in UTC.
    #[arg(long, value_name = "TIME", verbatim_doc_comment)]
    pub from: Option<String>,
    /// The time reading stops before, itself left out: a time as `--from` takes it.
    #[arg(long, value_name = "TIME", verbatim_doc_comment)]
    pub to: Option<String>,
}

/// Every person, group, official account and chat an answer names is `{id, display_name}`, the same shape everywhere: a chat listed, a message's sender, a Moment's author, an article's official account, a group's owner, a favorite's source, who joined, who was nudged, ….
///
/// - `id` is the contact's id as `wxread contacts` answers it: a wxid, a group's `<n>@chatroom`, an official account's `gh_...`; null when WeChat gives only a name.
/// - `display_name` is the name WeChat shows them by, as `wxread contacts` answers it: the account's remark, else (in a group) their name in it, else their own name, else the name the message or post gives; a group with no name of its own by its members' names, joined by "、"; "" for none.
///
/// A remarked contact is shown by the remark everywhere.
#[derive(Topic)]
#[topic(name = "names")]
pub(crate) struct Names;

/// The manual: every command's help, the topics, and each value the help
/// quotes from code, under the name its `{const:…}` gives.
pub(crate) fn manual() -> Help {
    use wxread_favorites::Kind as Favorite;
    use wxread_messages::Kind as Message;
    let listed = |items: Vec<(&str, String)>| {
        items
            .into_iter()
            .map(|(name, help)| format!("- `{name}`: {help}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let searched = |want: bool| {
        listed(
            Message::ALL
                .into_iter()
                .filter_map(|k| match crate::search::searched_by(k) {
                    Ok(by) if want => Some((k.name(), by.to_string())),
                    Err(what) if !want => Some((k.name(), what.to_string())),
                    _ => None,
                })
                .collect(),
        )
    };
    let folders = wxread_chats::Folder::ALL.into_iter().map(|f| {
        let held = crate::chats::folder_help(f);
        (f.id(), format!("\"{}\", {held}; read with `{}`.", f.name(), f.read_with()))
    });
    Help::new(Cli::command())
        .topic::<Names>()
        .constant("DATA_DIR", wxread_doctor::DATA_DIR_UNDER_HOME)
        .constant("DB_STORAGE", wxread_doctor::DB_STORAGE)
        .constant("MAX_LIMIT", wxread_answer::MAX_LIMIT)
        .constant("DEFAULT_LIMIT", wxread_answer::DEFAULT_LIMIT)
        .constant("RECENT", wxread_messages::RECENT)
        .constant("DIRECT_MEMBERS", wxread_chats::DIRECT_MEMBERS)
        .constant("FOLDERS", listed(folders.collect()))
        .constant("TEXT_TYPE", wxread_messages::kinds::TEXT)
        .constant("IMAGE_TYPE", wxread_messages::kinds::IMAGE)
        .constant("APP_TYPE", wxread_messages::kinds::APP)
        .constant("NOTICE_TYPE", wxread_messages::kinds::SYSTEM)
        .constant("WEB_PAGE_SUBTYPE", wxread_messages::kinds::WEB_PAGE)
        .constant("QUOTE_SUBTYPE", wxread_messages::kinds::QUOTE)
        .constant(
            "MESSAGE_KINDS",
            listed(
                Message::ALL
                    .into_iter()
                    .map(|k| (k.name(), crate::messages::kind_help(k)))
                    .collect(),
            ),
        )
        .constant("SEARCHED_KINDS", searched(true))
        .constant("UNSEARCHED_KINDS", searched(false))
        .constant("BY_INDEX", wxread_search::BY_INDEX)
        .constant("BY_MESSAGE", wxread_search::BY_MESSAGE)
        .constant(
            "FAVORITE_KINDS",
            listed(
                Favorite::ALL
                    .into_iter()
                    .map(|k| (k.name(), crate::favorites::kind_help(k).to_string()))
                    .collect(),
            ),
        )
        .constant(
            "FAVORITE_MEDIA_KINDS",
            Favorite::ALL
                .into_iter()
                .filter(|k| k.is_media())
                .map(Favorite::name)
                .collect::<Vec<_>>()
                .join(", "),
        )
        .constant("FAVORITE_TYPES", crate::favorites::numbered(&wxread_favorites::TYPES))
        .constant("FAVORITE_FILE", wxread_favorites::FILE_PREFIX)
        .constant("FAVORITE_SHELF", wxread_favorites::SHELF)
        .constant("NOTE_FILE", wxread_messages::note::FILE_PREFIX)
        .constant("MOMENT_IMAGE", wxread_media::MOMENT_IMAGE)
        .constant(
            "MOMENT_IMAGE_THUMBNAILS",
            wxread_media::MOMENT_IMAGE_THUMBNAILS.map(|n| n.to_string()).join(", else n "),
        )
        .constant("MOMENT_VIDEO", wxread_media::MOMENT_VIDEO)
        .constant("KVCOMM_DIR", wxread_media::image::KVCOMM_DIRS[0])
        .constant("SAMPLE_RATE", wxread_voice::SAMPLE_RATE)
        .constant(
            "PERMISSIONS",
            wxread_doctor::Permission::ALL.map(wxread_doctor::Permission::name).join(", "),
        )
        .constant("KEYS_SECONDS", crate::keys::DEFAULT_SECONDS)
        .constant("KEYS_MAX_SECONDS", crate::keys::MAX_SECONDS)
        .constant("CAPTURE_IDLE_SECONDS", crate::capture::IDLE_STOP_SECONDS)
        .constant("FRIDA_SPEC", crate::capture::FRIDA_SPEC)
        .constant("UV_INSTALL", crate::capture::UV_INSTALL)
        .constant("KEY_BYTES", wxread_keys::KEY_BYTES)
        .constant("KEY_HEX", wxread_keys::KEY_HEX)
        .constant("SALT_HEX", wxread_keys::SALT_HEX)
        .constant("APPS", wxread_resign::APPS.join(", else "))
        .constant("GET_TASK_ALLOW", wxread_resign::GET_TASK_ALLOW)
        .constant("COMPARED", wxread_leave::COMPARED)
        .constant("IDLE_SECONDS", wxread_leave::IDLE_SECONDS)
        .constant("IDLE_WAIT_SECONDS", wxread_leave::IDLE_WAIT_SECONDS)
        .constant("MAX_MATCHES", wxread_leave::MAX_MATCHES)
        .constant("POLL_MILLIS", wxread_leave::POLL_MILLIS)
        .constant("LEFT_WAIT_SECONDS", wxread_leave::LEFT_WAIT_SECONDS)
        .constant("SCREENS_OF", wxread_leave::SCREENS_OF)
}

/// What `-h` prints: one screen, of wxread's first line, its usage, each
/// command's first line and the options every command takes.
pub(crate) fn screen() -> String {
    let cli = Cli::command();
    let about = |c: &clap::Command| c.get_about().map(|a| a.to_string()).unwrap_or_default();
    let globals: Vec<&clap::Arg> = cli.get_arguments().filter(|a| a.is_global_set()).collect();
    let flag = |a: &clap::Arg| {
        let value = a.get_value_names().unwrap_or_default().join(" ").to_lowercase();
        format!("--{} <{value}>", a.get_long().unwrap_or_default())
    };
    let usage: Vec<String> = globals.iter().map(|a| format!("[{}]", flag(a))).collect();
    let mut screen =
        format!("{}\nUsage: wxread {} <command>\nCommands:\n", about(&cli), usage.join(" "));
    let commands: Vec<&clap::Command> = cli.get_subcommands().collect();
    let width = commands.iter().map(|c| c.get_name().len()).max().unwrap_or_default();
    for c in commands {
        screen.push_str(&format!("  {:width$} {}\n", c.get_name(), about(c)));
    }
    let options: Vec<(String, String)> = globals
        .iter()
        .map(|a| (flag(a), a.get_help().map(|h| h.to_string()).unwrap_or_default()))
        .chain([
            (
                "-h, --help".into(),
                "this screen; --help the whole manual; --version the version".into(),
            ),
            ("<command> --help".into(), "what a command reads and answers".into()),
        ])
        .collect();
    let width = options.iter().map(|(o, _)| o.len()).max().unwrap_or_default();
    screen.push_str("Options:\n");
    for (option, help) in options {
        screen.push_str(&format!("  {option:width$}  {help}\n"));
    }
    screen
}
