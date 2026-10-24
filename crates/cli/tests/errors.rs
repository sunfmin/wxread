//! Every failure is one `{"error": …}` with a non-zero exit, naming the fix.

use crate::common::World;

#[test]
fn a_command_line_wxread_cannot_parse_is_exit_2_and_points_at_the_guide() {
    let world = World::new();
    for args in [&["nope"][..], &[], &["doctor", "--nope"], &["--keys"]] {
        let answer = world.run(args, &[]);
        assert_eq!(answer.code, 2, "{args:?}");
        assert!(answer.error().ends_with(": run `wxread --help`"), "{args:?}");
    }
    assert!(world.run(&[], &[]).error().starts_with("no command was given"));
}

#[test]
fn keys_without_root_refuses_with_the_line_that_fixes_it() {
    // The gate runs as the developer, not root; attaching Frida to WeChat
    // needs root, so `wxread keys` refuses and names the command that elevates.
    let answer = World::new().run(&["keys"], &[]);
    assert_eq!(answer.code, 1);
    let error = answer.error();
    assert!(
        error.contains("only root may attach") && error.contains("sudo wxread keys"),
        "{error}"
    );
}

#[test]
fn with_no_home_doctor_says_to_set_it_or_name_the_directory() {
    let world = World::new();
    let out = world.command(&["doctor"]).env_remove("HOME").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let error = json["error"].as_str().unwrap();
    assert!(error.contains("set HOME, or pass --data-dir <dir>"), "{error}");
}
