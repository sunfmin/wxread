//! The help: `--help` is the manual an agent learns the tool from, one
//! Markdown text assembled from the help written on each command; `-h` is
//! one screen; `<command> --help` is that command's part of the manual; and
//! the commands either names are exactly the commands there are.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::{Value, json};

use crate::common::World;
use crate::synthetic::{
    self, Account, Article, Contact, DATABASES, Group, Posted, Said, Saved, Session, Voice,
};
use wxread_doctor::{DATA_DIR_FLAG, DATA_DIR_VAR, KEYS_FLAG, KEYS_VAR};
use wxread_keys::Key;

/// What the binary prints for `args`, which must exit 0 and leave stderr empty.
fn asked(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_wxread")).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?} exits 0");
    assert!(out.stderr.is_empty(), "{args:?} prints to stdout only");
    String::from_utf8(out.stdout).unwrap()
}

/// What `wxread <command> --help` prints, `command` the words of a command
/// as `wxread::commands` names it.
fn help_of(command: &str) -> String {
    let mut args: Vec<&str> = command.split(' ').collect();
    args.push("--help");
    asked(&args)
}

/// The lines of the part of `text` under its label `**<label>**`, up to the
/// next label or heading.
fn part<'a>(text: &'a str, label: &str) -> Vec<&'a str> {
    let label = format!("**{label}**");
    let after = text.lines().skip_while(|line| *line != label).skip(1);
    after.take_while(|line| !is_label(line) && !line.starts_with('#')).collect()
}

/// Whether `line` is a label, `**<label>**`, that a part starts with.
fn is_label(line: &str) -> bool {
    line.len() > 4 && line.starts_with("**") && line.ends_with("**")
}

/// The name of each list item `depth` levels deep in `lines`, its first
/// word in backticks: `- \`name\`: …`.
fn items(lines: &[&str], depth: usize) -> BTreeSet<String> {
    let marker = format!("{}- `", "  ".repeat(depth));
    let rest = lines.iter().filter_map(|line| line.strip_prefix(&marker));
    rest.filter_map(|rest| rest.split('`').next()).map(str::to_string).collect()
}

/// The keys `command`'s help lists under What it answers, `depth` levels
/// deep: 0 for the keys of its answer, 1 for theirs.
fn answered(command: &str, depth: usize) -> BTreeSet<String> {
    let help = help_of(command);
    items(&part(&help, "What it answers"), depth)
}

/// The keys of `value`, an object.
fn keys_of(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

/// The commands there are, at the top: the ones the screen lists.
fn existing() -> BTreeSet<String> {
    wxread::commands().into_iter().filter(|c| !c.contains(' ')).collect()
}

/// The word after `wxread ` in every command `text` quotes in backticks.
fn run_in(text: &str) -> BTreeSet<String> {
    let text = text.replace("`sudo wxread ", "`wxread ");
    let quoted = text.split("`wxread ").skip(1);
    let words = quoted.filter_map(|rest| rest.split([' ', '`']).next());
    let words =
        words.filter(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()));
    words.map(str::to_string).collect()
}

#[test]
fn the_manual_is_whole() {
    if let Err(faults) = agent_help::check(&wxread::manual()) {
        panic!("the manual is not whole:\n{faults}");
    }
}

#[test]
fn the_manual_and_the_screen_name_exactly_the_commands_there_are() {
    let names = [
        "articles",
        "avatar",
        "chats",
        "contact",
        "contacts",
        "doctor",
        "favorites",
        "file",
        "group",
        "help",
        "keys",
        "messages",
        "moments",
        "resign",
        "search",
        "voice",
    ];
    let existing = existing();
    assert_eq!(existing, names.into_iter().map(String::from).collect());
    let long = asked(&["--help"]);
    let short = asked(&["-h"]);
    let headed: BTreeSet<String> = long
        .lines()
        .filter_map(|line| line.strip_prefix("#### `wxread ")?.strip_suffix('`'))
        .map(str::to_string)
        .collect();
    assert_eq!(headed, wxread::commands().into_iter().collect());
    let listed: BTreeSet<String> = short
        .lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| *line != "Options:")
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect();
    assert_eq!(listed, existing);
    for text in [&long, &short] {
        let run = run_in(text);
        assert!(run.is_subset(&existing), "{run:?} beyond {existing:?}");
    }
}

