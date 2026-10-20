//! Re-signing WeChat so its memory can be read
//! (docs/adr/0005-wxread-resign-re-signs-wechat-when-asked.md).
//!
//! A WeChat installed from Tencent carries the hardened runtime without the
//! `com.apple.security.get-task-allow` entitlement, so macOS refuses
//! `task_for_pid` against it even for root. `wxread resign` re-signs it
//! ad-hoc with the entitlements it has plus that one. The effect crate runs
//! `codesign`; everything decided is here, a pure function of what codesign
//! reported:
//!
//! - [`merge`] reads the entitlements `codesign -d --entitlements - --xml`
//!   printed and answers either that the app carries `get-task-allow`
//!   already, or the plist to sign it with: the same entitlements, with only
//!   `get-task-allow` added (or turned true).
//! - [`answer`] is the one JSON object the command prints.
//! - The failures are each the one line that says what went wrong and fixes
//!   it.

use serde_json::{Value, json};

/// The entitlement that lets another process (root's) read the app's memory.
pub const GET_TASK_ALLOW: &str = "com.apple.security.get-task-allow";

/// Where WeChat for Mac is installed, in the order they are looked at: the
/// International build, then the Chinese one.
pub const APPS: [&str; 2] = ["/Applications/WeChat.app", "/Applications/Weixin.app"];

/// The one line a failure prints, carrying its own fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure(pub String);

/// What signing the app with `get-task-allow` takes, decided from the
/// entitlements it is signed with now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    /// It carries `get-task-allow` already: nothing is to be done.
    Already { kept: Vec<String> },
    /// Signing it with `plist` adds `get-task-allow` and keeps the rest.
    Add { kept: Vec<String>, plist: String },
}

impl Merge {
    /// The entitlements other than `get-task-allow`, by name, as the app
    /// carries them: what re-signing keeps.
    pub fn kept(&self) -> &[String] {
        match self {
            Merge::Already { kept } | Merge::Add { kept, .. } => kept,
        }
    }
}

/// The plist of an app signed with no entitlements: `get-task-allow` alone.
const FRESH: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">"#,
    r#"<plist version="1.0"><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>"#,
);
/// The entry added to the top dict.
const ENTRY: &str = "<key>com.apple.security.get-task-allow</key><true/>";

/// What signing `app` with `get-task-allow` takes, from `xml`, the
/// entitlements codesign printed of it (empty when it has none): either it
/// carries `get-task-allow` already, or the plist that keeps every
/// entitlement it has and adds that one.
pub fn merge(app: &str, xml: &str) -> Result<Merge, Failure> {
    if xml.trim().is_empty() {
        return Ok(Merge::Add { kept: Vec::new(), plist: FRESH.to_string() });
    }
    let Some(read) = read(xml) else {
        return Err(unreadable(app, "codesign printed entitlements that are not a plist dict"));
    };
    let kept: Vec<String> = read.keys.into_iter().filter(|k| k != GET_TASK_ALLOW).collect();
    let spliced = match read.get_task_allow {
        Some(value) if value.text == "true/" => return Ok(Merge::Already { kept }),
        // Present, and false (or anything else): its value becomes true.
        Some(value) => splice(xml, value.at, value.end, "<true/>"),
        None if read.top.text == "dict/" => {
            splice(xml, read.top.at, read.top.end, &format!("<dict>{ENTRY}</dict>"))
        }
        None => splice(xml, read.top.end, read.top.end, ENTRY),
    };
    match spliced {
        Some(plist) => Ok(Merge::Add { kept, plist }),
        None => Err(unreadable(app, "codesign printed entitlements that are not a plist dict")),
    }
}

/// `xml` with the bytes from `at` to `end` replaced by `with`, or `None` if
/// those are not character boundaries of it.
fn splice(xml: &str, at: usize, end: usize, with: &str) -> Option<String> {
    let head = xml.get(..at)?;
    let tail = xml.get(end..)?;
    Some(format!("{head}{with}{tail}"))
}

/// One tag of a plist: the byte it starts at (its `<`), the byte past its
/// `>`, and what is between them.
#[derive(Debug, Clone, Copy)]
struct Tag<'a> {
    at: usize,
    end: usize,
    text: &'a str,
}

/// Every tag of `xml`, in order; an unclosed `<` ends them.
fn tags(xml: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rest) = xml.get(from..) {
        let Some(open) = rest.find('<') else { break };
        let Some(at) = from.checked_add(open) else { break };
        let Some(inner) = xml.get(at..) else { break };
        let Some(close) = inner.find('>') else { break };
        let Some(gt) = at.checked_add(close) else { break };
        let Some(end) = gt.checked_add(1) else { break };
        let Some(start) = at.checked_add(1) else { break };
        let Some(text) = xml.get(start..gt) else { break };
        out.push(Tag { at, end, text });
        from = end;
    }
    out
}

