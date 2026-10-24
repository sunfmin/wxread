//! `--version`: the release, then for a build that is not a release's tag
//! the commits since it and the commit.

use std::process::Command;

#[test]
fn version_is_the_release_stamped_with_the_commit() {
    let out = Command::new(env!("CARGO_BIN_EXE_wxread")).arg("--version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let version = text.trim().strip_prefix("wxread ").expect("the name, then the version");
    assert_eq!(version, wxread::VERSION);
    let release = env!("CARGO_PKG_VERSION");
    let Some(build) = version.strip_prefix(release).and_then(|rest| rest.strip_prefix('+')) else {
        assert_eq!(version, release);
        return;
    };
    let build = build.strip_suffix(".dirty").unwrap_or(build);
    let (commits, commit) = build.split_once(".g").expect("<commits>.g<commit>");
    assert!(commits.parse::<u64>().is_ok(), "{version}");
    assert!(commit.len() >= 7 && commit.bytes().all(|b| b.is_ascii_hexdigit()), "{version}");
}
