//! Proving which captured key opens which database
//! (docs/adr/0005-keys-from-the-crypto-call.md).
//!
//! WeChat for Mac 4.x keeps no database key resident where a memory scan can
//! read it: the raw 32-byte key appears only for the instant it is handed to
//! CommonCrypto to decrypt a page. The effect crate captures those keys with
//! Frida (one 32-byte candidate per distinct AES-256 key seen) and reads the
//! first page of each database; everything here is a pure function over those
//! bytes:
//!
//! - [`verify`] keeps only the captured keys that open a database — each
//!   proved by [`wxread_sqlcipher::opens`], the same check every reader makes
//!   before a page is decrypted — and answers them as the keys, or the one
//!   line saying none was captured.
//! - The other failures a capture can end in (not root, no WeChat, the tool
//!   missing, Frida refused or failed) each have one line here, carrying the
//!   command that fixes them; the effect crate picks the one that happened.

use wxread_keys::{Key, Keys};
use wxread_sqlcipher::PAGE;

/// How many bytes a database key is.
pub const KEY_LEN: usize = 32;
/// How many bytes of a first page are its salt.
const SALT_LEN: usize = 16;

/// One database to find the key of: its path under `db_storage`, as the keys
/// are answered, and its first page.
#[derive(Debug, Clone)]
pub struct Target {
    pub path: String,
    pub page1: [u8; PAGE],
}

/// Why `keys` cannot answer: the one line it prints, carrying its own fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure(pub String);

/// `keys` was not run as root, and only root may attach Frida to WeChat.
pub const NOT_ROOT: &str = "wxread keys captures the keys from the running WeChat with Frida, which only root may attach to: run `sudo wxread keys`";
/// No WeChat process was found to capture from.
pub const NOT_RUNNING: &str = "WeChat is not running, so there is nothing to capture keys from: open the re-signed WeChat copy and log in (`wxread keys --help` says how), then run `sudo wxread keys`";
/// WeChat was attached and hooked, but it decrypted no page while the capture
/// ran, so no key was seen.
pub const NOTHING_CAPTURED: &str = "WeChat is running but no database key was captured: while `sudo wxread keys` runs, open chats, use search and open files in WeChat so it decrypts its databases; then run `sudo wxread keys` again";

/// The capture tool `name` (uv, which runs Frida) is not installed; `install`
/// is the command that installs it.
pub fn tool_missing(name: &str, install: &str) -> Failure {
    Failure(format!(
        "wxread keys runs Frida through {name}, and {name} is not installed: install it (`{install}`), then run `sudo wxread keys`"
    ))
}

/// Frida could not attach to WeChat, for `cause`: almost always because the
/// running WeChat is a signed build without `get-task-allow`.
pub fn attach_refused(cause: &str) -> Failure {
    Failure(format!(
        "Frida could not attach to WeChat ({cause}): it is a signed build without get-task-allow, so capture from a re-signed copy — clone WeChat.app, `wxread resign --app <copy> --yes`, open the copy and log in, then `sudo wxread keys` (`wxread keys --help` has the steps)"
    ))
}

/// Frida or uv ran but ended in an error that is not a refused attach.
pub fn capture_failed(why: &str) -> Failure {
    Failure(format!(
        "Frida could not capture WeChat's keys ({why}): `wxread keys --help` says what the capture needs, then run `sudo wxread keys` again"
    ))
}

/// The captured keys that open a database, each proved against its first page
/// by [`wxread_sqlcipher::opens`]; or [`NOTHING_CAPTURED`] when none does. A
/// database no captured key opens is simply absent from the answer, never
/// guessed.
pub fn verify(candidates: &[[u8; KEY_LEN]], dbs: &[Target]) -> Result<Keys, Failure> {
    let mut found: Vec<(String, Key)> = Vec::new();
    for db in dbs {
        for candidate in candidates {
            if wxread_sqlcipher::opens(&Key::new(*candidate, None), &db.page1) {
                found.push((db.path.clone(), Key::new(*candidate, Some(salt_of(&db.page1)))));
                break;
            }
        }
    }
    if found.is_empty() {
        return Err(Failure(NOTHING_CAPTURED.to_string()));
    }
    Ok(Keys::of(found))
}

