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
use wxread_schema::{
    BIZ_MESSAGE_PREFIX, CHAT_ROOM, CHAT_ROOM_INFO_DETAIL, CHATROOM_MEMBER, CONTACT, CONTACT_DB,
    HEAD_IMAGE, HEAD_IMAGE_DB, NAME2ID, SESSION, SESSION_DB, ZSTD, messages, messages_table,
};
use wxread_sqlcipher::{PAGE, RESERVE};

/// A contact of the account: its id, its own name, the account's remark.
pub struct Contact(pub &'static str, pub &'static str, pub &'static str);

/// More of a contact: its id, its WeChat ID and its signature.
pub struct Profile(pub &'static str, pub &'static str, pub &'static str);

/// A group the account is in: its id (a contact's), its owner, each member
/// with their name in it ("" for none; a member who is no contact is only
/// in its `ext_buffer`), and its notice: text, who posted it, when.
pub struct Group {
    pub id: &'static str,
    pub owner: &'static str,
    pub members: Vec<(&'static str, &'static str)>,
    pub notice: Option<(&'static str, &'static str, i64)>,
}

/// A message an official account sent, in the official accounts' store
/// `message/biz_message_<store>.db`: `from` the official account's id, `at` when
/// it was sent, `local_type` and `content` as WeChat keeps them, the content
/// zstd-compressed when `zstd` says, and only in the store's WAL when `in_wal`.
pub struct Biz {
    pub store: u8,
    pub from: &'static str,
    pub at: i64,
    pub local_type: i64,
    pub content: String,
    pub zstd: bool,
    pub in_wal: bool,
}

/// One article of a push: its title, digest, url and cover.
pub struct Article(pub &'static str, pub &'static str, pub &'static str, pub &'static str);

/// An app message of a link (49, its appmsg type 5), as WeChat stores one.
pub const ARTICLE_TYPE: i64 = 49 | 5 << 32;

impl Biz {
    /// A push of `articles` from official account `from`, named `name` in
    /// it, in store 0, as WeChat writes one: the first article in the
    /// appmsg itself, every article an item of its `mmreader`.
    pub fn push(from: &'static str, name: &str, at: i64, articles: &[Article]) -> Biz {
        let items: String = articles
            .iter()
            .map(|Article(title, digest, url, cover)| {
                format!("<item><itemshowtype>0</itemshowtype><title><![CDATA[{title}]]></title><url><![CDATA[{url}]]></url><pub_time>{at}</pub_time><cover><![CDATA[{cover}]]></cover><digest><![CDATA[{digest}]]></digest></item>")
            })
            .collect();
        let Article(title, digest, url, cover) = &articles[0];
        let content = format!(
            "<msg><appmsg appid=\"\" sdkver=\"0\"><title><![CDATA[{title}]]></title><des><![CDATA[{digest}]]></des><type>5</type><url><![CDATA[{url}]]></url><thumburl><![CDATA[{cover}]]></thumburl><mmreader><category type=\"20\" count=\"{}\"><name><![CDATA[{name}]]></name>{items}</category><publisher><username><![CDATA[{from}]]></username><nickname><![CDATA[{name}]]></nickname></publisher></mmreader></appmsg><fromusername><![CDATA[{from}]]></fromusername></msg>",
            articles.len()
        );
        Biz { store: 0, from, at, local_type: ARTICLE_TYPE, content, zstd: false, in_wal: false }
    }
}

/// A chat in `SessionTable`: its id, when its last message was (which is
/// where WeChat sorts it, in seconds), and its unread messages.
pub struct Session(pub &'static str, pub i64, pub i64);

/// One account's directory under a data directory, and what its databases
/// hold: `contacts` and `sessions` in the databases themselves, `recent`
/// sessions only in committed frames of the session database's WAL, and
/// `uncommitted` in frames after them that no commit ended; `profiles` more
/// of some contacts, `avatars` the contacts WeChat holds an avatar of, and
/// `groups` each group's record and notice; `biz` the messages official
/// accounts sent, in the official accounts' stores.
pub struct Account {
    pub id: &'static str,
    pub storage: PathBuf,
    pub contacts: Vec<Contact>,
    pub sessions: Vec<Session>,
    pub recent: Vec<Session>,
    pub uncommitted: Vec<Session>,
    pub profiles: Vec<Profile>,
    pub avatars: Vec<&'static str>,
    pub groups: Vec<Group>,
    pub biz: Vec<Biz>,
}

/// The databases a synthetic account has.
pub const DATABASES: [&str; 3] = [CONTACT_DB, HEAD_IMAGE_DB, SESSION_DB];

impl Account {
    pub fn new(data_dir: &Path, id: &'static str) -> Account {
        Account {
            id,
            storage: data_dir.join(id).join(DB_STORAGE),
            contacts: Vec::new(),
            sessions: Vec::new(),
            recent: Vec::new(),
            uncommitted: Vec::new(),
            profiles: Vec::new(),
            avatars: Vec::new(),
            groups: Vec::new(),
            biz: Vec::new(),
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
        for Profile(id, alias, signature) in &self.profiles {
            contacts
                .execute(
                    "UPDATE contact SET alias = ?2, description = ?3 WHERE username = ?1",
                    params![id, alias, signature],
                )
                .unwrap();
        }
        contacts
            .execute_batch(&format!("{CHATROOM_MEMBER}; {CHAT_ROOM}; {CHAT_ROOM_INFO_DETAIL};"))
            .unwrap();
        for group in &self.groups {
            for (member, _) in &group.members {
                contacts
                    .execute(
                        "INSERT INTO chatroom_member(room_id, member_id) SELECT g.id, m.id FROM contact g, contact m WHERE g.username = ?1 AND m.username = ?2",
                        params![group.id, member],
                    )
                    .unwrap();
            }
            contacts
                .execute(
                    "INSERT INTO chat_room(id, username, owner, ext_buffer) SELECT id, username, ?2, ?3 FROM contact WHERE username = ?1",
                    params![group.id, group.owner, ext_buffer(&group.members)],
                )
                .unwrap();
            if let Some((text, editor, at)) = group.notice {
                contacts
                    .execute(
                        "INSERT INTO chat_room_info_detail(room_id_, username_, announcement_, announcement_editor_, announcement_publish_time_) SELECT id, username, ?2, ?3, ?4 FROM contact WHERE username = ?1",
                        params![group.id, text, editor, at],
                    )
                    .unwrap();
            }
        }
        self.seal(CONTACT_DB, &bytes(&contacts), &[]);
        let avatars = plain(HEAD_IMAGE);
        for id in &self.avatars {
            avatars
                .execute(
                    "INSERT INTO head_image(username, md5, image_buffer, update_time) VALUES (?1, 'md5', x'ffd8ffe0', 1760000000)",
                    params![id],
                )
                .unwrap();
        }
        self.seal(HEAD_IMAGE_DB, &bytes(&avatars), &[]);
        let sessions = plain(SESSION);
        insert(&sessions, &self.sessions);
        let before = bytes(&sessions);
        insert(&sessions, &self.recent);
        let committed = bytes(&sessions);
        insert(&sessions, &self.uncommitted);
        let after = bytes(&sessions);
        self.seal(SESSION_DB, &before, &[(&before, &committed, true), (&committed, &after, false)]);
        self.write_biz();
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

/// A group's `ext_buffer` as WeChat writes it: a field 1 a member, holding
/// their id (field 1) and their name in the group (field 2) when they set
/// one, each beside a varint WeChat writes there too.
fn ext_buffer(members: &[(&str, &str)]) -> Vec<u8> {
    let field = |number: u8, bytes: &[u8]| {
        let mut out = vec![number << 3 | 2, bytes.len() as u8];
        out.extend_from_slice(bytes);
        out
    };
    let mut buffer = Vec::new();
    for (id, name) in members {
        let mut member = field(1, id.as_bytes());
        if !name.is_empty() {
            member.extend(field(2, name.as_bytes()));
        }
        member.extend([0x18, 0x01]);
        buffer.extend(field(1, &member));
        buffer.extend([0x10, 0x02]);
    }
    buffer
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

impl Account {
    /// The official accounts' stores `biz` fills, by their paths.
    pub fn biz_databases(&self) -> Vec<String> {
        let mut stores: Vec<u8> = self.biz.iter().map(|b| b.store).collect();
        stores.sort();
        stores.dedup();
        stores.iter().map(|n| format!("{BIZ_MESSAGE_PREFIX}{n}.db")).collect()
    }

    /// Writes each official accounts' store `biz` names: its `Name2Id`, a
    /// table of each official account's messages, and a WAL of the ones
    /// only there.
    fn write_biz(&self) {
        for path in self.biz_databases() {
            let db = plain(NAME2ID);
            let store = |b: &&Biz| format!("{BIZ_MESSAGE_PREFIX}{}.db", b.store) == path;
            let sent: Vec<&Biz> = self.biz.iter().filter(store).collect();
            for b in &sent {
                let table = messages_table(b.from);
                db.execute(
                    "INSERT OR IGNORE INTO Name2Id(user_name, is_session) VALUES (?1, 1)",
                    params![b.from],
                )
                .unwrap();
                db.execute_batch(
                    &messages(&table).replace("CREATE TABLE", "CREATE TABLE IF NOT EXISTS"),
                )
                .unwrap();
            }
            let insert = |b: &Biz| {
                let content: Vec<u8> = if b.zstd {
                    ruzstd::encoding::compress_to_vec(
                        b.content.as_bytes(),
                        ruzstd::encoding::CompressionLevel::Fastest,
                    )
                } else {
                    b.content.as_bytes().to_vec()
                };
                let compressed = if b.zstd { Some(ZSTD) } else { None };
                db.execute(
                    &format!("INSERT INTO {}(server_id, local_type, sort_seq, real_sender_id, create_time, status, message_content, WCDB_CT_message_content) VALUES (?1, ?2, ?3, 1, ?4, 3, ?5, ?6)", messages_table(b.from)),
                    params![b.at * 7, b.local_type, b.at * 1000, b.at, content, compressed],
                )
                .unwrap();
            };
            sent.iter().filter(|b| !b.in_wal).for_each(|b| insert(b));
            let before = bytes(&db);
            sent.iter().filter(|b| b.in_wal).for_each(|b| insert(b));
            let after = bytes(&db);
            let changes: &[(&[u8], &[u8], bool)] =
                if before == after { &[] } else { &[(&before, &after, true)] };
            self.seal(&path, &before, changes);
        }
    }
}
