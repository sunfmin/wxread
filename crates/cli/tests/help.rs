//! The help: `--help` is the guide an agent learns the tool from, `-h` one
//! screen, and the commands either names are exactly the commands there are.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::{Value, json};

use crate::common::World;
use crate::synthetic::{
    Account, Article, Contact, DATABASES, Group, Posted, Said, Saved, Session, Voice,
};
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

/// The subcommands the guide sorts into groups: its lines of bare names,
/// a command's own subcommand (`group leave`) counted as the command.
fn grouped(long: &str) -> BTreeSet<String> {
    let lines = between(long, "THE COMMANDS", "RULES").into_iter();
    let lines = lines.filter(|line| line.starts_with("      "));
    let mut names = BTreeSet::new();
    for line in lines {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [command, sub] if wxread::subcommands(command).iter().any(|s| s == sub) => {
                names.insert(command.to_string());
            }
            _ => names.extend(words.iter().map(|w| w.to_string())),
        }
    }
    names
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
        "keys",
        "messages",
        "moments",
        "resign",
        "search",
        "voice",
    ];
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
fn messages_help_says_how_to_read_a_chat_forward() {
    let help = asked(&["messages", "--help"]);
    let options = between(&help, "OPTIONS", "WHAT IT ANSWERS").join("\n");
    assert!(options.contains("--oldest-first"), "{options}");
    let trips = between(&help, "TRIPS YOU UP", "\u{0}").join("\n");
    assert!(trips.contains("--oldest-first"), "{trips}");
}

#[test]
fn the_guide_says_how_to_find_a_chat_with_someone() {
    let run_first = between(wxread::GUIDE, "RUN FIRST", "WHAT A COMMAND ANSWERS").join("\n");
    assert!(run_first.contains("wxread chats --with <id>"), "{run_first}");
    assert!(run_first.contains("--query <name>"), "{run_first}");
    let chats = asked(&["chats", "--help"]);
    assert!(chats.contains("--with <id>") && chats.contains("smallest first"), "{chats}");
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

/// The keys the guide's row for `command` names, the lines that continue
/// it (indented fourteen) too.
fn guide_row(command: &str) -> BTreeSet<String> {
    let mut lines = between(wxread::GUIDE, "WHAT A COMMAND ANSWERS", "THE COMMANDS")
        .into_iter()
        .skip_while(|line| !line.starts_with(&format!("    {command} ")));
    let row = lines.next().expect("the guide has a row for the command");
    let more = lines.take_while(|line| line.starts_with(&" ".repeat(14)));
    let words = row.split_whitespace().skip(1).chain(more.flat_map(str::split_whitespace));
    words.map(str::to_string).collect()
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
fn chats_help_says_what_query_matches() {
    let help = asked(&["chats", "--help"]);
    for words in ["--query <text>", "id", "remark", "own name", "members' names", "case"] {
        assert!(help.contains(words), "chats --help says {words:?}");
    }
    assert!(asked(&["--help"]).contains("chats --query"), "the guide names chats --query");
}

#[test]
fn voice_answers_exactly_the_keys_its_help_and_the_guide_name() {
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
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("voice"), keys);
    assert_eq!(guide_row("voice"), keys);
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
    // A channels feed's title is not WeChat's.
    let words = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        words.contains("For a channels feed or live, title is its description"),
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

#[test]
fn articles_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let article = Article("Knots", "Eight ways", "https://e.com/k", "https://e.com/k.jpg");
    account.messages = vec![Said::push("gh_crag", "Crag News", 1_760_000_000, &[article])];
    account.write();
    let json = world.run(&["articles"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("articles"), keys);
    assert_eq!(guide_row("articles"), keys);
    let help = asked(&["articles", "--help"]);
    let article = json["articles"][0].as_object().unwrap();
    for field in article.keys() {
        assert!(
            help.contains(&format!("              {field} ")),
            "articles --help says what {field} is"
        );
    }
    for field in json["articles"][0]["official_account"].as_object().unwrap().keys() {
        assert!(help.contains("{id, display_name}") && help.contains(field), "{field}");
    }
}

#[test]
fn moments_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    let xml = r#"<SnsDataItem><TimelineObject><id>9</id><username>wxid_amy</username><createTime>1760000000</createTime><contentDesc>hi</contentDesc><location poiName="Peak" /><ContentObject><type>3</type><title>T</title><contentUrl>https://e.com</contentUrl><mediaList><media><id>1</id><type>6</type></media></mediaList></ContentObject></TimelineObject><LocalExtraInfo><like_user_list><user_comment><username>wxid_bob</username></user_comment></like_user_list><comment_user_list><user_comment><username>wxid_bob</username><content>c</content><ref_username>wxid_amy</ref_username></user_comment></comment_user_list></LocalExtraInfo></SnsDataItem>"#;
    account.moments = vec![Posted(9, "wxid_amy", xml.to_string())];
    account.write();
    let json = world.run(&["moments"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("moments"), keys);
    assert_eq!(guide_row("moments"), keys);
    let help = asked(&["moments", "--help"]);
    let moment = &json["moments"][0];
    for field in moment.as_object().unwrap().keys() {
        assert!(
            help.contains(&format!("              {field} ")),
            "moments --help says what {field} is"
        );
    }
    for field in moment["media"][0].as_object().unwrap().keys() {
        assert!(help.contains(&format!("                {field} ")), "media's {field}");
    }
    // Each object a Moment holds is listed in the help as `{its keys}`.
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
        let keys: BTreeSet<String> = object.as_object().unwrap().keys().cloned().collect();
        assert!(listed.contains(&keys), "{part}: {keys:?} listed in the help");
    }
}

#[test]
fn favorites_answers_exactly_the_keys_its_help_and_the_guide_name() {
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
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("favorites"), keys);
    assert_eq!(guide_row("favorites"), keys);
    let help = asked(&["favorites", "--help"]);
    let favorite = json["favorites"][0].as_object().unwrap();
    for field in favorite.keys().filter(|k| json!(k) != favorite["kind"]) {
        assert!(
            help.contains(&format!("               {field} ")),
            "favorites --help says what {field} is"
        );
    }
    // Every field of every kind answered is named in the help.
    for field in fields(&json).iter().filter(|f| !f.contains(char::is_numeric)) {
        assert!(help.contains(field.as_str()), "favorites --help names {field}");
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
        wxread_favorites::Kind::ALL.iter().map(|k| k.name().to_string()).collect();
    assert_eq!(kinds, all);
}

#[test]
fn search_help_says_whitespace_does_not_decide_a_match() {
    let help = asked(&["search", "--help"]);
    let trips = between(&help, "TRIPS YOU UP", "\u{0}").join("\n");
    assert!(trips.contains("Whitespace does not decide a match"), "{help}");
    assert!(trips.contains("\"15万行\" and \"15 万行\" both find"), "{help}");
    assert!(help.contains("with or without spaces"), "{help}");
}

#[test]
fn search_answers_exactly_the_keys_its_help_and_the_guide_name() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.messages = vec![Said::text("wxid_amy", 1, 1_760_000_000, "wxid_amy", "hi")];
    account.write();
    let json = world.run(&["search", "hi"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("search"), keys);
    assert_eq!(guide_row("search"), keys);
    let help = asked(&["search", "--help"]);
    let messages = asked(&["messages", "--help"]);
    let fields: BTreeSet<String> =
        json["messages"][0].as_object().unwrap().keys().cloned().collect();
    // Its own fields have their line; the rest are a message's, as
    // `wxread messages --help` says them.
    for field in fields.iter().filter(|f| *f != "text") {
        let own = help.contains(&format!("              {field} "));
        let listed = help.contains(&format!("{field},"))
            && messages.contains(&format!("              {field} "));
        assert!(own || listed, "search --help says what {field} is");
    }
    // Each kind it names, it names by its name.
    let kinds: BTreeSet<String> =
        wxread_messages::Kind::ALL.iter().map(|k| k.name().to_string()).collect();
    let searched = between(&help, "WHAT A MESSAGE IS SEARCHED BY", "TRIPS YOU UP");
    let a_kinds_line = |l: &&&str| {
        l.len() - l.trim_start().len() == 2
            && l.trim_start().starts_with(|c: char| c.is_ascii_lowercase())
    };
    for line in searched.iter().filter(a_kinds_line) {
        let named = line.split("  ").find(|s| !s.is_empty()).unwrap();
        for kind in named.split(", ").map(str::trim).filter(|k| !k.is_empty()) {
            if kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                assert!(kinds.contains(kind), "{kind} is a kind");
            }
        }
    }
}

