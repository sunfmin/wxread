//! `wxread keys`: captures every database's key from the running WeChat with
//! Frida and prints them (docs/adr/0005-keys-from-the-crypto-call.md). The
//! capture is [`crate::capture`]'s; this reads the first page of each database
//! and asks [`wxread_crack`] which captured keys open which databases. It
//! keeps nothing and writes nothing.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wxread_crack::Target;
use wxread_doctor::Listing;
use wxread_sqlcipher::PAGE;

use crate::capture;
use crate::world::World;

/// How long the capture runs, in seconds, when `--seconds` is not given.
const DEFAULT_SECONDS: u64 = 25;
/// The longest a capture may be asked to run.
const MAX_SECONDS: u64 = 600;

/// Captures the keys and answers them as one JSON object, or fails with the
/// one line that says why there are none and how to get them.
pub fn run(data_dir: Option<PathBuf>, seconds: Option<String>) -> anyhow::Result<Value> {
    let seconds = parse_seconds(seconds)?;
    let candidates = match capture::probe(seconds) {
        Ok(candidates) => candidates,
        Err(wxread_crack::Failure(line)) => return Err(anyhow::anyhow!("{line}")),
    };
    // Build the databases to prove the captured keys against only after a
    // capture that ran, under the home `sudo` was invoked from.
    let dbs = targets(World::from_env(data_dir, None)?.listing());
    match wxread_crack::verify(&candidates, &dbs) {
        Ok(keys) => Ok(keys.to_value()),
        Err(wxread_crack::Failure(line)) => Err(anyhow::anyhow!("{line}")),
    }
}

/// The capture length `--seconds` asks for: a whole number of seconds from 1
/// to [`MAX_SECONDS`], or [`DEFAULT_SECONDS`] when it is not given.
fn parse_seconds(seconds: Option<String>) -> anyhow::Result<u64> {
    let Some(text) = seconds else {
        return Ok(DEFAULT_SECONDS);
    };
    match text.parse::<u64>() {
        Ok(n) if (1..=MAX_SECONDS).contains(&n) => Ok(n),
        _ => Err(anyhow::anyhow!(
            "--seconds must be a whole number of seconds from 1 to {MAX_SECONDS}: pass `--seconds <N>`, or leave it off for {DEFAULT_SECONDS}"
        )),
    }
}

/// Every database under every account, each with its first page, for the
/// captured keys to be matched against. A database shorter than one page, or
/// one that cannot be read, is left out.
fn targets(listing: Listing) -> Vec<Target> {
    let Listing::Accounts(accounts) = listing else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for account in &accounts {
        let Ok(databases) = &account.databases else {
            continue;
        };
        for path in databases {
            let file = Path::new(&account.db_storage).join(path);
            if let Some(page1) = first_page(&file) {
                targets.push(Target { path: path.clone(), page1 });
            }
        }
    }
    targets
}

/// The first SQLCipher page of a database, or `None` when the file is shorter
/// than one page (so not an encrypted database) or cannot be read.
fn first_page(file: &Path) -> Option<[u8; PAGE]> {
    let mut reader = std::fs::File::open(file).ok()?;
    let mut page = [0u8; PAGE];
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