#[test]
fn the_manual_and_the_screen_are_what_the_binary_answers() {
    let manual = wxread::manual();
    let long = asked(&["--help"]);
    assert_eq!(long, manual.manual().unwrap());
    assert_eq!(asked(&["help"]), long);
    let short = asked(&["-h"]);
    assert_eq!(short, wxread::screen());
    assert!(short.lines().count() <= 24, "-h is one screen: {short}");
    for label in ["What it needs", "Run first", "What a command answers", "Rules"] {
        assert!(long.contains(&format!("\n**{label}**\n")), "the manual says {label}");
    }
    for heading in ["## Commands", "## Topics"] {
        assert!(long.contains(&format!("\n{heading}\n")), "the manual has {heading}");
    }
}

#[test]
fn no_paragraph_of_the_manual_is_wrapped() {
    // Outside a code block, a line that follows another without a blank
    // line between them starts a list item, a heading or a numbered step:
    // a paragraph is written on one line.
    let long = asked(&["--help"]);
    let mut fenced = false;
    let mut before = "";
    for line in long.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            before = line;
            continue;
        }
        if !fenced && !before.trim().is_empty() && !line.trim().is_empty() {
            let start = line.trim_start();
            let numbered = start.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok());
            assert!(
                start.starts_with("- ") || start.starts_with('#') || numbered,
                "a paragraph wrapped:\n{before}\n{line}"
            );
        }
        before = line;
    }
}

#[test]
fn each_command_answers_its_own_part_of_the_manual_whatever_else_its_line_holds() {
    let manual = asked(&["--help"]);
    for command in wxread::commands() {
        let long = help_of(&command);
        let heading = format!("# `wxread {command}`\n");
        let body = long.strip_prefix(&heading).unwrap_or_else(|| panic!("{command}: {long}"));
        let one_down = body.replace("\n## ", "\n##### ");
        assert!(manual.contains(&format!("#{heading}{one_down}")), "{command} is in the manual");
        let words: Vec<&str> = command.split(' ').collect();
        assert_eq!(asked(&[words.as_slice(), &["-h"]].concat()), long, "{command} -h");
        let noisy = [&["--keys", "/nowhere"], words.as_slice(), &["--help", "--nope"]].concat();
        assert_eq!(asked(&noisy), long, "{command} with more on its line");
        if command != "file" {
            for label in ["When to run it", "What it answers"] {
                assert!(long.contains(&format!("\n**{label}**\n")), "{command} says {label}");
            }
        }
        let run = run_in(&long);
        assert!(run.is_subset(&existing()), "{command}: {run:?}");
    }
}

