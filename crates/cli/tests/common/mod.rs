//! The world a test runs wxread in: a home of its own, with WeChat's data
//! directory under it when the test makes one, and no variable of the host.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde_json::Value;
use tempfile::TempDir;
use wxread_doctor::{DATA_DIR_UNDER_HOME, DB_STORAGE};

pub const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

pub struct World {
    _root: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
}

/// One answer of the CLI: the JSON object it printed, and its exit code.
#[derive(Debug)]
pub struct Answer {
    pub json: Value,
    pub code: i32,
}

impl Answer {
    pub fn ok(self) -> Value {
        assert_eq!(self.code, 0, "{}", self.json);
        self.json
    }

    pub fn error(self) -> String {
        assert_ne!(self.code, 0, "expected a failure, got {}", self.json);
        let error = self.json["error"].as_str().expect("a failure prints {\"error\": …}");
        assert_eq!(self.json.as_object().unwrap().len(), 1, "{}", self.json);
        error.to_string()
    }
}

impl World {
    /// A home with nothing of WeChat's in it.
    pub fn new() -> World {
        let root = TempDir::new().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let home = dir.join("home");
        std::fs::create_dir(&home).unwrap();
        World { _root: root, root: dir, home }
    }

    /// WeChat's data directory where WeChat for Mac keeps it.
    pub fn data_dir(&self) -> PathBuf {
        self.home.join(DATA_DIR_UNDER_HOME)
    }

    /// An account `id` under `data_dir`, with an empty file for each of
    /// its `databases`; its db_storage.
    pub fn account(&self, data_dir: &Path, id: &str, databases: &[&str]) -> PathBuf {
        let storage = data_dir.join(id).join(DB_STORAGE);
        std::fs::create_dir_all(&storage).unwrap();
        for db in databases {
            let path = storage.join(db);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"").unwrap();
        }
        storage
    }

    /// The keys of `databases`, in the shape `wxread keys` answers.
    pub fn keys(databases: &[&str]) -> String {
        let keyed = databases.iter().map(|db| (*db, [KEY.to_string(), KEY[..32].to_string()]));
        Value::Object(keys_shown(keyed)).to_string()
    }

    /// Writes `text` to a file in the world, and says where.
    pub fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    /// wxread run with `args`, `env` its only variables beside HOME.
    pub fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Answer {
        let mut command = self.command(args);
        command.envs(env.iter().copied());
        let out = command.output().unwrap();
        assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "one JSON object, on one line: {text}");
        Answer { json: serde_json::from_str(&text).unwrap(), code: out.status.code().unwrap() }
    }

    /// The command line of wxread with `args`, in this world.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wxread"));
        command.args(args).env_clear().env("HOME", &self.home);
        command
    }
}

/// The keys of each database of `keyed`, its path under db_storage with the
/// hex of its key and its salt, as `wxread keys --help` shows them: its
/// example filled in, each `<n hex>` with the one of the two that has n
/// digits, so the keys every test supplies are in the shape the help shows.
pub fn keys_shown<'a>(
    keyed: impl IntoIterator<Item = (&'a str, [String; 2])>,
) -> serde_json::Map<String, Value> {
    let blocks = agent_help::code_blocks(&wxread::manual(), "json");
    let shown: Vec<_> = blocks.iter().filter(|b| b.at == "`wxread keys`").collect();
    let [shown] = shown.as_slice() else { panic!("one example of the keys: {shown:?}") };
    let example: serde_json::Map<String, Value> = serde_json::from_str(&shown.code).unwrap();
    let [entry] = example.values().collect::<Vec<_>>()[..] else { panic!("{example:?}") };
    let mut keys = serde_json::Map::new();
    for (db, hexes) in keyed {
        let mut filled = entry.as_object().unwrap().clone();
        for value in filled.values_mut() {
            let digits = value.as_str().and_then(|v| v.strip_prefix('<')?.strip_suffix(" hex>"));
            let digits: usize = digits.unwrap_or_else(|| panic!("{value}")).parse().unwrap();
            let hex = hexes.iter().find(|h| h.len() == digits).unwrap_or_else(|| panic!("{value}"));
            *value = Value::from(hex.as_str());
        }
        keys.insert(db.to_string(), Value::Object(filled));
    }
    keys
}

/// Every path under `dir`, with its bytes and when it was last changed.
pub fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut all = BTreeMap::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::metadata(&path).unwrap();
            let bytes = if meta.is_dir() {
                dirs.push(path.clone());
                Vec::new()
            } else {
                std::fs::read(&path).unwrap()
            };
            all.insert(path, (bytes, meta.modified().unwrap()));
        }
    }
    all
}
