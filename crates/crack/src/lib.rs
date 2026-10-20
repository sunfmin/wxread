//! Finding each database's key in the running WeChat's memory, and proving
//! it opens the database before it is reported (docs/adr/0003-keys-from-memory.md).
//!
//! WeChat for Mac 4.x keeps every database's key in memory as the raw
//! 32-byte AES key, written as the ASCII SQLCipher literal `x'<64 hex>...'`.
//! The effect crate reads the running WeChat's memory and the first page of
//! each database; everything here is a pure function over those bytes:
//!
//! - [`candidates`] reads every 32-byte key written in a block of memory.
//! - [`opens`] checks one candidate against a database's first-page HMAC,
//!   the way SQLCipher 4 computes it (AES-256-CBC pages, a reserve of an IV
//!   and an HMAC-SHA512 at the end of each page).
//! - [`run`] turns what the effect crate gathered — whether the memory could
//!   be read at all, the candidates, and the databases — into either the keys
//!   that open the databases or the one line that says why it could not.

use hmac::{Hmac, Mac};
use sha2::Sha512;
use wxread_keys::{Key, Keys};

/// A SQLCipher page is 4096 bytes; each database key is 32 bytes and each
/// salt 16.
pub const PAGE: usize = 4096;
pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
/// Each page ends with a reserve of a 16-byte IV and a 64-byte HMAC-SHA512.
/// The first page's HMAC is over its bytes `[SALT_LEN, HMAC_END)` and the
/// page number, and is stored in `[HMAC_END, PAGE)`.
const RESERVE: usize = 80;
const HMAC_END: usize = PAGE - RESERVE + SALT_LEN;

/// Every distinct 32-byte key written as hex in `memory`: the 64 hex digits
/// at the start of each run of 64 or more hex digits (WeChat holds the key as
/// `x'<64 hex key><32 hex salt>'`, so its key is the run's first 64 digits).
pub fn candidates(memory: &[u8]) -> Vec<[u8; KEY_LEN]> {
    // Every maximal run of hex digits (anything else, the `x'` and `'` around
    // WeChat's literal among it, separates them); each run of 64 or more
    // digits yields the key its first 64 digits spell.
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut run_start = 0usize;
    let mut in_run = false;
    while let Some(&byte) = memory.get(i) {
        if byte.is_ascii_hexdigit() {
            if !in_run {
                in_run = true;
                run_start = i;
            }
        } else if in_run {
            in_run = false;
            take_key(memory, run_start, &mut out);
        }
        match i.checked_add(1) {
            Some(next) => i = next,
            None => break,
        }
    }
    if in_run {
        take_key(memory, run_start, &mut out);
    }
    out
}

/// Reads the key the 64 hex digits at `start` spell into `out`, if there are
/// 64 digits there to read.
fn take_key(memory: &[u8], start: usize, out: &mut Vec<[u8; KEY_LEN]>) {
    if let Some(end) = start.checked_add(wxread_keys::KEY_HEX)
        && let Some(head) = memory.get(start..end)
        && let Some(key) = wxread_keys::decode_key(head)
    {
        out.push(key);
    }
}

/// The salt of a database from its first page: its first 16 bytes, which
/// SQLCipher keeps in the clear. `None` for a page shorter than that.
pub fn salt_of(page1: &[u8]) -> Option<[u8; SALT_LEN]> {
    if page1.len() < SALT_LEN {
        return None;
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&page1[..SALT_LEN]);
    Some(salt)
}

/// Whether `key` opens the database whose first page is `page1`: the key the
/// SQLCipher 4 way, its first-page HMAC-SHA512 is recomputed and must equal
/// the one stored in the page. A page shorter than one SQLCipher page opens
/// to nothing.
pub fn opens(page1: &[u8], key: &[u8; KEY_LEN]) -> bool {
    if page1.len() < PAGE {
        return false;
    }
    // The HMAC key is PBKDF2-HMAC-SHA512 of the raw key over the salt with
    // every byte flipped by 0x3a, two rounds, 32 bytes.
    let mut mac_salt = [0u8; SALT_LEN];
    mac_salt.copy_from_slice(&page1[..SALT_LEN]);
    for b in mac_salt.iter_mut() {
        *b ^= 0x3a;
    }
    let mut mac_key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<Sha512>(key, &mac_salt, 2, &mut mac_key);

    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key)
        .expect("HMAC-SHA512 takes a key of any length");
    mac.update(&page1[SALT_LEN..HMAC_END]);
    mac.update(&1u32.to_le_bytes());
    mac.finalize().into_bytes().as_slice() == &page1[HMAC_END..PAGE]
}