/// What the top dict of an entitlements plist holds: the dict's own opening
/// tag, its keys in order, and the value tag of `get-task-allow` if it is
/// one of them.
struct Read<'a> {
    top: Tag<'a>,
    keys: Vec<String>,
    get_task_allow: Option<Tag<'a>>,
}

/// The top dict of `xml`, or `None` when it has none or it is not closed.
/// Only its own keys are read: a dict inside it is passed over.
fn read(xml: &str) -> Option<Read<'_>> {
    let tags = tags(xml);
    let mut top: Option<Tag> = None;
    let mut depth = 0usize;
    let mut keys = Vec::new();
    let mut get_task_allow = None;
    let mut i = 0usize;
    while let Some(tag) = tags.get(i) {
        let next = i.checked_add(1)?;
        match (top, tag.text) {
            (None, "dict/") => return Some(Read { top: *tag, keys, get_task_allow }),
            (None, "dict") => {
                top = Some(*tag);
                depth = 1;
            }
            (None, _) => {}
            (Some(_), "dict") => depth = depth.checked_add(1)?,
            (Some(open), "/dict") => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(Read { top: open, keys, get_task_allow });
                }
            }
            (Some(_), "key") if depth == 1 => {
                let close = tags.get(next)?;
                if close.text != "/key" {
                    return None;
                }
                let name = xml.get(tag.end..close.at)?.trim().to_string();
                if name == GET_TASK_ALLOW {
                    get_task_allow = Some(*tags.get(next.checked_add(1)?)?);
                }
                keys.push(name);
            }
            (Some(_), _) => {}
        }
        i = next;
    }
    None
}

/// `word` as a shell reads it back: as it is when it holds nothing a shell
/// would read otherwise, else in single quotes.
fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-+@:,".contains(&b));
    if plain { word.to_string() } else { format!("'{}'", word.replace('\'', r"'\''")) }
}

/// The command line that runs `wxread resign` again, with `--app` when the
/// user named the app, and `--yes` when it is to re-sign.
pub fn line(app: Option<&str>, yes: bool) -> String {
    let mut line = String::from("wxread resign");
    if let Some(app) = app {
        line.push_str(" --app ");
        line.push_str(&quote(app));
    }
    if yes {
        line.push_str(" --yes");
    }
    line
}

/// What to do once WeChat carries `get-task-allow`, in order.
pub const AFTER: [&str; 4] = [
    "quit WeChat (⌘Q) and open it again: a running WeChat keeps the signature it was started with",
    "grant the app running wxread (a terminal, an agent's host) Full Disk Access, in System Settings › Privacy & Security › Full Disk Access",
    "run `sudo wxread keys`",
    "re-signing can stop WeChat updating itself: reinstall WeChat from Tencent to undo it",
];

/// The one JSON object `wxread resign` answers about `app`, which the user
/// named with `--app` when `named`: what `merge` decided, and whether this
/// run re-signed it (`signed`) or only said what it would do.
pub fn answer(app: &str, named: bool, merge: &Merge, signed: bool) -> Value {
    let (applied, carries, added): (bool, bool, Vec<&str>) = match merge {
        Merge::Already { .. } => (false, true, Vec::new()),
        Merge::Add { .. } => (signed, signed, vec![GET_TASK_ALLOW]),
    };
    let mut next: Vec<String> = Vec::new();
    if !carries {
        let again = line(named.then_some(app), true);
        next.push(format!("nothing was changed: run `{again}` to re-sign it"));
    }
    next.extend(AFTER.iter().map(|s| s.to_string()));
    json!({
        "app": app,
        "applied": applied,
        "get_task_allow": carries,
        "entitlements": {"kept": merge.kept(), "added": added},
        "next": next,
    })
}

/// WeChat is not where WeChat for Mac installs itself.
pub fn not_found() -> Failure {
    Failure(format!(
        "WeChat for Mac was not found at {}: install WeChat for Mac 4.x, or pass --app <path> naming the WeChat.app to re-sign",
        APPS.join(" or ")
    ))
}

/// `app`, which the user named, is not an app, for `why`.
pub fn not_an_app(app: &str, why: &str) -> Failure {
    Failure(format!(
        "{app} is not an app ({why}): pass --app <path> naming WeChat's .app, as {}",
        APPS[0]
    ))
}

/// There is no `codesign` to run; `retry` is the line to run once there is.
pub fn codesign_missing(retry: &str) -> Failure {
    Failure(format!(
        "codesign was not found, and wxread re-signs WeChat with it: install Apple's command line tools with `xcode-select --install`, then run `{retry}` again"
    ))
}

