//! `wxread keys`: reads every database's key out of the running WeChat's
//! memory and prints them (docs/adr/0003-keys-from-memory.md). It reads the
//! world — whether wxread is root, whether WeChat is running and readable,
//! and the first page of each database — and asks [`wxread_crack`] which keys
//! open which databases. It keeps nothing and writes nothing.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wxread_crack::{Access, Request, Target};
use wxread_doctor::{DATA_DIR_VAR, DB_STORAGE};

use crate::{memory, paths};

/// Reads the keys and answers them as one JSON object, or fails with the one
/// line that says why there are none and how to get them.
pub fn run(data_dir: Option<PathBuf>) -> anyhow::Result<Value> {
    // Only root can read another process's memory; everything else is decided
    // only once wxread is root and has (or could not get) WeChat's memory.
    let is_root = unsafe { libc::geteuid() } == 0;
    let (access, candidates, dbs) = if is_root {
        let (access, candidates) = memory::probe();
        let dbs = match access {
            Access::Read => targets(data_dir)?,
            _ => Vec::new(),
        };
        (access, candidates, dbs)
    } else {
        (Access::NotRoot, Vec::new(), Vec::new())
    };
    let request = Request { access, candidates: &candidates, dbs: &dbs };
    match wxread_crack::run(&request) {
        Ok(keys) => Ok(keys.to_value()),
        Err(wxread_crack::Failure(line)) => Err(anyhow::anyhow!("{line}")),
    }
}

/// Every database under every account, each with its first page, for the keys
/// to be matched against. Under `sudo` the data directory is the invoking
/// user's, not root's.
fn targets(data_dir: Option<PathBuf>) -> anyhow::Result<Vec<Target>> {
    let (dir, _from) = paths::data_dir(data_dir, paths::var(DATA_DIR_VAR), real_home())?;
    let mut targets = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(targets);
    };
    for entry in entries.flatten() {
        let storage = entry.path().join(DB_STORAGE);
        if !storage.is_dir() {
            continue;
        }
        if let Ok(databases) = paths::databases(&storage) {
            for (path, file) in databases {
                if let Some(page1) = first_page(&file) {
                    targets.push(Target { path, page1 });
                }
            }
        }
    }
    Ok(targets)
}

/// The first SQLCipher page of a database, or `None` when the file is shorter
/// than one page (so not an encrypted database) or cannot be read.
fn first_page(file: &Path) -> Option<Vec<u8>> {
    let mut reader = std::fs::File::open(file).ok()?;
    let mut page = vec![0u8; wxread_crack::PAGE];
    let mut read = 0;
    while read < page.len() {
        match reader.read(&mut page[read..]) {
            Ok(0) => return None,
            Ok(n) => read += n,
            Err(_) => return None,
        }
    }
    Some(page)
}

/// The home directory WeChat's files are under: the user `sudo` was run by,
/// when there is one, else this process's own `HOME`.
fn real_home() -> Option<PathBuf> {
    if let Some(user) = paths::var("SUDO_USER")
        && let Some(home) = home_of(&user)
    {
        return Some(home);
    }
    std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from)
}

/// The home directory of user `name` from the password database, if it has
/// one.
fn home_of(name: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    let name = std::ffi::CString::new(name).ok()?;
    // getpwnam is not re-entrant, but wxread is one short-lived process.
    let pw = unsafe { libc::getpwnam(name.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    let dir = unsafe { (*pw).pw_dir };
    if dir.is_null() {
        return None;
    }
    let bytes = unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes();
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}