#[test]
fn help_shows_a_topic_and_refuses_one_there_is_not() {
    let manual = wxread::manual();
    assert_eq!(asked(&["help", "names"]), manual.topic_help("names").unwrap());
    assert!(asked(&["--help"]).contains("](help:names)"), "the manual links it");
    let out = Command::new(env!("CARGO_BIN_EXE_wxread")).args(["help", "nope"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let error = json["error"].as_str().unwrap();
    assert!(error.contains("`nope`") && error.contains("`names`"), "{error}");
}

#[test]
fn messages_help_says_how_to_read_a_chat_forward() {
    let help = help_of("messages");
    assert!(help.contains("\n- `--oldest-first`: "), "{help}");
    let trips = part(&help, "Trips you up").join("\n");
    assert!(trips.contains("--oldest-first"), "{trips}");
}

#[test]
fn the_manual_says_how_to_find_a_chat_with_someone() {
    let long = asked(&["--help"]);
    let run_first = part(&long, "Run first").join("\n");
    assert!(run_first.contains("wxread chats --with <id>"), "{run_first}");
    assert!(run_first.contains("--query <name>"), "{run_first}");
    let chats = help_of("chats");
    assert!(chats.contains("--with <") && chats.contains("smallest first"), "{chats}");
}

#[test]
fn the_help_names_the_options_and_variables_wxread_reads() {
    for text in [asked(&["--help"]), asked(&["-h"]), help_of("doctor")] {
        for name in [DATA_DIR_FLAG, DATA_DIR_VAR, KEYS_FLAG, KEYS_VAR] {
            assert!(text.contains(name), "{name} in {text}");
        }
    }
}

#[test]
fn doctor_answers_exactly_the_keys_its_help_names() {
    let json = World::new().run(&["doctor"], &[]).ok();
    assert_eq!(answered("doctor", 0), keys_of(&json));
}

#[test]
fn chats_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.sessions = vec![Session("wxid_amy", 1_760_000_000, 0)];
    account.write();
    let json = world.run(&["chats"], &[("WXREAD_KEYS", &account.keys(&DATABASES))]).ok();
    assert_eq!(answered("chats", 0), keys_of(&json));
    assert_eq!(answered("chats", 1), keys_of(&json["chats"][0]), "chats --help says each field");
}

#[test]
fn chats_help_says_what_query_matches() {
    let help = help_of("chats");
    for words in ["--query <TEXT>", "id", "remark", "own name", "members' names", "case"] {
        assert!(help.contains(words), "chats --help says {words:?}");
    }
    assert!(asked(&["--help"]).contains("chats --query"), "the manual names chats --query");
}

#[test]
fn voice_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let mut said = Said::text("wxid_amy", 1, 1_760_000_000, "wxid_amy", "<msg/>");
    said.local_type = 34;
    account.messages = vec![said];
    account.voices =
        vec![Voice("wxid_amy", 1, include_bytes!("../../silk/tests/tone.silk").to_vec())];
    account.write();
    let out = world.root.join("v.wav").display().to_string();
    let json = world
        .run(
            &["voice", "1", "--chat", "wxid_amy", "--out", &out],
            &[("WXREAD_KEYS", &account.all_keys())],
        )
        .ok();
    assert_eq!(answered("voice", 0), keys_of(&json));
}

#[test]
fn messages_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.messages = vec![Said::text("wxid_amy", 1, 1_760_000_000, "wxid_amy", "hi")];
    account.write();
    let json = world
        .run(&["messages", "--chat", "wxid_amy"], &[("WXREAD_KEYS", &account.all_keys())])
        .ok();
    assert_eq!(answered("messages", 0), keys_of(&json));
    // Each field has its line, the kind's own (here `text`) under the kinds.
    let fields: BTreeSet<String> =
        keys_of(&json["messages"][0]).into_iter().filter(|f| f != "text").collect();
    assert!(fields.is_subset(&answered("messages", 1)), "messages --help says each field");
    // A channels feed's title is not WeChat's.
    let help = help_of("messages");
    assert!(
        help.contains("For a channels feed or live, title is its description"),
        "messages --help says what title is for a channels feed"
    );
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
fn contacts_contact_and_group_answer_exactly_the_keys_their_help_names() {
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
        assert_eq!(answered(args[0], 0), keys_of(&json), "{args:?}");
        // Each field, at whatever depth, has its line.
        let named: BTreeSet<String> = (1..4).flat_map(|depth| answered(args[0], depth)).collect();
        for field in fields(&json) {
            assert!(named.contains(&field), "{} --help says what {field} is", args[0]);
        }
    }
}

#[test]
fn file_get_and_avatar_answer_exactly_the_keys_their_help_names() {
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
        (
            "file get",
            [&["file", "get", "8", "--out", out][..], &["file", "get", "7", "--out", out]],
        ),
        (
            "avatar",
            [&["avatar", "wxid_amy", "--out", out][..], &["avatar", "wxid_bob", "--out", out]],
        ),
    ] {
        let mut keys_answered = BTreeSet::new();
        for args in args {
            let json = world.run(args, &[("WXREAD_KEYS", &keys)]).ok();
            keys_answered.extend(keys_of(&json));
        }
        assert_eq!(answered(command, 0), keys_answered, "{command}");
    }
}