/// One database to find the key of: its path under `db_storage`, as the keys
/// are answered, and its first page.
#[derive(Debug, Clone)]
pub struct Target {
    pub path: String,
    pub page1: Vec<u8>,
}

/// Whether the running WeChat's memory could be read at all.
#[derive(Debug, Clone)]
pub enum Access {
    /// wxread was not run as root.
    NotRoot,
    /// No running WeChat was found.
    NotRunning,
    /// macOS refused reading WeChat's memory; the string is the low cause.
    Refused(String),
    /// Its memory was read.
    Read,
}

/// Why `keys` cannot answer: the one line it prints, carrying its own fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure(pub String);

/// What the effect crate gathered for `keys` to decide on: whether the memory
/// could be read, the key candidates read out of it, and the databases found.
pub struct Request<'a> {
    pub access: Access,
    pub candidates: &'a [[u8; KEY_LEN]],
    pub dbs: &'a [Target],
}

/// `keys` was not run as root, and only root reads another process's memory.
pub const NOT_ROOT: &str = "wxread keys reads the keys out of the running WeChat's memory, which only root can read: run `sudo wxread keys`";
/// No WeChat process was found.
pub const NOT_RUNNING: &str = "WeChat is not running, so it holds no keys to read: open WeChat for Mac 4.x and log in, then run `sudo wxread keys`";
/// WeChat is running and its memory was read, but no key in it opens any
/// database: no account is logged in, or a signed WeChat was read in place of
/// the re-signed copy.
pub const NOT_LOGGED_IN: &str = "WeChat is running but no key in its memory opens a database on this Mac: log in to WeChat for Mac 4.x (and if you re-signed a copy of WeChat, quit WeChat and open that copy), then run `sudo wxread keys` again";

/// macOS refused reading WeChat's memory, for `cause`: WeChat is a signed
/// build without `get-task-allow`, and must be re-signed to be read.
pub fn refused(cause: &str) -> Failure {
    Failure(format!(
        "macOS will not let wxread read WeChat's memory ({cause}): WeChat is a signed build without get-task-allow. `wxread keys --help` says how to re-sign a copy of WeChat and grant Full Disk Access so its memory can be read; then quit WeChat, open the re-signed copy, and run `sudo wxread keys` again"
    ))
}

