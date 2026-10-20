//! The keys of an account's databases, in the shape `wxread keys` answers
//! them and `wechat-cli init` writes its `all_keys.json`: one JSON object of
//! each database's path under `db_storage` (`message/message_0.db`) to
//! `{"enc_key": <64 hex digits>, "salt": <32 hex digits>}`. WeChat for Mac
//! 4.x keys each database apart, so a database is opened only by its own
//! entry. Entries whose name starts with `_` are a tool's own notes, and
//! passed over.

use std::collections::BTreeMap;

use flux_rs::attrs::*;
use serde_json::Value;

/// One database's key, and the salt its first page starts with when the
/// keys say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub enc_key: [u8; 32],
    pub salt: Option<[u8; 16]>,
}

/// Every database the keys open, by its path under `db_storage`, `/`
/// between its parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys(BTreeMap<String, Key>);

impl Keys {
    /// The keys in `text`, or every fault in it at once, each saying which
    /// entry and what is wrong with it.
    pub fn parse(text: &str) -> Result<Keys, Vec<String>> {
        let all: BTreeMap<String, Value> = match serde_json::from_str(text) {
            Ok(all) => all,
            Err(e) => {
                return Err(vec![format!(
                    "the keys are not one JSON object of each database's path to its key ({e})"
                )]);
            }
        };
        let mut keys = BTreeMap::new();
        let mut faults = Vec::new();
        for (path, entry) in all.into_iter().filter(|(path, _)| !path.starts_with('_')) {
            match entry_key(&entry) {
                Ok(key) => {
                    keys.insert(path.replace('\\', "/"), key);
                }
                Err(why) => faults.push(format!("the key of {path} {why}")),
            }
        }
        if faults.is_empty() { Ok(Keys(keys)) } else { Err(faults) }
    }

    /// The key of database `path` (`message/message_0.db`), if there is one.
    pub fn get(&self, path: &str) -> Option<&Key> {
        self.0.get(path)
    }

    /// How many databases the keys open.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The key one entry holds, or what is wrong with it.
fn entry_key(entry: &Value) -> Result<Key, &'static str> {
    let Some(entry) = entry.as_object() else {
        return Err("is not an object {\"enc_key\", \"salt\"}");
    };
    let enc_key = match entry.get("enc_key").and_then(Value::as_str) {
        None => return Err("has no enc_key"),
        Some(hex) => key32(hex).ok_or("is not 64 hex digits")?,
    };
    let salt = match entry.get("salt") {
        None | Some(Value::Null) => None,
        Some(salt) => {
            Some(salt.as_str().and_then(salt16).ok_or("has a salt of other than 32 hex digits")?)
        }
    };
    Ok(Key { enc_key, salt })
}

/// A database key written as 64 hex digits.
fn key32(hex: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    unhex(hex, &mut out).then_some(out)
}

/// A salt written as 32 hex digits.
fn salt16(hex: &str) -> Option<[u8; 16]> {
    let mut out = [0u8; 16];
    unhex(hex, &mut out).then_some(out)
}

/// Fills `out` from `hex`, two digits a byte, and says whether `hex` was
/// exactly that: as many digit pairs as `out` has bytes, and nothing else.
fn unhex(hex: &str, out: &mut [u8]) -> bool {
    let digits = hex.as_bytes();
    if !digits.len().is_multiple_of(2) || digits.len() / 2 != out.len() {
        return false;
    }
    for (&[hi, lo], byte) in digits.as_chunks::<2>().0.iter().zip(out.iter_mut()) {
        match (digit(hi), digit(lo)) {
            (Some(hi), Some(lo)) => *byte = hi * 16 + lo,
            _ => return false,
        }
    }
    true
}

/// The value of one hex digit, either case: below sixteen, so two of them
/// make a byte.
#[spec(fn(u8) -> Option<u8{v: v < 16}>)]
fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899AABBCCDDEEFF";

    #[test]
    fn the_keys_are_read_in_all_keys_jsons_shape() {
        let text = format!(
            r#"{{"message\\message_0.db": {{"enc_key": "{KEY}", "salt": "000102030405060708090a0b0c0d0e0f"}},
                "contact/contact.db": {{"enc_key": "{KEY}"}},
                "_db_dir": "/somewhere"}}"#
        );
        let keys = Keys::parse(&text).unwrap();
        assert_eq!(keys.len(), 2);
        let key = keys.get("message/message_0.db").unwrap();
        assert_eq!(key.enc_key[..4], [0x00, 0x11, 0x22, 0x33]);
        assert_eq!(key.enc_key[31], 0xff);
        assert_eq!(key.salt.unwrap()[15], 0x0f);
        assert_eq!(keys.get("contact/contact.db").unwrap().salt, None);
        assert_eq!(keys.get("_db_dir"), None);
    }

    #[test]
    fn every_fault_is_named_at_once() {
        let text = format!(
            r#"{{"a.db": {{"enc_key": "abc"}}, "b.db": {{}}, "c.db": "x",
                "d.db": {{"enc_key": "{KEY}", "salt": "zz"}}, "e.db": {{"enc_key": "{KEY}"}}}}"#
        );
        let faults = Keys::parse(&text).unwrap_err();
        assert_eq!(
            faults,
            [
                "the key of a.db is not 64 hex digits",
                "the key of b.db has no enc_key",
                "the key of c.db is not an object {\"enc_key\", \"salt\"}",
                "the key of d.db has a salt of other than 32 hex digits",
            ]
        );
    }

    #[test]
    fn text_that_is_not_one_object_is_one_fault() {
        for text in ["", "[]", "{\"a\": 1", "not json"] {
            let faults = Keys::parse(text).unwrap_err();
            assert_eq!(faults.len(), 1, "{text}");
            assert!(faults[0].starts_with("the keys are not one JSON object"), "{text}");
        }
    }

    proptest! {
        #[test]
        fn a_key_written_in_hex_reads_back(key: [u8; 32], upper: bool) {
            let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
            let hex = if upper { hex.to_uppercase() } else { hex };
            prop_assert_eq!(key32(&hex), Some(key));
        }

        #[test]
        fn anything_but_64_hex_digits_is_no_key(text in "\\PC{0,70}") {
            let is_key = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
            prop_assert_eq!(key32(&text).is_some(), is_key);
        }
    }
}