#[test]
fn messages_and_search_help_say_what_a_solitaire_is() {
    let messages = asked(&["messages", "--help"]);
    let kinds = between(&messages, "THE KINDS AND THEIR FIELDS", "TRIPS YOU UP").join(" ");
    let words = kinds.split_whitespace().collect::<Vec<_>>().join(" ");
    for said in ["solitaire {id, title,", "a 接龙", "the same in every message of one 接龙"] {
        assert!(words.contains(said), "messages --help says {said}");
    }
    let search = asked(&["search", "--help"]);
    let searched = between(&search, "WHAT A MESSAGE IS SEARCHED BY", "  The rest are not searched");
    assert!(
        searched.iter().any(|l| l.trim_start().starts_with("solitaire ")),
        "search --help says a solitaire is searched"
    );
}

#[test]
fn resign_answers_exactly_the_keys_its_help_and_the_guide_name() {
    // The object the command prints is the pure crate's answer;
    // tests/resign.rs runs the command itself against a signed app.
    let merge = wxread_resign::merge("W.app", "").unwrap();
    let json = wxread_resign::answer("W.app", false, &merge, false);
    let keys: BTreeSet<String> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(answered("resign"), keys);
    assert_eq!(guide_row("resign"), keys);
}

#[test]
fn keys_help_points_at_resign_when_macos_refuses() {
    let help = asked(&["keys", "--help"]);
    let section = between(&help, "IF MACOS REFUSES TO READ WECHAT", "TRIPS YOU UP").join("\n");
    assert!(section.contains("`wxread resign`") && section.contains("`wxread resign --yes`"));
    let resign = asked(&["resign", "--help"]);
    for needle in [
        "WHEN TO RUN IT",
        "WHAT IT READS AND WRITES",
        "UNDOING IT",
        "Full Disk Access",
        "quit WeChat",
    ] {
        assert!(resign.contains(needle), "resign --help says {needle}");
    }
}
