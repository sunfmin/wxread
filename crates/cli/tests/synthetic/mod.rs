//! Synthetic WeChat 4.x accounts: databases made from the tables
//! `wxread_schema` declares, each encrypted under a test key of its own as
//! WeChat encrypts one, with what was written last in the `-wal` beside it,
//! and the keys in the shape `wxread keys` answers. Nobody's chats: every
//! row is made up here, and the plain databases are made in memory, so no
//! test writes a decrypted byte either.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use rusqlite::{Connection, DatabaseName, params};
use wxread_doctor::DB_STORAGE;
use wxread_schema::{CONTACT, CONTACT_DB, SESSION, SESSION_DB};
use wxread_sqlcipher::{PAGE, RESERVE};

/// A contact of the account: its id, its own name, the account's remark.
pub struct Contact(pub &'static str, pub &'static str, pub &'static str);

/// A chat in `SessionTable`: its id, when its last message was (which is
/// where WeChat sorts it, in seconds), and its unread messages.
pub struct Session(pub &'static str, pub i64, pub i64);

/// One account's directory under a data directory, and what its databases
/// hold: `contacts` and `sessions` in the databases themselves, `recent`
/// sessions only in committed frames of the session database's WAL, and
/// `uncommitted` in frames after them that no commit ended.
pub struct Account {
    pub id: &'static str,
    pub storage: PathBuf,
    pub contacts: Vec<Contact>,
    pub sessions: Vec<Session>,
    pub recent: Vec<Session>,
    pub uncommitted: Vec<Session>,
}

/// The databases a synthetic account has.
pub const DATABASES: [&str; 2] = [CONTACT_DB, SESSION_DB];

impl Account {
    pub fn new(data_dir: &Path, id: &'static str) -> Account {
        Account {
            id,
            storage: data_dir.join(id).join(DB_STORAGE),
            contacts: Vec::new(),
            sessions: Vec::new(),
            recent: Vec::new(),
            uncommitted: Vec::new(),
        }
    }

    /// The account's directory, above its db_storage.
    pub fn dir(&self) -> PathBuf {
        self.storage.parent().unwrap().to_path_buf()
    }

    /// The test key of database `path`: its own, as WeChat's are.
    pub fn key(&self, path: &str) -> [u8; 32] {
        let mut key = [0u8; 32];
        let seed = format!("{}/{path}", self.id);
        for (i, b) in seed.bytes().cycle().take(32).enumerate() {
            key[i] = b.wrapping_mul(31).wrapping_add(i as u8);
        }
        key
    }

    /// The salt database `path` starts with.
    pub fn salt(&self, path: &str) -> [u8; 16] {
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&self.key(path)[8..24]);
        salt
    }

    /// The keys of `databases`, in the shape `wxread keys` answers, with a
    /// tool's own note beside them.
    pub fn keys(&self, databases: &[&str]) -> String {
        let entries: Vec<String> = databases
            .iter()
            .map(|db| {
                format!(
                    "\"{db}\": {{\"enc_key\": \"{}\", \"salt\": \"{}\"}}",
                    hex(&self.key(db)),
                    hex(&self.salt(db))
                )
            })
            .collect();
        format!("{{\"_db_dir\": \"{}\", {}}}", self.storage.display(), entries.join(", "))
    }

    /// Writes every database, encrypted, and the session database's WAL.
    pub fn write(&self) {
        let contacts = plain(CONTACT);
        for (i, Contact(id, name, remark)) in self.contacts.iter().enumerate() {
            contacts
                .execute(
                    "INSERT INTO contact(id, username, nick_name, remark, local_type) VALUES (?1, ?2, ?3, ?4, 1)",
                    params![i as i64 + 1, id, name, remark],
                )
                .unwrap();
        }
        self.seal(CONTACT_DB, &bytes(&contacts), &[]);
        let sessions = plain(SESSION);
        insert(&sessions, &self.sessions);
        let before = bytes(&sessions);
        insert(&sessions, &self.recent);
        let committed = bytes(&sessions);
        insert(&sessions, &self.uncommitted);
        let after = bytes(&sessions);
        self.seal(SESSION_DB, &before, &[(&before, &committed, true), (&committed, &after, false)]);
    }

    /// Writes database `path`, `plain` encrypted, and a WAL of each change
    /// in `changes`: the pages that differ from the one before, the last of
    /// them ending a commit when it says so.
    fn seal(&self, path: &str, plain: &[u8], changes: &[(&[u8], &[u8], bool)]) {
        let (key, salt) = (self.key(path), self.salt(path));
        let iv = |n: u32| {
            let mut iv = [n as u8; 16];
            iv[..4].copy_from_slice(&n.to_be_bytes());
            iv
        };
        let file = self.storage.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, wxread_sqlcipher::encrypt(plain, &key, &salt, iv)).unwrap();
        let mut frames: Vec<(u32, u32, [u8; PAGE])> = Vec::new();
        for (from, to, commit) in changes {
            let sealed = wxread_sqlcipher::encrypt(to, &key, &salt, |n| iv(n ^ 0x80));
            let pages = (to.len() / PAGE) as u32;
            let changed: Vec<u32> =
                (1..=pages).filter(|&n| page(from, n) != Some(page(to, n).unwrap())).collect();
            for (i, &n) in changed.iter().enumerate() {
                let size = if *commit && i + 1 == changed.len() { pages } else { 0 };
                frames.push((n, size, *page(&sealed, n).unwrap()));
            }
        }
        if !frames.is_empty() {
            let frames: Vec<(u32, u32, &[u8; PAGE])> =
                frames.iter().map(|(n, s, p)| (*n, *s, p)).collect();
            std::fs::write(
                format!("{}-wal", file.display()),
                wxread_sqlcipher::wal(&frames, (11, 22)),
            )
            .unwrap();
        }
    }
}

