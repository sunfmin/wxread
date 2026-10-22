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
    CHAT_ROOM, CHAT_ROOM_INFO_DETAIL, CHATROOM_MEMBER, CONTACT, CONTACT_DB, FAV_DB_ITEM,
    FAVORITE_DB, FTS_NAME2ID, FTS_RANGE, HEAD_IMAGE, HEAD_IMAGE_DB, MEDIA_NAME2ID, MESSAGE_FTS_DB,
    NAME2ID, SESSION, SESSION_DB, SNS_DB, SNS_TIMELINE, TIMESTAMP, VOICE_INFO, ZSTD, fts_content,
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

/// A chat in `SessionTable`: its id, when its last message was (which is
/// where WeChat sorts it, in seconds), and its unread messages.
pub struct Session(pub &'static str, pub i64, pub i64);

/// A message of chat `chat`, in message database `db` (`message_0`): its
/// server id, type, when it was sent, its order in that second, its
/// sender, its content as WeChat keeps it (zstd-compressed when `zstd`
/// says), its `packed_info_data`, and the text WeChat's full-text index
/// holds of it, where a range of the index covers it (None for none).
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
    pub indexed: Option<String>,
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
            indexed: None,
        }
    }

    /// A push of `articles` from official account `from`, named `name` in
    /// it, at `create_time`, in biz_message_0, as WeChat writes one: an app
    /// message of a link, the first article in the appmsg itself and every
    /// article an item of its `mmreader`, sent by the official account.
    pub fn push(from: &'static str, name: &str, create_time: i64, articles: &[Article]) -> Said {
        let items: String = articles
            .iter()
            .map(|Article(title, digest, url, cover)| {
                format!("<item><itemshowtype>0</itemshowtype><title><![CDATA[{title}]]></title><url><![CDATA[{url}]]></url><pub_time>{create_time}</pub_time><cover><![CDATA[{cover}]]></cover><digest><![CDATA[{digest}]]></digest></item>")
            })
            .collect();
        let Article(title, digest, url, cover) = &articles[0];
        let content = format!(
            "<msg><appmsg appid=\"\" sdkver=\"0\"><title><![CDATA[{title}]]></title><des><![CDATA[{digest}]]></des><type>5</type><url><![CDATA[{url}]]></url><thumburl><![CDATA[{cover}]]></thumburl><mmreader><category type=\"20\" count=\"{}\"><name><![CDATA[{name}]]></name>{items}</category><publisher><username><![CDATA[{from}]]></username><nickname><![CDATA[{name}]]></nickname></publisher></mmreader></appmsg><fromusername><![CDATA[{from}]]></fromusername></msg>",
            articles.len()
        );
        Said {
            db: "biz_message_0",
            local_type: ARTICLES,
            content,
            ..Said::text(from, create_time * 7, create_time, from, "")
        }
    }
}

