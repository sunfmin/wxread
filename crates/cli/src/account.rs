//! Opening one account's databases: each decrypted with its own key into
//! memory, its WAL's committed frames laid over it, and opened from there
//! read-only, so nothing decrypted is written to disk
//! (docs/adr/0003-databases-are-read-in-memory.md).

use std::io::ErrorKind;
use std::path::PathBuf;
use std::ptr::NonNull;

use anyhow::anyhow;
use rusqlite::serialize::OwnedData;
use rusqlite::{Connection, DatabaseName};
use wxread_keys::Keys;

use crate::world::World;

/// The account a command reads, and the keys of its databases.
pub struct Account {
    id: String,
    storage: PathBuf,
    keys: Keys,
}

impl Account {
    /// The account `wanted` names, or the one there is, with the keys
    /// supplied; `needed` the databases the command reads, which a refusal
    /// of missing keys names.
    pub fn open(world: &World, wanted: Option<&str>, needed: &[&str]) -> anyhow::Result<Account> {
        let (dir, from) = world.data_dir();
        let listing = world.listing();
        let seen = wxread_account::choose(&dir, from, &listing, wanted).map_err(|e| anyhow!(e))?;
        let keys = wxread_account::keys(&world.keys(), needed).map_err(|e| anyhow!(e))?;
        Ok(Account { id: seen.id.clone(), storage: PathBuf::from(&seen.db_storage), keys })
    }

    /// The account's id: its directory's name.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Database `path` (`session/session.db`), as it is on disk now with
    /// its WAL, opened from memory and read-only.
    pub fn database(&self, path: &str) -> anyhow::Result<Connection> {
        let key = wxread_account::key_of(&self.keys, &self.id, path).map_err(|e| anyhow!(e))?;
        let file = self.storage.join(path);
        let unreadable =
            |e: std::io::Error| anyhow!(wxread_account::unreadable(&self.id, path, &e.to_string()));
        let db = std::fs::read(&file).map_err(unreadable)?;
        let wal = match std::fs::read(format!("{}-wal", file.display())) {
            Ok(wal) => wal,
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(unreadable(e)),
        };
        let plain = wxread_sqlcipher::decrypt(key, &db, &wal)
            .map_err(|r| anyhow!(wxread_account::refused(&self.id, path, r)))?;
        let not_wechats =
            |e: rusqlite::Error| anyhow!(wxread_account::not_wechats(path, &e.to_string()));
        let mut conn = Connection::open_in_memory().map_err(not_wechats)?;
        conn.deserialize(DatabaseName::Main, sqlite_owned(&plain)?, true).map_err(not_wechats)?;
        // What a query sorts stays in memory too.
        conn.execute_batch("PRAGMA temp_store = MEMORY;").map_err(not_wechats)?;
        Ok(conn)
    }
}

/// `bytes` copied into memory SQLite allocated, which a database opened
/// from memory takes and frees.
fn sqlite_owned(bytes: &[u8]) -> anyhow::Result<OwnedData> {
    // SAFETY: sqlite3_malloc64 answers `bytes.len()` writable bytes or
    // null; they are filled before SQLite is handed them, as OwnedData
    // requires memory sqlite3_malloc allocated.
    unsafe {
        let ptr = rusqlite::ffi::sqlite3_malloc64(bytes.len() as u64).cast::<u8>();
        let ptr = NonNull::new(ptr).ok_or_else(|| anyhow!("SQLite could not allocate the {} bytes of a database: free some memory, and run it again", bytes.len()))?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr(), bytes.len());
        Ok(OwnedData::from_raw_nonnull(ptr, bytes.len()))
    }
}
