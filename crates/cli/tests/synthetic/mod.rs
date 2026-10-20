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
use wxread_schema::{CHAT_ROOM, CONTACT, CONTACT_DB, NAME2ID, SESSION, SESSION_DB, ZSTD};
use wxread_sqlcipher::{PAGE, RESERVE};

/// A contact of the account: its id, its own name, the account's remark.
pub struct Contact(pub &'static str, pub &'static str, pub &'static str);

/// A chat in `SessionTable`: its id, when its last message was (which is
/// where WeChat sorts it, in seconds), and its unread messages.
pub struct Session(pub &'static str, pub i64, pub i64);

/// A group's members, each by their id and the name they gave themselves
/// in it.
pub struct Members(pub &'static str, pub Vec<(&'static str, &'static str)>);

/// A message of chat `chat`, in message database `db` (`message_0`): its
/// server id, type, when it was sent, its order in that second, its
/// sender, its content as WeChat keeps it (zstd-compressed when `zstd`
/// says), and its `packed_info_data`.
#[derive(Clone)]
pub struct Said {
    pub db: &'static str,
    pub chat: &'static str,
    pub server_id: i64,
    pub local_type: i64,
    pub create_time: i64,
    pub sort_seq: i64,
    pub sender: Option<&'static str>,
    pub content: String,
    pub zstd: bool,
    pub packed: Vec<u8>,
}

impl Said {
    /// A text `content` said by `sender` in `chat` at `create_time`, in
    /// message_0.
    pub fn text(
        chat: &'static str,
        server_id: i64,
        create_time: i64,
        sender: &'static str,
        content: &str,
    ) -> Said {
        Said {
            db: "message_0",
            chat,
            server_id,
            local_type: 1,
            create_time,
            sort_seq: create_time * 1000,
            sender: Some(sender),
            content: content.to_string(),
            zstd: false,
            packed: Vec::new(),
        }
    }
}

/// One account's directory under a data directory, and what its databases
/// hold: `contacts`, `members`, `sessions` and `messages` in the databases
/// themselves, `recent` sessions and `recent_messages` only in committed
/// frames of their database's WAL, and `uncommitted` sessions in frames
/// after them that no commit ended.
pub struct Account {
    pub id: &'static str,
    pub storage: PathBuf,
    pub contacts: Vec<Contact>,
    pub members: Vec<Members>,
    pub sessions: Vec<Session>,
    pub recent: Vec<Session>,
    pub uncommitted: Vec<Session>,
    pub messages: Vec<Said>,
    pub recent_messages: Vec<Said>,
}

/// The databases a synthetic account has.
pub const DATABASES: [&str; 2] = [CONTACT_DB, SESSION_DB];

impl Account {
    pub fn new(data_dir: &Path, id: &'static str) -> Account {
        Account {
            id,
            storage: data_dir.join(id).join(DB_STORAGE),
            contacts: Vec::new(),
            members: Vec::new(),
            sessions: Vec::new(),
            recent: Vec::new(),
            uncommitted: Vec::new(),
            messages: Vec::new(),
            recent_messages: Vec::new(),
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

    /// Every database it has: the contact and session databases, and each
    /// message database its messages are in.
    pub fn databases(&self) -> Vec<String> {
        let mut all: Vec<String> = DATABASES.iter().map(|d| d.to_string()).collect();
        for said in self.messages.iter().chain(&self.recent_messages) {
            let path = format!("message/{}.db", said.db);
            if !all.contains(&path) {
                all.push(path);
            }
        }
        all
    }

    /// The keys of every database it has.
    pub fn all_keys(&self) -> String {
        let all = self.databases();
        self.keys(&all.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// Writes every database, encrypted, and the WAL of the session
    /// database and of each message database.
    pub fn write(&self) {
        let contacts = plain(&format!("{CONTACT}; {CHAT_ROOM}"));
        for (i, Contact(id, name, remark)) in self.contacts.iter().enumerate() {
            contacts
                .execute(
                    "INSERT INTO contact(id, username, nick_name, remark, local_type) VALUES (?1, ?2, ?3, ?4, 1)",
                    params![i as i64 + 1, id, name, remark],
                )
                .unwrap();
        }
        for (i, Members(group, members)) in self.members.iter().enumerate() {
            let buffer: Vec<u8> = members
                .iter()
                .flat_map(|(id, name)| {
                    field(1, &[field(1, id.as_bytes()), field(2, name.as_bytes())].concat())
                })
                .collect();
            contacts
                .execute(
                    "INSERT INTO chat_room(id, username, owner, ext_buffer) VALUES (?1, ?2, '', ?3)",
                    params![i as i64 + 1, group, buffer],
                )
                .unwrap();
        }
        self.seal(CONTACT_DB, &bytes(&contacts), &[]);
        let mut dbs: Vec<&str> =
            self.messages.iter().chain(&self.recent_messages).map(|s| s.db).collect();
        dbs.sort();
        dbs.dedup();
        for db in dbs {
            let conn = plain(NAME2ID);
            let of = |all: &[Said]| all.iter().filter(|s| s.db == db).cloned().collect::<Vec<_>>();
            said(&conn, &of(&self.messages));
            let before = bytes(&conn);
            said(&conn, &of(&self.recent_messages));
            let after = bytes(&conn);
            let path = format!("message/{db}.db");
            self.seal(&path, &before, &[(&before, &after, true)]);
        }
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

/// Writes `messages` into the message database `db`: each chat's table,
/// made the first time one of its messages is written, and each sender in
/// `Name2Id`.
fn said(db: &Connection, messages: &[Said]) {
    for m in messages {
        let table = wxread_schema::messages_table(m.chat);
        db.execute_batch(
            &wxread_schema::messages(&table).replace("CREATE TABLE", "CREATE TABLE IF NOT EXISTS"),
        )
        .unwrap();
        let sender: Option<i64> = m.sender.map(|s| {
            db.execute("INSERT OR IGNORE INTO Name2Id(user_name, is_session) VALUES (?1, 0)", [s])
                .unwrap();
            db.query_row("SELECT rowid FROM Name2Id WHERE user_name = ?1", [s], |r| r.get(0))
                .unwrap()
        });
        let content: rusqlite::types::Value = if m.zstd {
            ruzstd::encoding::compress_to_vec(
                m.content.as_bytes(),
                ruzstd::encoding::CompressionLevel::Fastest,
            )
            .into()
        } else {
            m.content.clone().into()
        };
        db.execute(
            &format!("INSERT INTO {table}(server_id, local_type, sort_seq, real_sender_id, create_time, message_content, packed_info_data, WCDB_CT_message_content) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![m.server_id, m.local_type, m.sort_seq, sender, m.create_time, content, m.packed, m.zstd.then_some(ZSTD)],
        )
        .unwrap();
    }
}

/// Protobuf field `number` holding `value`, length-delimited.
pub fn field(number: u8, value: &[u8]) -> Vec<u8> {
    let mut out = vec![number << 3 | 2];
    let mut len = value.len();
    loop {
        let b = (len & 0x7f) as u8;
        len >>= 7;
        if len == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out.extend_from_slice(value);
    out
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
