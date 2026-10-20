//! `wxread resign`: re-signs WeChat ad-hoc with `get-task-allow` added to
//! its entitlements, so `sudo wxread keys` can read its memory
//! (docs/adr/0005-wxread-resign-re-signs-wechat-when-asked.md). Without
//! `--yes` it only says what it would do. This module runs `codesign`, the
//! one place its command lines are built; [`wxread_resign`] decides what to
//! sign with and what to answer.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use wxread_resign::{APPS, Failure, Merge};

/// Says what re-signing the app at `app` (else WeChat where it is
/// installed) would do, or with `yes` does it, and answers what changed.
pub fn run(app: Option<PathBuf>, yes: bool) -> anyhow::Result<Value> {
    let named = app.is_some();
    let app = match app {
        Some(app) => {
            match std::fs::metadata(app.join("Contents")) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => {
                    return Err(fail(wxread_resign::not_an_app(
                        &shown(&app),
                        "it holds no Contents directory",
                    )));
                }
                Err(e) => {
                    return Err(fail(wxread_resign::not_an_app(&shown(&app), &e.to_string())));
                }
            }
            app
        }
        None => match APPS.iter().map(PathBuf::from).find(|p| p.is_dir()) {
            Some(app) => app,
            None => return Err(fail(wxread_resign::not_found())),
        },
    };
    let name = shown(&app);
    let retry = wxread_resign::line(named.then_some(name.as_str()), yes);
    let merge = wxread_resign::merge(&name, &entitlements(&app, &retry)?).map_err(fail)?;
    let plist = match &merge {
        Merge::Add { plist, .. } if yes => plist,
        _ => return Ok(wxread_resign::answer(&name, named, &merge, false)),
    };
    sign(&app, plist, &retry)?;
    // What it answers it changed is what codesign now reports: every
    // entitlement kept (codesign may sort them) and get-task-allow carried.
    let sorted = |kept: &[String]| {
        let mut kept = kept.to_vec();
        kept.sort();
        kept
    };
    match wxread_resign::merge(&name, &entitlements(&app, &retry)?).map_err(fail)? {
        Merge::Already { kept } if sorted(&kept) == sorted(merge.kept()) => {}
        _ => return Err(fail(wxread_resign::not_taken(&name, &retry))),
    }
    Ok(wxread_resign::answer(&name, named, &merge, true))
}

/// The entitlements `app` is signed with, as the XML plist codesign prints
/// (empty when it has none).
fn entitlements(app: &Path, retry: &str) -> anyhow::Result<String> {
    let out = Command::new("codesign")
        .args(["-d", "--entitlements", "-", "--xml"])
        .arg(app)
        .output()
        .map_err(|e| missing(e, retry))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(fail(wxread_resign::unreadable(&shown(app), why.trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Re-signs `app` ad-hoc with the entitlements `plist`. codesign reads
/// entitlements only from a file it can seek (not a pipe), so they are
/// written to one in the temporary directory, removed once codesign is done.
fn sign(app: &Path, plist: &str, retry: &str) -> anyhow::Result<()> {
    let file = std::env::temp_dir().join(format!("wxread-resign-{}.plist", std::process::id()));
    std::fs::write(&file, plist)
        .map_err(|e| anyhow::anyhow!("{} could not be written ({e}): set TMPDIR to a directory wxread may write, then run `{retry}` again", file.display()))?;
    let out = Command::new("codesign")
        .args(["--force", "--sign", "-", "--entitlements"])
        .arg(&file)
        .arg(app)
        .output();
    let _ = std::fs::remove_file(&file);
    let out = out.map_err(|e| missing(e, retry))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(fail(wxread_resign::sign_failed(&shown(app), &why, retry)));
    }
    Ok(())
}

/// Why codesign could not be started: missing, or `e` as it is.
fn missing(e: std::io::Error, retry: &str) -> anyhow::Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        fail(wxread_resign::codesign_missing(retry))
    } else {
        anyhow::anyhow!("codesign could not be run ({e}): run `{retry}` again")
    }
}

/// The path as the answer and the failures show it.
fn shown(app: &Path) -> String {
    app.display().to_string()
}

/// A pure crate's failure, as the error the run prints.
fn fail(Failure(line): Failure) -> anyhow::Error {
    anyhow::anyhow!("{line}")
}
