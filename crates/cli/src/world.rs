//! What every command reads first: the data directory and the keys the
//! flags and the environment name, and the accounts under that directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::bail;
use wxread_doctor::{
    AccountSeen, DATA_DIR_FLAG, DATA_DIR_UNDER_HOME, DATA_DIR_VAR, DB_STORAGE, KEYS_VAR, KeysSeen,
    Listing, Seen, Source,
};

/// What a command reads: the data directory and the keys, each with where
/// it was named.
pub struct World {
    data_dir: PathBuf,
    data_dir_from: Source,
    keys_file: Option<PathBuf>,
    keys_var: Option<String>,
}

impl World {
    /// The world the flags and the environment name: a flag over its
    /// variable, and an empty variable as none.
    pub fn from_env(
        data_dir: Option<PathBuf>,
        keys_file: Option<PathBuf>,
    ) -> anyhow::Result<World> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let (data_dir, data_dir_from) = match (data_dir, var(DATA_DIR_VAR)) {
            (Some(dir), _) => (dir, Source::Flag),
            (None, Some(dir)) => (PathBuf::from(dir), Source::Var),
            (None, None) => match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
                Some(home) => (PathBuf::from(home).join(DATA_DIR_UNDER_HOME), Source::Default),
                None => bail!(
                    "HOME is not set, so WeChat's data directory is not known: set HOME, or pass {DATA_DIR_FLAG} <dir>"
                ),
            },
        };
        Ok(World { data_dir, data_dir_from, keys_file, keys_var: var(KEYS_VAR) })
    }

    /// The data directory, and what named it.
    pub fn data_dir(&self) -> (String, Source) {
        (self.data_dir.display().to_string(), self.data_dir_from)
    }

    /// The accounts the data directory holds.
    pub fn listing(&self) -> Listing {
        listing(&self.data_dir)
    }

    /// Everything doctor judges.
    pub fn seen(&self) -> Seen {
        Seen {
            data_dir: self.data_dir.display().to_string(),
            data_dir_from: self.data_dir_from,
            listing: self.listing(),
            keys: self.keys(),
        }
    }

    /// The keys as supplied: the flag's file over the variable.
    pub fn keys(&self) -> KeysSeen {
        match (&self.keys_file, &self.keys_var) {
            (Some(file), _) => match fs::read_to_string(file) {
                Ok(text) => KeysSeen::Text { from: Source::Flag, text },
                Err(e) => {
                    KeysSeen::Unreadable { file: file.display().to_string(), why: e.to_string() }
                }
            },
            (None, Some(text)) => KeysSeen::Text { from: Source::Var, text: text.clone() },
            (None, None) => KeysSeen::None,
        }
    }
}

/// The accounts under the data directory `dir`: each directory there that
/// holds a `db_storage`, in the order of their names.
fn listing(dir: &Path) -> Listing {
    match fs::metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Listing::Missing,
        Err(e) => return Listing::Unreadable(e.to_string()),
        Ok(meta) if !meta.is_dir() => return Listing::NotADirectory,
        Ok(_) => {}
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => return Listing::Unreadable(e.to_string()),
    };
    let mut accounts: Vec<AccountSeen> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let storage = entry.path().join(DB_STORAGE);
            storage.is_dir().then(|| AccountSeen {
                id: entry.file_name().to_string_lossy().into_owned(),
                db_storage: storage.display().to_string(),
                databases: databases(&storage),
            })
        })
        .collect();
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    Listing::Accounts(accounts)
}

/// Every `.db` file under `storage`, by its path there with `/` between
/// its parts, sorted; or why they could not all be listed.
fn databases(storage: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut dirs = vec![storage.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| format!("{}: {e}", path.display()))?;
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "db") {
                let under = path.strip_prefix(storage).unwrap_or(&path);
                let parts: Vec<_> =
                    under.components().map(|c| c.as_os_str().to_string_lossy()).collect();
                found.push(parts.join("/"));
            }
        }
    }
    found.sort();
    Ok(found)
}