/// codesign could not say what `app` is signed with, for `why`.
pub fn unreadable(app: &str, why: &str) -> Failure {
    Failure(format!(
        "codesign could not read how {app} is signed ({why}): pass --app <path> naming WeChat's .app, or reinstall WeChat from Tencent and run `wxread resign` again"
    ))
}

/// What says macOS refused codesign changing an app, in codesign's words.
const NOT_PERMITTED: [&str; 3] =
    ["Permission denied", "Operation not permitted", "Write permissions error"];

/// codesign refused to re-sign `app`, saying `why` (its stderr, where its
/// note that it is replacing the signature is passed over); `retry` is the
/// line the user ran.
pub fn sign_failed(app: &str, why: &str, retry: &str) -> Failure {
    let lines: Vec<&str> = why
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.ends_with("replacing existing signature"))
        .collect();
    let why = lines.join("; ");
    if NOT_PERMITTED.iter().any(|n| why.contains(n)) {
        Failure(format!(
            "macOS did not let wxread re-sign {app} ({why}): run `sudo {retry}`, and if it is still refused, allow the app running wxread in System Settings › Privacy & Security › App Management, then run it again"
        ))
    } else {
        Failure(format!(
            "codesign could not re-sign {app} ({why}): reinstall WeChat from Tencent, then run `{retry}` again"
        ))
    }
}

