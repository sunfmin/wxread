//! What a release is made of: `packaging/release.sh`, which the release
//! workflow runs step by step and the README gives for a release cut by
//! hand, the formula it generates, and the SKILL.md it ships.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/release.sh");

const WORKFLOW: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.github/workflows/release.yml"));
const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"));
const SKILL: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../skills/wxread/SKILL.md"));

/// The rustup line `build` gives when the toolchain or a target is missing,
/// and the one the workflow installs its toolchain with.
const INSTALL_STABLE: &str = "rustup toolchain install stable --profile minimal --target aarch64-apple-darwin,x86_64-apple-darwin";

fn release(dist: &Path, args: &[&str]) -> Output {
    Command::new(SCRIPT).args(args).env("DIST", dist).output().unwrap()
}

fn stdout(out: &Output) -> String {
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    assert!(!out.status.success(), "expected a refusal: {}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn sha256(file: &Path) -> String {
    let out = Command::new("shasum").args(["-a", "256"]).arg(file).output().unwrap();
    stdout(&out).split_whitespace().next().unwrap().to_string()
}

fn archive(dist: &Path, arch: &str) -> PathBuf {
    dist.join(format!("wxread-{VERSION}-macos-{arch}.tar.gz"))
}

#[test]
fn a_release_is_tagged_with_the_version_cargo_builds() {
    let dist = tempfile::tempdir().unwrap();
    assert_eq!(stdout(&release(dist.path(), &["version"])).trim(), VERSION);
    assert!(release(dist.path(), &["check", &format!("v{VERSION}")]).status.success());
    for wrong in [VERSION, "v0.0.0-not-this", "release"] {
        let said = stderr(&release(dist.path(), &["check", wrong]));
        assert!(said.contains(&format!("{wrong} is not v{VERSION}")), "{said}");
    }
}

#[test]
fn the_formula_installs_what_was_packaged_from_the_release() {
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(archive(dist.path(), "arm64"), "the arm64 build").unwrap();

    let said = stderr(&release(dist.path(), &["formula"]));
    assert!(said.contains("macos-x86_64.tar.gz is missing: package it first"), "{said}");

    std::fs::write(archive(dist.path(), "x86_64"), "the x86_64 build").unwrap();
    let formula = stdout(&release(dist.path(), &["formula"]));
    let base = format!("https://github.com/sunfmin/wxread/releases/download/v{VERSION}");
    for arch in ["arm64", "x86_64"] {
        let block = format!(
            "    url \"{base}/wxread-{VERSION}-macos-{arch}.tar.gz\"\n    sha256 \"{}\"\n",
            sha256(&archive(dist.path(), arch))
        );
        assert!(formula.contains(&block), "{arch}: {formula}");
    }
    assert!(formula.contains(&format!("  version \"{VERSION}\"\n")), "{formula}");
    assert!(formula.contains("  depends_on :macos\n"), "{formula}");
    assert!(formula.contains("bin.install \"wxread\""), "{formula}");
    // what the installer cannot do: the keys, read once with sudo
    assert!(formula.contains("sudo wxread keys"), "{formula}");
    assert!(formula.contains("wxread keys --help"), "{formula}");
    assert!(formula.contains("assert_equal \"wxread #{version}\""), "{formula}");

    // it is Ruby, where there is a ruby to ask
    let file = dist.path().join("wxread.rb");
    std::fs::write(&file, &formula).unwrap();
    if let Ok(checked) = Command::new("ruby").arg("-c").arg(&file).output() {
        assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
    }
}

#[test]
fn package_refuses_what_was_not_built() {
    let dist = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let out = Command::new(SCRIPT)
        .arg("package")
        .env("DIST", dist.path())
        .env("CARGO_TARGET_DIR", target.path())
        .output()
        .unwrap();
    let said = stderr(&out);
    assert!(
        said.contains("aarch64-apple-darwin/release/wxread is missing: build it first"),
        "{said}"
    );
    assert_eq!(std::fs::read_dir(dist.path()).unwrap().count(), 0, "nothing is packaged");
}

#[test]
fn a_build_with_no_stable_toolchain_says_how_to_get_it() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(SCRIPT)
        .arg("build")
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let said = stderr(&out);
    assert!(said.contains("no rustup is installed (https://rustup.rs)"), "{said}");
    assert!(said.contains(INSTALL_STABLE), "{said}");
    assert!(WORKFLOW.contains(INSTALL_STABLE), "the workflow installs the same: {WORKFLOW}");
}

/// rustup's `--target` takes one value, or a comma-separated list: a second
/// target after a space is read as a toolchain name, and the install fails.
#[test]
fn the_install_line_names_one_toolchain_and_both_targets() {
    let args =
        INSTALL_STABLE.strip_prefix("rustup toolchain install ").expect("it installs a toolchain");
    let (mut toolchains, mut targets) = (Vec::new(), Vec::new());
    let mut words = args.split_whitespace();
    while let Some(word) = words.next() {
        match word {
            "--profile" => {
                words.next().expect("--profile takes a value");
            }
            "--target" | "-t" => {
                targets.extend(words.next().expect("--target takes a value").split(','))
            }
            flag if flag.starts_with('-') => panic!("{flag}: not a flag this line is read with"),
            toolchain => toolchains.push(toolchain),
        }
    }
    assert_eq!(toolchains, ["stable"], "{INSTALL_STABLE}");
    assert_eq!(targets, ["aarch64-apple-darwin", "x86_64-apple-darwin"], "{INSTALL_STABLE}");
}

#[test]
fn the_tap_is_not_pushed_without_a_formula() {
    let dist = tempfile::tempdir().unwrap();
    let said = stderr(&release(dist.path(), &["tap"]));
    assert!(said.contains("is missing: package it first"), "{said}");
}

#[test]
fn the_workflow_runs_the_steps_the_readme_gives_in_order() {
    let mut last = 0;
    for step in ["check", "build", "package", "release", "tap"] {
        let line = format!("packaging/release.sh {step}");
        let at = WORKFLOW[last..]
            .find(&line)
            .unwrap_or_else(|| panic!("the workflow runs no {step} after the step before it"));
        last += at + line.len();
        assert!(README.contains(&format!("\n{line} ")), "{step}: the README");
    }
    // `publish` is `release`, then `tap`: the README's one line for both
    assert!(README.contains("\npackaging/release.sh publish "), "{README}");
    // the release is the repository's own token's to make; the tap needs a
    // deploy key that may write sunfmin/homebrew-tap, and says the fix when it
    // is not set
    let tap = WORKFLOW.find("packaging/release.sh tap").unwrap();
    assert!(WORKFLOW[..tap].contains("HOMEBREW_TAP_DEPLOY_KEY"), "{WORKFLOW}");
    assert!(WORKFLOW.contains("No HOMEBREW_TAP_DEPLOY_KEY"), "{WORKFLOW}");
}

#[test]
fn the_shipped_skill_sends_an_agent_to_the_help() {
    let (front, body) = SKILL
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("SKILL.md opens with its front matter");
    assert!(front.lines().any(|line| line == "name: wxread"), "{front}");
    assert!(front.lines().any(|line| line.starts_with("description: ")), "{front}");
    assert!(body.contains("`wxread --help`"), "{body}");
    assert!(
        body.lines().filter(|line| !line.trim().is_empty()).count() <= 6,
        "a few lines: {body}"
    );
    assert!(README.contains("npx skills add sunfmin/wxread"), "the README says how to add it");
}
