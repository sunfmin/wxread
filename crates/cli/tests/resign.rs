//! `wxread resign`, run against a synthetic app signed the way Tencent signs
//! WeChat (hardened runtime, entitlements, no get-task-allow), with the real
//! `codesign`. No WeChat is touched.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{World, snapshot};
use wxread_resign::GET_TASK_ALLOW;

/// The entitlements the synthetic app is signed with, as WeChat's are
/// (sorted; the app lists them out of order).
const TENCENT: [&str; 4] = [
    "com.apple.security.app-sandbox",
    "com.apple.security.application-groups",
    "com.apple.security.cs.allow-jit",
    "com.apple.security.network.client",
];

/// An app at `<world>/<name>`, its program a copy of `/usr/bin/true`, signed
/// ad-hoc with the hardened runtime and [`TENCENT`]'s entitlements.
fn app(world: &World, name: &str) -> PathBuf {
    let app = world.root.join(name);
    let program = app.join("Contents/MacOS/Fake");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::copy("/usr/bin/true", &program).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>Fake</string><key>CFBundleIdentifier</key><string>com.example.fake</string></dict></plist>"#,
    )
    .unwrap();
    let entitlements = world.file(
        "tencent.plist",
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>{}</key><true/><key>{}</key><array><string>5A4RE8SF68.com.example.fake</string></array><key>{}</key><true/><key>{}</key><true/></dict></plist>"#,
            TENCENT[3], TENCENT[1], TENCENT[2], TENCENT[0]
        ),
    );
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-", "--options", "runtime", "--entitlements"])
        .arg(&entitlements)
        .arg(&app)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    app
}

/// The top-level entitlements `app` is signed with, by name, sorted.
fn signed_with(app: &Path) -> Vec<String> {
    let out = Command::new("/usr/bin/codesign")
        .args(["-d", "--entitlements", "-", "--xml"])
        .arg(app)
        .output()
        .unwrap();
    let xml = String::from_utf8(out.stdout).unwrap();
    let mut keys: Vec<String> = xml
        .split("<key>")
        .skip(1)
        .filter_map(|rest| rest.split("</key>").next())
        .map(str::to_string)
        .collect();
    keys.sort();
    keys
}

/// `wxread resign` with `args`, with codesign on the PATH.
fn resign(world: &World, args: &[&str]) -> crate::common::Answer {
    let mut all = vec!["resign"];
    all.extend_from_slice(args);
    world.run(&all, &[("PATH", "/usr/bin:/bin")])
}

#[test]
fn without_yes_it_says_what_it_would_do_and_changes_nothing() {
    let world = World::new();
    let app = app(&world, "We Chat.app");
    let before = snapshot(&app);
    let path = app.to_str().unwrap();
    let json = resign(&world, &["--app", path]).ok();
    assert_eq!(snapshot(&app), before, "a preview changes nothing");
    assert_eq!(json["app"], path);
    assert_eq!(json["applied"], false);
    assert_eq!(json["get_task_allow"], false);
    assert_eq!(json["entitlements"]["added"], serde_json::json!([GET_TASK_ALLOW]));
    let mut kept: Vec<String> =
        serde_json::from_value(json["entitlements"]["kept"].clone()).unwrap();
    kept.sort();
    assert_eq!(kept, TENCENT);
    let first = json["next"][0].as_str().unwrap();
    assert!(first.contains(&format!("`wxread resign --app '{path}' --yes`")), "{first}");
}

#[test]
fn with_yes_it_adds_get_task_allow_keeps_the_rest_and_a_second_run_changes_nothing() {
    let world = World::new();
    let app = app(&world, "WeChat.app");
    let path = app.to_str().unwrap();
    let json = resign(&world, &["--app", path, "--yes"]).ok();
    assert_eq!(json["applied"], true);
    assert_eq!(json["get_task_allow"], true);
    let mut want: Vec<String> = TENCENT.iter().map(|s| s.to_string()).collect();
    want.push(GET_TASK_ALLOW.to_string());
    want.sort();
    assert_eq!(signed_with(&app), want, "only get-task-allow is added");
    let next = json["next"].to_string();
    for needle in ["quit WeChat", "Full Disk Access", "sudo wxread keys", "reinstall"] {
        assert!(next.contains(needle), "{needle} in {next}");
    }

    // Run again: it carries get-task-allow, so nothing is re-signed.
    let before = snapshot(&app);
    let again = resign(&world, &["--app", path, "--yes"]).ok();
    assert_eq!(snapshot(&app), before, "a second run changes nothing");
    assert_eq!(again["applied"], false);
    assert_eq!(again["get_task_allow"], true);
    assert_eq!(again["entitlements"]["added"], serde_json::json!([]));
}

#[test]
fn an_app_that_is_not_there_is_named_with_the_fix() {
    let world = World::new();
    let nowhere = world.root.join("Nowhere.app");
    let error = resign(&world, &["--app", nowhere.to_str().unwrap(), "--yes"]).error();
    assert!(
        error.contains("Nowhere.app is not an app") && error.contains("--app <path>"),
        "{error}"
    );
}

#[test]
fn with_no_codesign_it_says_how_to_install_it() {
    let world = World::new();
    let app = app(&world, "WeChat.app");
    let empty = world.root.join("bin");
    std::fs::create_dir(&empty).unwrap();
    let answer = world
        .run(&["resign", "--app", app.to_str().unwrap()], &[("PATH", empty.to_str().unwrap())]);
    let error = answer.error();
    assert!(
        error.contains("codesign was not found") && error.contains("xcode-select --install"),
        "{error}"
    );
}

#[test]
fn an_app_it_may_not_change_is_refused_naming_sudo() {
    use std::os::unix::fs::PermissionsExt;

    let world = World::new();
    let app = app(&world, "WeChat.app");
    // The whole bundle read-only, as a WeChat installed by root is to the
    // user: every directory and file in it, the app itself last to be locked.
    let set = |mode: u32| {
        let mut paths: Vec<PathBuf> = snapshot(&app).into_keys().collect();
        paths.push(app.clone());
        if mode & 0o200 == 0 {
            paths.reverse();
        }
        for path in paths {
            let mode = if path.is_dir() { mode } else { mode & 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    set(0o555);
    let before = snapshot(&app);
    let answer = resign(&world, &["--app", app.to_str().unwrap(), "--yes"]);
    let after = snapshot(&app);
    set(0o755);
    let error = answer.error();
    assert!(
        error.contains("sudo wxread resign --app") && error.contains("App Management"),
        "{error}"
    );
    assert_eq!(after, before, "a refused re-sign changes nothing");
}