/// The salt a first page starts with: its first 16 bytes.
fn salt_of(page1: &[u8; PAGE]) -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&page1[..SALT_LEN]);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const KEY: [u8; 32] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
        0xee, 0xff,
    ];
    const SALT: [u8; 16] = [
        0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
        0x0c,
    ];

    /// A synthetic first page that `key` and `salt` open, built the way
    /// `wxread_sqlcipher::opens` checks: a correct candidate opens it. No real
    /// database or key.
    fn page(key: &[u8; 32], salt: &[u8; 16], body: &[u8]) -> [u8; PAGE] {
        let mut plain = [0u8; PAGE];
        let take = body.len().min(PAGE);
        plain[..take].copy_from_slice(&body[..take]);
        let db = wxread_sqlcipher::encrypt(&plain, key, salt, |n| [n as u8 ^ 0x5a; 16]);
        let mut first = [0u8; PAGE];
        first.copy_from_slice(&db[..PAGE]);
        first
    }

    #[test]
    fn verify_finds_the_key_of_each_database_among_the_captured() {
        let other = [9u8; 32];
        let dbs = [Target {
            path: "message/message_0.db".to_string(),
            page1: page(&KEY, &SALT, b"chat"),
        }];
        // The capture order is whatever CommonCrypto was called in; the wrong
        // key before the right one changes nothing.
        let keys = verify(&[other, KEY], &dbs).unwrap();
        let value = keys.to_value();
        assert_eq!(value["message/message_0.db"]["enc_key"], wxread_keys::encode(&KEY));
        assert_eq!(value["message/message_0.db"]["salt"], wxread_keys::encode(&SALT));
    }

    #[test]
    fn nothing_captured_when_no_key_opens_a_database() {
        let dbs = [Target { path: "m.db".to_string(), page1: page(&KEY, &SALT, b"chat") }];
        // Captured something, but none of it opens the database.
        let Failure(line) = verify(&[[9u8; 32]], &dbs).unwrap_err();
        assert!(line.contains("no database key was captured"), "{line}");
        // Captured nothing at all is the same answer.
        assert_eq!(verify(&[], &dbs).unwrap_err(), verify(&[[9u8; 32]], &dbs).unwrap_err());
    }

    #[test]
    fn every_failure_line_names_the_command_that_fixes_it() {
        let lines = [
            Failure(NOT_ROOT.to_string()),
            Failure(NOT_RUNNING.to_string()),
            Failure(NOTHING_CAPTURED.to_string()),
            tool_missing("uv", "brew install uv"),
            attach_refused("os error 5"),
            capture_failed("uv exited with code 1"),
        ];
        for Failure(line) in lines {
            assert!(line.contains("wxread") || line.contains("brew install"), "{line}");
        }
    }

    proptest! {
        // Over random keys, salts and bodies, the key that built a page is the
        // one `verify` reports for it, and a different key is not.
        #[test]
        fn verify_reports_only_the_key_that_built_the_page(
            key: [u8; 32], salt: [u8; 16], body in proptest::collection::vec(any::<u8>(), 0..200),
        ) {
            prop_assume!(key != [0u8; 32]);
            let dbs = [Target { path: "m.db".to_string(), page1: page(&key, &salt, &body) }];
            let value = verify(&[key], &dbs).unwrap().to_value();
            prop_assert_eq!(value["m.db"]["enc_key"].as_str().unwrap(), wxread_keys::encode(&key));

            let mut other = key;
            other[(key[0] as usize) % 32] ^= 1;
            prop_assert!(verify(&[other], &dbs).is_err());
        }
    }
}