/// codesign answered success, but `app` does not carry `get-task-allow`.
pub fn not_taken(app: &str, retry: &str) -> Failure {
    Failure(format!(
        "codesign re-signed {app}, and it still does not carry {GET_TASK_ALLOW}: reinstall WeChat from Tencent, then run `{retry}` again"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// WeChat's own entitlements, as codesign prints them (shortened).
    const WECHAT: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict>"#,
        r#"<key>com.apple.application-identifier</key><string>5A4RE8SF68.com.tencent.xinWeChat</string>"#,
        r#"<key>com.apple.security.app-sandbox</key><true/>"#,
        r#"<key>com.apple.security.application-groups</key><array><string>5A4RE8SF68.com.tencent.xinWeChat</string></array>"#,
        r#"<key>com.example.nested</key><dict><key>inner</key><false/></dict>"#,
        r#"<key>com.apple.security.temporary-exception.sbpl</key><array><string>(allow network-outbound (literal &quot;/private/var/run/usbmuxd&quot;))</string></array>"#,
        r#"</dict></plist>"#,
    );

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn merge_keeps_every_entitlement_and_adds_only_get_task_allow() {
        let Merge::Add { kept, plist } = merge("W.app", WECHAT).unwrap() else { panic!() };
        let all = [
            "com.apple.application-identifier",
            "com.apple.security.app-sandbox",
            "com.apple.security.application-groups",
            "com.example.nested",
            "com.apple.security.temporary-exception.sbpl",
        ];
        assert_eq!(kept, names(&all));
        // The plist signed with is the one read, with the one entry more.
        assert_eq!(plist.replacen(ENTRY, "", 1), WECHAT);
        let mut with = names(&all);
        with.insert(0, GET_TASK_ALLOW.to_string());
        assert_eq!(read(&plist).unwrap().keys, with);
        // And signed with it, there is nothing more to do.
        assert_eq!(merge("W.app", &plist).unwrap(), Merge::Already { kept: names(&all) });
    }

    #[test]
    fn merge_turns_a_false_get_task_allow_true() {
        let xml = WECHAT.replace(
            "<dict><key>com.apple.application",
            &format!("<dict><key>{GET_TASK_ALLOW}</key><false/><key>com.apple.application"),
        );
        let Merge::Add { kept, plist } = merge("W.app", &xml).unwrap() else { panic!() };
        assert_eq!(kept.len(), 5);
        assert_eq!(plist, xml.replacen("<false/>", "<true/>", 1));
    }

    #[test]
    fn merge_of_an_app_with_no_entitlements_signs_with_get_task_allow_alone() {
        for xml in ["", "  \n"] {
            let Merge::Add { kept, plist } = merge("W.app", xml).unwrap() else { panic!() };
            assert!(kept.is_empty());
            assert_eq!(read(&plist).unwrap().keys, names(&[GET_TASK_ALLOW]));
        }
        let empty = r#"<?xml version="1.0"?><plist version="1.0"><dict/></plist>"#;
        let Merge::Add { kept, plist } = merge("W.app", empty).unwrap() else { panic!() };
        assert!(kept.is_empty());
        assert_eq!(read(&plist).unwrap().keys, names(&[GET_TASK_ALLOW]));
    }

    #[test]
    fn merge_refuses_what_is_not_a_plist_dict_naming_the_app() {
        for xml in ["garbage", "<plist><dict><key>a</key><true/>", "<plist><array/></plist>"] {
            let Failure(line) = merge("/A/W.app", xml).unwrap_err();
            assert!(line.contains("/A/W.app") && line.contains("--app"), "{line}");
        }
    }

    #[test]
    fn the_answer_says_what_it_would_do_did_or_need_not_do() {
        let add = merge("W.app", WECHAT).unwrap();
        let preview = answer("/Applications/WeChat.app", false, &add, false);
        assert_eq!(preview["applied"], false);
        assert_eq!(preview["get_task_allow"], false);
        assert_eq!(preview["entitlements"]["added"], json!([GET_TASK_ALLOW]));
        assert_eq!(preview["entitlements"]["kept"].as_array().unwrap().len(), 5);
        let next = preview["next"][0].as_str().unwrap();
        assert!(next.contains("nothing was changed") && next.contains("`wxread resign --yes`"));

        let named = answer("/My Apps/WeChat.app", true, &add, false);
        let next = named["next"][0].as_str().unwrap();
        assert!(next.contains("`wxread resign --app '/My Apps/WeChat.app' --yes`"), "{next}");

        let done = answer("W.app", false, &add, true);
        assert_eq!(done["applied"], true);
        assert_eq!(done["get_task_allow"], true);
        assert_eq!(done["next"], json!(AFTER));

        let already = answer("W.app", false, &Merge::Already { kept: vec![] }, false);
        assert_eq!(already["applied"], false);
        assert_eq!(already["get_task_allow"], true);
        assert_eq!(already["entitlements"]["added"], json!([]));
        assert_eq!(already["next"], json!(AFTER));
        let keys: Vec<&String> = already.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["app", "applied", "entitlements", "get_task_allow", "next"]);
    }

    #[test]
    fn after_names_quitting_full_disk_access_keys_and_the_undo() {
        let all = AFTER.join("\n");
        for needle in ["quit WeChat", "Full Disk Access", "sudo wxread keys", "reinstall"] {
            assert!(all.contains(needle), "{needle}");
        }
    }

    #[test]
    fn each_failure_names_its_fix() {
        let retry = line(None, true);
        let cases = [
            (not_found(), "--app <path>"),
            (not_an_app("/x", "No such file or directory"), "--app <path>"),
            (codesign_missing(&retry), "xcode-select --install"),
            (unreadable("/x", "bundle format unrecognized"), "reinstall"),
            (sign_failed("/x", "/x: Permission denied\n", &retry), "sudo wxread resign --yes"),
            (sign_failed("/x", "/x: Operation not permitted", &retry), "App Management"),
            (sign_failed("/x", "/x: something else", &retry), "reinstall"),
            (
                sign_failed(
                    "/x",
                    "/x: replacing existing signature\n/x: Write permissions error.\n",
                    &retry,
                ),
                "App Management",
            ),
            (not_taken("/x", &retry), "reinstall"),
        ];
        let Failure(text) = sign_failed(
            "/x",
            "/x: replacing existing signature\n/x: Write permissions error.\n",
            &retry,
        );
        assert!(!text.contains("replacing"), "{text}");
        for (Failure(text), needle) in cases {
            assert!(text.contains(needle), "{needle} in {text}");
            assert!(text.contains(": "), "{text}");
        }
    }

    #[test]
    fn line_quotes_only_what_a_shell_would_split() {
        assert_eq!(line(None, false), "wxread resign");
        assert_eq!(
            line(Some("/Applications/WeChat.app"), true),
            "wxread resign --app /Applications/WeChat.app --yes"
        );
        assert_eq!(line(Some("/a b/it's.app"), false), r"wxread resign --app '/a b/it'\''s.app'");
    }

    proptest! {
        // Whatever top-level keys an app carries, merging keeps each of them,
        // in order, and adds get-task-allow, once.
        #[test]
        fn merge_keeps_any_keys_and_adds_one(
            keys in proptest::collection::vec("[a-z][a-z.-]{0,20}", 0..12),
        ) {
            let body: String = keys.iter().map(|k| format!("<key>{k}</key><string>v</string>")).collect();
            let xml = format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>{body}</dict></plist>");
            let Merge::Add { kept, plist } = merge("W.app", &xml).unwrap() else { panic!() };
            prop_assert_eq!(&kept, &keys);
            let mut with = vec![GET_TASK_ALLOW.to_string()];
            with.extend(keys.iter().cloned());
            prop_assert_eq!(read(&plist).unwrap().keys, with);
        }
    }
}