/// The keys that open the databases, or the one line saying why there are
/// none: the memory could not be read, or no key in it opened a database.
pub fn run(request: &Request) -> Result<Keys, Failure> {
    match &request.access {
        Access::NotRoot => return Err(Failure(NOT_ROOT.to_string())),
        Access::NotRunning => return Err(Failure(NOT_RUNNING.to_string())),
        Access::Refused(cause) => return Err(refused(cause)),
        Access::Read => {}
    }
    let mut found: Vec<(String, Key)> = Vec::new();
    for db in request.dbs {
        for candidate in request.candidates {
            if opens(&db.page1, candidate) {
                found.push((db.path.clone(), Key::new(*candidate, salt_of(&db.page1))));
                break;
            }
        }
    }
    if found.is_empty() {
        return Err(Failure(NOT_LOGGED_IN.to_string()));
    }
    Ok(Keys::of(found))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use proptest::prelude::*;

    type Enc = cbc::Encryptor<aes::Aes256>;

    /// A synthetic first page that `key` and `salt` open: the SQLCipher 4
    /// layout built the other way round, so a correct [`opens`] accepts it.
    /// No real database or key: `plaintext` is whatever the test gives.
    fn encrypt_page1(key: &[u8; 32], salt: &[u8; 16], plaintext: &[u8], iv: &[u8; 16]) -> Vec<u8> {
        let mut page = vec![0u8; PAGE];
        page[..SALT_LEN].copy_from_slice(salt);
        // The clear-text of page 1 is everything between the salt and the
        // reserve; pad the test's bytes to it.
        let body = PAGE - RESERVE - SALT_LEN;
        let mut buf = vec![0u8; body];
        let take = plaintext.len().min(body);
        buf[..take].copy_from_slice(&plaintext[..take]);
        let ct = Enc::new(key.into(), iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, body)
            .unwrap();
        page[SALT_LEN..PAGE - RESERVE].copy_from_slice(ct);
        page[PAGE - RESERVE..PAGE - RESERVE + 16].copy_from_slice(iv);
        // The HMAC over [salt, HMAC_END) and the page number, as opens checks.
        let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
        let mut mac_key = [0u8; KEY_LEN];
        pbkdf2::pbkdf2_hmac::<Sha512>(key, &mac_salt, 2, &mut mac_key);
        let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key).unwrap();
        mac.update(&page[SALT_LEN..HMAC_END]);
        mac.update(&1u32.to_le_bytes());
        page[HMAC_END..PAGE].copy_from_slice(&mac.finalize().into_bytes());
        page
    }

    const KEY: [u8; 32] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
        0xee, 0xff,
    ];
    const SALT: [u8; 16] = [
        0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
        0x0c,
    ];
    const IV: [u8; 16] = [7u8; 16];

    #[test]
    fn a_key_opens_the_page_built_for_it_and_no_other_key_does() {
        let page = encrypt_page1(&KEY, &SALT, b"a synthetic page", &IV);
        assert!(opens(&page, &KEY));
        assert_eq!(salt_of(&page), Some(SALT));
        let mut wrong = KEY;
        wrong[0] ^= 1;
        assert!(!opens(&page, &wrong));
        // A tampered page no longer verifies.
        let mut tampered = page.clone();
        tampered[100] ^= 1;
        assert!(!opens(&tampered, &KEY));
    }

    #[test]
    fn a_page_shorter_than_one_sqlcipher_page_opens_to_nothing() {
        let page = encrypt_page1(&KEY, &SALT, b"x", &IV);
        assert!(!opens(&page[..PAGE - 1], &KEY));
        assert_eq!(salt_of(&[1, 2, 3]), None);
    }

    #[test]
    fn candidates_reads_the_key_out_of_the_hex_literal_in_memory() {
        let key_hex = wxread_keys::encode(&KEY);
        let salt_hex = wxread_keys::encode(&SALT);
        // The key as WeChat holds it, amid other bytes and other hex runs.
        let memory = format!(
            "....PRAGMA key = \"x'{key_hex}{salt_hex}'\";....{}....",
            "zz 0123 deadbeef", // runs of fewer than 64 hex digits are ignored
        );
        let found = candidates(memory.as_bytes());
        assert!(found.contains(&KEY), "{found:?}");
        // A bare 64-hex run is a candidate too.
        assert!(candidates(key_hex.as_bytes()).contains(&KEY));
        // Nothing of value in bytes with no long hex run.
        assert!(candidates(b"no keys here, just prose and 0xbadf00d").is_empty());
    }

    #[test]
    fn run_finds_the_key_of_each_database_among_the_candidates() {
        let other = [9u8; 32];
        let page = encrypt_page1(&KEY, &SALT, b"chat", &IV);
        let dbs = [Target { path: "message/message_0.db".to_string(), page1: page.clone() }];
        let request = Request { access: Access::Read, candidates: &[other, KEY], dbs: &dbs };
        let keys = run(&request).unwrap();
        let value = keys.to_value();
        assert_eq!(value["message/message_0.db"]["enc_key"], wxread_keys::encode(&KEY));
        assert_eq!(value["message/message_0.db"]["salt"], wxread_keys::encode(&SALT));
    }

    #[test]
    fn each_way_of_reading_nothing_is_its_own_line_with_the_fix() {
        let page = encrypt_page1(&KEY, &SALT, b"chat", &IV);
        let dbs = [Target { path: "m.db".to_string(), page1: page }];
        let no_dbs: [Target; 0] = [];
        let cases = [
            (Access::NotRoot, &no_dbs[..], &[][..], "sudo wxread keys"),
            (Access::NotRunning, &no_dbs[..], &[][..], "not running"),
            (Access::Refused("os error 5".into()), &no_dbs[..], &[][..], "get-task-allow"),
            // Read, but no candidate opens the one database: not logged in.
            (Access::Read, &dbs[..], &[[9u8; 32]][..], "no key in its memory opens"),
        ];
        for (access, dbs, candidates, needle) in cases {
            let request = Request { access, candidates, dbs };
            let Failure(line) = run(&request).unwrap_err();
            assert!(line.contains(needle), "{line}");
            assert!(line.contains("wxread keys"), "every fix names the command: {line}");
        }
    }

    proptest! {
        // Over random keys, salts and bodies, the key that built a page opens
        // it and a different key does not.
        #[test]
        fn opens_accepts_only_the_key_that_built_the_page(
            key: [u8; 32], salt: [u8; 16], iv: [u8; 16], body in proptest::collection::vec(any::<u8>(), 0..200),
        ) {
            let page = encrypt_page1(&key, &salt, &body, &iv);
            prop_assert!(opens(&page, &key));
            let mut other = key;
            other[key[0] as usize % 32] ^= 1;
            prop_assert!(!opens(&page, &other));
        }
    }
}