#[test]
fn articles_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let article = Article("Knots", "Eight ways", "https://e.com/k", "https://e.com/k.jpg");
    account.messages = vec![Said::push("gh_crag", "Crag News", 1_760_000_000, &[article])];
    account.write();
    let json = world.run(&["articles"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(answered("articles", 0), keys_of(&json));
    assert_eq!(answered("articles", 1), keys_of(&json["articles"][0]));
    let help = help_of("articles");
    for field in keys_of(&json["articles"][0]["official_account"]) {
        assert!(help.contains("{id, display_name}") && help.contains(&field), "{field}");
    }
}

#[test]
fn moments_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let xml = r#"<SnsDataItem><TimelineObject><id>9</id><username>wxid_amy</username><createTime>1760000000</createTime><contentDesc>hi</contentDesc><location poiName="Peak" /><ContentObject><type>3</type><title>T</title><contentUrl>https://e.com</contentUrl><mediaList><media><id>1</id><type>6</type></media></mediaList></ContentObject></TimelineObject><LocalExtraInfo><like_user_list><user_comment><username>wxid_bob</username></user_comment></like_user_list><comment_user_list><user_comment><username>wxid_bob</username><content>c</content><ref_username>wxid_amy</ref_username></user_comment></comment_user_list></LocalExtraInfo></SnsDataItem>"#;
    account.moments = vec![Posted(9, "wxid_amy", xml.to_string())];
    account.write();
    let json = world.run(&["moments"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(answered("moments", 0), keys_of(&json));
    let moment = &json["moments"][0];
    assert_eq!(answered("moments", 1), keys_of(moment));
    assert_eq!(answered("moments", 2), keys_of(&moment["media"][0]), "media's fields");
    // Each object a Moment holds is listed in the help as `{its keys}`.
    let help = help_of("moments");
    let listed: Vec<BTreeSet<String>> = help
        .split('{')
        .skip(1)
        .filter_map(|after| after.split_once('}'))
        .map(|(inside, _)| {
            inside.split([',', ' ', '\n']).filter(|w| !w.is_empty()).map(String::from).collect()
        })
        .collect();
    for part in ["author", "location", "link", "likes", "comments"] {
        let object = match &moment[part] {
            Value::Array(all) => all[0].clone(),
            one => one.clone(),
        };
        assert!(listed.contains(&keys_of(&object)), "{part}: listed in the help");
    }
}

#[test]
fn favorites_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let favitem = |t: i64, body: &str| {
        format!(
            "<favitem type=\"{t}\">{body}<source><fromusr>wxid_amy</fromusr><tousr>wxid_me</tousr><createtime>1760000000</createtime><msgid>7</msgid><link>https://e.com</link></source></favitem>"
        )
    };
    let item = |d: i64, body: &str| {
        format!("<datalist><dataitem datatype=\"{d}\" dataid=\"i{d}\">{body}</dataitem></datalist>")
    };
    let saved = |id: i64, t: i64, content: String| Saved {
        id,
        fav_type: t,
        saved: 1_760_000_000 + id,
        content,
        from: "wxid_amy",
        sender: "",
    };
    account.favorites = vec![
        saved(1, 2, favitem(2, &item(2, "<fullsize>3</fullsize>"))),
        saved(2, 4, favitem(4, &item(4, "<duration>3</duration>"))),
        saved(3, 3, favitem(3, &item(3, "<duration>3</duration>"))),
        saved(4, 8, favitem(8, &item(8, "<datatitle>a.pdf</datatitle>"))),
        saved(
            5,
            14,
            favitem(14, &item(1, "<datadesc>hi</datadesc><datasrcname>Amy</datasrcname>")),
        ),
    ];
    account.write();
    let json = world.run(&["favorites"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(answered("favorites", 0), keys_of(&json));
    let favorite = &json["favorites"][0];
    let own: BTreeSet<String> =
        keys_of(favorite).into_iter().filter(|k| json!(k) != favorite["kind"]).collect();
    assert!(own.is_subset(&answered("favorites", 1)), "favorites --help says each field");
    // Every field of every kind answered is named in the help.
    let help = help_of("favorites");
    for field in fields(&json).iter().filter(|f| !f.contains(char::is_numeric)) {
        assert!(help.contains(field.as_str()), "favorites --help names {field}");
    }
}

#[test]
fn search_help_says_whitespace_does_not_decide_a_match() {
    let help = help_of("search");
    let trips = part(&help, "Trips you up").join("\n");
    assert!(trips.contains("Whitespace does not decide a match"), "{help}");
    assert!(trips.contains("\"15万行\" and \"15 万行\" both find"), "{help}");
    assert!(help.contains("with or without spaces"), "{help}");
}

#[test]
fn search_answers_exactly_the_keys_its_help_names() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.messages = vec![Said::text("wxid_amy", 1, 1_760_000_000, "wxid_amy", "hi")];
    account.write();
    let json = world.run(&["search", "hi"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(answered("search", 0), keys_of(&json));
    // Its own fields have their line; the rest are a message's, as
    // `wxread messages --help` says them.
    let help = help_of("search");
    let own = answered("search", 1);
    let message = answered("messages", 1);
    for field in keys_of(&json["messages"][0]).iter().filter(|f| *f != "text") {
        let listed = help.contains(&format!("{field},")) && message.contains(field);
        assert!(own.contains(field) || listed, "search --help says what {field} is");
    }
}

#[test]
fn messages_and_search_help_say_what_a_solitaire_is() {
    let messages = help_of("messages");
    let kinds = part(&messages, "The kinds and their fields").join("\n");
    for said in ["- `solitaire`: `{id, title,", "a 接龙", "the same in every message of one 接龙"]
    {
        assert!(kinds.contains(said), "messages --help says {said}");
    }
    let search = help_of("search");
    let searched = part(&search, "What a message is searched by").join("\n");
    let indexed = searched.split("The rest are not searched").next().unwrap();
    assert!(indexed.contains("- `solitaire`: "), "search --help says a solitaire is searched");
}

#[test]
fn resign_answers_exactly_the_keys_its_help_names() {
    // The object the command prints is the pure crate's answer;
    // tests/resign.rs runs the command itself against a signed app.
    let merge = wxread_resign::merge("W.app", "").unwrap();
    let json = wxread_resign::answer("W.app", false, &merge, false);
    assert_eq!(answered("resign", 0), keys_of(&json));
}

#[test]
fn keys_answers_in_the_shape_its_help_shows() {
    // What the command prints is the pure crate's answer; capturing a key
    // needs a running WeChat, which no test has.
    let (key, salt) = ([0xab; 32], [0xcd; 16]);
    let keys = wxread_keys::Keys::of([("message/message_0.db".into(), Key::new(key, Some(salt)))]);
    let hexes = [synthetic::hex(&key), synthetic::hex(&salt)];
    let shown = crate::common::keys_shown([("message/message_0.db", hexes)]);
    assert_eq!(keys.to_value(), Value::Object(shown));
}

#[test]
fn keys_help_points_at_resign_when_macos_refuses() {
    let help = help_of("keys");
    let section = part(&help, "Do it with a copy, never the installed app").join("\n");
    assert!(section.contains("wxread resign --app") && section.contains("--yes"), "{section}");
    let resign = help_of("resign");
    for needle in [
        "**When to run it**",
        "**What it reads and writes**",
        "**Undoing it**",
        "Full Disk Access",
        "quit WeChat",
    ] {
        assert!(resign.contains(needle), "resign --help says {needle}");
    }
}

#[test]
fn file_get_help_names_the_moment_copies_the_code_reads() {
    use wxread_media::{MOMENT_IMAGE, MOMENT_IMAGE_THUMBNAILS, MOMENT_VIDEO};
    let help = help_of("file get");
    let [first, second] = MOMENT_IMAGE_THUMBNAILS;
    let image = format!("n {MOMENT_IMAGE}; its thumbnail n {first}, else n {second},");
    assert!(help.contains(&image), "file get --help says {image}");
    let video = format!(".mp4, n {MOMENT_VIDEO};");
    assert!(help.contains(&video), "file get --help says {video}");
}

#[test]
fn keys_help_names_the_key_size_the_code_reads() {
    let bytes = format!("the raw {}-byte key", wxread_keys::KEY_BYTES);
    assert!(help_of("keys").contains(&bytes), "keys --help says {bytes}");
}
