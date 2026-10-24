//! `wxread keys`: captures every database's key from the running WeChat with
//! Frida and prints them (docs/adr/0005-keys-from-the-crypto-call.md). The
//! capture is [`crate::capture`]'s; this reads the first page of each database
//! and asks [`wxread_crack`] which captured keys open which databases. As each
//! key is captured it proves it against the databases present then and reports
//! the one it opens on stderr, so a long capture counts up as it goes, and the
//! capture stops once the keys stop coming rather than running out the window
//! ([`crate::capture`]); the one JSON object on stdout, the authoritative
//! answer, is still printed once at the end. It keeps nothing and writes
//! nothing.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wxread_crack::Target;
use wxread_doctor::Listing;
use wxread_sqlcipher::PAGE;

use crate::capture;
use crate::world::World;

/// How long the capture runs, in seconds, when `--seconds` is not given.
pub(crate) const DEFAULT_SECONDS: u64 = 25;
/// The longest a capture may be asked to run.
pub(crate) const MAX_SECONDS: u64 = 600;

/// Captures the keys and answers them as one JSON object, or fails with the
/// one line that says why there are none and how to get them.
pub fn run(data_dir: Option<PathBuf>, seconds: Option<String>) -> anyhow::Result<Value> {
    let seconds = parse_seconds(seconds)?;

    // Keep every captured key for the one authoritative check at the end, and
    // as each arrives prove it against the databases present then: report the
    // one it opens on stderr the moment it is keyed, so a long capture counts
    // up as it runs. Databases appear while WeChat decrypts them (a fresh
    // login writes them one by one), so the listing is read again per key.
    let mut candidates: Vec<[u8; wxread_crack::KEY_LEN]> = Vec::new();
    let mut reported: BTreeSet<String> = BTreeSet::new();
    let watch = |key: &[u8; wxread_crack::KEY_LEN]| {
        candidates.push(*key);
        for db in current_targets(&data_dir) {
            if !reported.contains(&db.path) && wxread_crack::prove(key, &db).is_some() {
                reported.insert(db.path.clone());
                eprintln!("wxread keys: captured {} ({} so far)", db.path, reported.len());
            }
        }
    };
    if let Err(wxread_crack::Failure(line)) = capture::probe(seconds, watch) {
        return Err(anyhow::anyhow!("{line}"));
    }

    // Prove every captured key against the databases as they stand now: the
    // one answer, and the place a key seen before its database's file existed
    // is still caught.
    let dbs = targets(World::from_env(data_dir, None)?.listing());
    match wxread_crack::verify(&candidates, &dbs) {
        Ok(keys) => Ok(keys.to_value()),
        Err(wxread_crack::Failure(line)) => Err(anyhow::anyhow!("{line}")),
    }
}

/// The databases present right now, each with its first page, for proving a
/// key the moment it is captured. Read again on every key because a fresh
/// login writes the databases one at a time; one that cannot be listed or
/// read yet is simply not there yet.
fn current_targets(data_dir: &Option<PathBuf>) -> Vec<Target> {
    match World::from_env(data_dir.clone(), None) {
        Ok(world) => targets(world.listing()),
        Err(_) => Vec::new(),
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
