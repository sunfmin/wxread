//! Where wxread reads WeChat's files from: the one place the data directory
//! is chosen, so `doctor` and `keys` choose it the same way — the `--data-dir`
//! flag, else `WXREAD_DATA_DIR`, else `xwechat_files` under a home directory.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::bail;
use wxread_doctor::{DATA_DIR_FLAG, DATA_DIR_UNDER_HOME, Source};

/// The value of environment variable `name`, or `None` when it is unset or
/// empty.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// WeChat's data directory and where its path came from: `flag` (the
/// `--data-dir` option) first, then `env` (`WXREAD_DATA_DIR`), then
/// `xwechat_files` under `home`. Fails, naming the fix, when none is known.
pub fn data_dir(
    flag: Option<PathBuf>,
    env: Option<String>,
    home: Option<PathBuf>,
) -> anyhow::Result<(PathBuf, Source)> {
    match (flag, env) {
        (Some(dir), _) => Ok((dir, Source::Flag)),
        (None, Some(dir)) => Ok((PathBuf::from(dir), Source::Var)),
        (None, None) => match home {
            Some(home) => Ok((home.join(DATA_DIR_UNDER_HOME), Source::Default)),
            None => bail!(
                "no home directory is known, so WeChat's data directory is not known: set HOME, or pass {DATA_DIR_FLAG} <dir>"
            ),
        },
    }
}

/// Every `.db` file under `storage`, each by its path there (`/` between its
/// parts) and its full path, sorted by the first; or why they could not all
/// be listed.
pub fn databases(storage: &Path) -> Result<Vec<(String, PathBuf)>, String> {
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
                found.push((parts.join("/"), path));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}