fn insert(db: &Connection, sessions: &[Session]) {
    for Session(id, at, unread) in sessions {
        db.execute(
            "INSERT INTO SessionTable(username, type, unread_count, is_hidden, summary, last_timestamp, sort_timestamp) VALUES (?1, 0, ?3, 0, 'hi', ?2, ?2)",
            params![id, at, unread],
        )
        .unwrap();
    }
}

/// Page `n` of `db`, when it has one.
fn page(db: &[u8], n: u32) -> Option<&[u8; PAGE]> {
    let at = (n as usize - 1) * PAGE;
    db.get(at..at + PAGE).map(|p| p.try_into().unwrap())
}

/// A database in memory of 4096-byte pages that reserve the cipher's bytes,
/// with the table `declared`.
fn plain(declared: &str) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA page_size = 4096;").unwrap();
    // SQLITE_FCNTL_RESERVE_BYTES: the bytes each page keeps for the
    // cipher's IV and HMAC, set before the first page is written.
    let mut reserve: std::os::raw::c_int = RESERVE as _;
    // SAFETY: the handle is the open connection's, and the pointer is to
    // an int, as SQLITE_FCNTL_RESERVE_BYTES takes.
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            db.handle(),
            c"main".as_ptr(),
            38,
            (&mut reserve as *mut std::os::raw::c_int).cast(),
        )
    };
    assert_eq!(rc, 0, "set the reserved bytes");
    db.execute_batch(declared).unwrap();
    db
}

/// The bytes of `db` as a file of it would hold them, in WAL mode as
/// WeChat keeps its databases.
fn bytes(db: &Connection) -> Vec<u8> {
    let mut bytes = db.serialize(DatabaseName::Main).unwrap().to_vec();
    assert_eq!(bytes[20] as usize, RESERVE, "each page reserves the cipher's bytes");
    assert_eq!(bytes.len() % PAGE, 0);
    bytes[18] = 2;
    bytes[19] = 2;
    bytes
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