/// A voice message's sound in `message/media_0.db`: its chat, the
/// message's server id, and its SILK bytes.
pub struct Voice(pub &'static str, pub i64, pub Vec<u8>);

/// The media database a synthetic account's voice messages are in.
pub const MEDIA_DB: &str = "message/media_0.db";

/// The avatar WeChat holds of contact `id`: a JPEG's bytes of its own.
pub fn avatar_of(id: &str) -> Vec<u8> {
    let mut image = vec![0xff, 0xd8, 0xff, 0xe0];
    image.extend(id.bytes().cycle().take(300));
    image
}

/// One article of a push: its title, digest, url and cover.
pub struct Article(pub &'static str, pub &'static str, pub &'static str, pub &'static str);

/// The type of an official account's push of articles: an app message
/// (49) of a link (its appmsg type 5).
pub const ARTICLES: i64 = 49 | 5 << 32;

/// A Moment in `sns/sns.db`: its `tid`, its author's id, its XML.
pub struct Posted(pub i64, pub &'static str, pub String);

/// A favorite in `favorite/favorite.db`: its id, type, when it was saved,
/// its `<favitem>`, and the `fromusr` and `realchatname` its row says.
pub struct Saved {
    pub id: i64,
    pub fav_type: i64,
    pub saved: i64,
    pub content: String,
    pub from: &'static str,
    pub sender: &'static str,
}

/// What WeChat's full-text index covers: chat `chat`'s messages `first` to
/// `last` (their `local_id`) in message database `db`.
pub struct Indexed {
    pub chat: &'static str,
    pub db: &'static str,
    pub first: i64,
    pub last: i64,
}

/// An entry of the index whose message is gone: its chat, its `local_id`,
/// its text and when it was sent.
pub struct Stale(pub &'static str, pub i64, pub &'static str, pub i64);

/// One account's directory under a data directory, and what its databases
/// hold: `contacts`, `sessions` and `messages` in the databases
/// themselves, `recent` sessions and `recent_messages` only in committed
/// frames of their database's WAL, and `uncommitted` sessions in frames
/// after them that no commit ended; `profiles` more of some contacts,
/// `avatars` the contacts WeChat holds an avatar of, and `groups` each
/// group's record and notice; `voices` its voice messages' sound, `files`
/// the files under its directory (each by its path there), `kvcomm` the
/// codes of the kvcomm files WeChat wrote beside the data directory;
/// `index` what WeChat's full-text index covers (None for an account it
/// keeps none for), and `stale` its entries whose messages are gone;
/// `favorites` what it saved, `recent_favorites` those only in committed
/// frames of the favorites database's WAL; `moments` the Moments WeChat
/// loaded, the last of them only in a committed frame of its database's
/// WAL.
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
    pub messages: Vec<Said>,
    pub recent_messages: Vec<Said>,
    pub voices: Vec<Voice>,
    pub files: Vec<(String, Vec<u8>)>,
    pub kvcomm: Vec<u32>,
    pub moments: Vec<Posted>,
    pub index: Option<Vec<Indexed>>,
    pub stale: Vec<Stale>,
    pub favorites: Vec<Saved>,
    pub recent_favorites: Vec<Saved>,
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
            messages: Vec::new(),
            recent_messages: Vec::new(),
            voices: Vec::new(),
            files: Vec::new(),
            kvcomm: Vec::new(),
            moments: Vec::new(),
            index: None,
            stale: Vec::new(),
            favorites: Vec::new(),
            recent_favorites: Vec::new(),
        }
    }

    /// The account's directory, above its db_storage.
    pub fn dir(&self) -> PathBuf {
        self.storage.parent().unwrap().to_path_buf()
    }

    /// WeChat's Documents, the data directory's parent.
    pub fn documents(&self) -> PathBuf {
        self.dir().parent().unwrap().parent().unwrap().to_path_buf()
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
        // The note is JSON too: a Windows path's backslashes must be escaped.
        let note = self.storage.display().to_string().replace('\\', "\\\\");
        format!("{{\"_db_dir\": \"{note}\", {}}}", entries.join(", "))
    }

    /// Every database it has: the contact and session databases, each
    /// message database its messages are in, each media database its voice
    /// messages are in, and the full-text index when it has one.
    pub fn databases(&self) -> Vec<String> {
        let mut all: Vec<String> = DATABASES.iter().map(|d| d.to_string()).collect();
        for said in self.messages.iter().chain(&self.recent_messages) {
            let path = format!("message/{}.db", said.db);
            if !all.contains(&path) {
                all.push(path);
            }
        }
        if !self.voices.is_empty() {
            all.push(MEDIA_DB.to_string());
        }
        if !self.moments.is_empty() {
            all.push(SNS_DB.to_string());
        }
        if !self.favorites.is_empty() || !self.recent_favorites.is_empty() {
            all.push(FAVORITE_DB.to_string());
        }
        if self.index.is_some() {
            all.push(MESSAGE_FTS_DB.to_string());
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
                    "INSERT INTO head_image(username, md5, image_buffer, update_time) VALUES (?1, 'md5', ?2, 1760000000)",
                    params![id, avatar_of(id)],
                )
                .unwrap();
        }
        self.seal(HEAD_IMAGE_DB, &bytes(&avatars), &[]);
        let mut dbs: Vec<&str> =
            self.messages.iter().chain(&self.recent_messages).map(|s| s.db).collect();
        dbs.sort();
        dbs.dedup();
        let mut placed = Vec::new();
        for db in dbs {
            let conn = plain(NAME2ID);
            conn.execute_batch(TIMESTAMP).unwrap();
            conn.execute("INSERT INTO TimeStamp(timestamp) VALUES (?1)", [stamp(db)]).unwrap();
            let of = |all: &[Said]| all.iter().filter(|s| s.db == db).cloned().collect::<Vec<_>>();
            placed.extend(said(&conn, &of(&self.messages)));
            let before = bytes(&conn);
            placed.extend(said(&conn, &of(&self.recent_messages)));
            let after = bytes(&conn);
            let path = format!("message/{db}.db");
            self.seal(&path, &before, &[(&before, &after, true)]);
        }
        if !self.voices.is_empty() {
            let media = plain(&format!("{MEDIA_NAME2ID}; {VOICE_INFO};"));
            for Voice(chat, server_id, silk) in &self.voices {
                media
                    .execute("INSERT OR IGNORE INTO Name2Id(user_name) VALUES (?1)", [chat])
                    .unwrap();
                media
                    .execute(
                        "INSERT INTO VoiceInfo(chat_name_id, create_time, local_id, svr_id, voice_data) SELECT rowid, 1760000000, 1, ?2, ?3 FROM Name2Id WHERE user_name = ?1",
                        params![chat, server_id, silk],
                    )
                    .unwrap();
            }
            self.seal(MEDIA_DB, &bytes(&media), &[]);
        }
        if let Some((last, before)) = self.moments.split_last() {
            let sns = plain(SNS_TIMELINE);
            let post = |Posted(tid, author, content): &Posted| {
                sns.execute(
                    "INSERT INTO SnsTimeLine(tid, user_name, content) VALUES (?1, ?2, ?3)",
                    params![tid, author, content],
                )
                .unwrap();
            };
            before.iter().for_each(post);
            let earlier = bytes(&sns);
            post(last);
            self.seal(SNS_DB, &earlier, &[(&earlier, &bytes(&sns), true)]);
        }
        if !self.favorites.is_empty() || !self.recent_favorites.is_empty() {
            let favorites = plain(FAV_DB_ITEM);
            saved(&favorites, &self.favorites);
            let before = bytes(&favorites);
            saved(&favorites, &self.recent_favorites);
            let after = bytes(&favorites);
            self.seal(FAVORITE_DB, &before, &[(&before, &after, true)]);
        }
        for (path, bytes) in &self.files {
            let file = self.dir().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        for code in &self.kvcomm {
            let dir = self.documents().join("app_data/net/kvcomm");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("key_{code}_1.statistic")), b"").unwrap();
        }
        if let Some(index) = &self.index {
            self.seal(MESSAGE_FTS_DB, &bytes(&self.fts(index, &placed)), &[]);
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

    /// WeChat's full-text index of `placed` (each message with its
    /// `local_id`), as WeChat writes one: each chat in its `name2id`, each
    /// range of `index`, and an entry of each message a range covers that
    /// has its indexed text, the entries spread over two of its tables;
    /// then the `stale` entries.
    fn fts(&self, index: &[Indexed], placed: &[(Said, i64)]) -> Connection {
        let fts = plain(FTS_NAME2ID);
        fts.execute_batch(&format!("{FTS_RANGE}; {}; {};", fts_content(0), fts_content(1)))
            .unwrap();
        let session = |chat: &str| -> i64 {
            fts.execute("INSERT OR IGNORE INTO name2id(username) VALUES (?1)", [chat]).unwrap();
            fts.query_row("SELECT rowid FROM name2id WHERE username = ?1", [chat], |r| r.get(0))
                .unwrap()
        };
        for range in index {
            fts.execute(
                "INSERT INTO message_fts_v4_range(db_time_stamp, start_local_id, end_local_id, session_id) VALUES (?1, ?2, ?3, ?4)",
                params![stamp(range.db), range.first, range.last, session(range.chat)],
            )
            .unwrap();
        }
        let mut entries: Vec<(&str, i64, i64, i64, i64, &str)> = Vec::new();
        for (said, local_id) in placed {
            let covered = index.iter().any(|r| {
                r.chat == said.chat && r.db == said.db && (r.first..=r.last).contains(local_id)
            });
            if let (true, Some(text)) = (covered, &said.indexed) {
                let t = said.local_type;
                entries.push((said.chat, *local_id, said.sort_seq, t, said.create_time, text));
            }
        }
        for Stale(chat, local_id, text, at) in &self.stale {
            entries.push((chat, *local_id, at * 1000, 1, *at, text));
        }
        for (i, (chat, local_id, sort_seq, local_type, at, text)) in entries.into_iter().enumerate()
        {
            fts.execute(
                &format!("INSERT INTO message_fts_v4_{}_content(c0, c1, c2, c3, c4, c5, c6) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)", i % 2),
                params![text, local_id, sort_seq, local_type, session(chat), at],
            )
            .unwrap();
        }
        fts
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

/// Writes `favorites` into the favorites database.
fn saved(db: &Connection, favorites: &[Saved]) {
    for f in favorites {
        db.execute(
            "INSERT INTO fav_db_item(local_id, server_id, type, update_time, content, fromusr, realchatname, flag) VALUES (?1, ?1 + 1000, ?2, ?3, ?4, ?5, ?6, 0)",
            params![f.id, f.fav_type, f.saved, f.content, f.from, f.sender],
        )
        .unwrap();
    }
}

/// Writes `messages` into the message database `db`: each chat's table,
/// made the first time one of its messages is written, and each chat and
/// sender in `Name2Id`.
fn said(db: &Connection, messages: &[Said]) -> Vec<(Said, i64)> {
    let mut placed = Vec::new();
    for m in messages {
        let table = wxread_schema::messages_table(m.chat);
        db.execute_batch(
            &wxread_schema::messages(&table).replace("CREATE TABLE", "CREATE TABLE IF NOT EXISTS"),
        )
        .unwrap();
        // WeChat names each chat in Name2Id too, as a session.
        db.execute("INSERT OR IGNORE INTO Name2Id(user_name, is_session) VALUES (?1, 1)", [m.chat])
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
        placed.push((m.clone(), db.last_insert_rowid()));
    }
    placed
}

/// The `TimeStamp` of message database `db`: its number's, so that
/// `message_0` and `biz_message_0` share one, as WeChat's do.
fn stamp(db: &str) -> i64 {
    let n: i64 = db.rsplit('_').next().unwrap().parse().unwrap();
    1_700_000_000 + n * 1000
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
