//! `wxread keys`: reads every database's key out of the running WeChat's
//! memory and prints them (docs/adr/0004-keys-from-memory.md). It reads the
//! world — whether wxread is root, whether WeChat is running and readable,
//! and the first page of each database — and asks [`wxread_crack`] which keys
//! open which databases. It keeps nothing and writes nothing.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wxread_crack::{Access, Request, Target};
use wxread_doctor::Listing;
use wxread_sqlcipher::PAGE;

use crate::memory;
use crate::world::World;

/// Reads the keys and answers them as one JSON object, or fails with the one
/// line that says why there are none and how to get them.
pub fn run(data_dir: Option<PathBuf>) -> anyhow::Result<Value> {
    // Only root can read another process's memory; everything else is decided
    // only once wxread is root and has (or could not get) WeChat's memory.
    let is_root = unsafe { libc::geteuid() } == 0;
    let (access, candidates, dbs) = if is_root {
        let (access, candidates) = memory::probe();
        let dbs = match access {
            Access::Read => targets(World::from_env(data_dir, None)?.listing()),
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
/// to be matched against. A database shorter than one page, or one that
/// cannot be read, is left out.
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
