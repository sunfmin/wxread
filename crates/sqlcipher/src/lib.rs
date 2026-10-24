//! WeChat 4.x's databases: SQLCipher 4, each under a key of its own
//! (docs/adr/0003-databases-are-read-in-memory.md).
//!
//! A page is 4096 bytes: AES-256-CBC over all but its last 80, which hold
//! its IV (16) and an HMAC-SHA512 (64) of the encrypted bytes, the IV and
//! the page's number (little-endian), under a key PBKDF2-HMAC-SHA512
//! derives from the database's key and its salt XOR 0x3a, two rounds. The
//! first page's first 16 bytes are the salt, where SQLite's header was.
//! Recent writes are in the `-wal` beside the database: SQLite's WAL, each
//! frame's page encrypted alike and its checksums taken over the encrypted
//! bytes. [`decrypt`] makes of the two the plain database SQLite would
//! read, for the effect crate to open from memory; nothing here touches a
//! file.

use aes::cipher::{
    BlockDecryptMut as _, BlockEncryptMut as _, KeyIvInit as _, block_padding::NoPadding,
};
use hmac::{Hmac, Mac as _};
use sha2::Sha512;
use wxread_keys::Key;

/// A page's size, and the bytes at its end that hold its IV and HMAC.
pub const PAGE: usize = 4096;
pub const RESERVE: usize = 80;
const SALT: usize = 16;
const IV: usize = 16;
const HMAC: usize = 64;
/// Where a page's encrypted bytes end and its IV starts.
const BODY_END: usize = PAGE - RESERVE;

/// A WAL's header, and the header of each of its frames.
const WAL_HEADER: usize = 32;
const FRAME_HEADER: usize = 24;
const FRAME: usize = FRAME_HEADER + PAGE;
/// The magic a WAL starts with: its checksums of little-endian words, or
/// with the low bit set, of big-endian ones.
const WAL_MAGIC: u32 = 0x377f_0682;
const WAL_VERSION: u32 = 3_007_000;

/// Why a database cannot be decrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// It is shorter than one page: not a database WeChat wrote.
    Short,
    /// The key does not open it: its first page's HMAC, or the salt the
    /// keys say, is not the database's.
    KeyDoesNotOpen,
}

/// Whether `key` opens the database whose first page is `page`: the salt
/// the keys say, when they say one, is the page's, and the page's HMAC is
/// the one `key` makes. Checked before any byte is decrypted.
pub fn opens(key: &Key, page: &[u8; PAGE]) -> bool {
    let (salt, _) = split16(page);
    if key.salt.is_some_and(|s| s != *salt) {
        return false;
    }
    let mac = page_mac(&mac_key(&key.enc_key, salt), page, SALT, 1);
    let (_, stored) = page.split_at(PAGE - HMAC);
    mac.as_slice() == stored
}

/// The plain SQLite database that the database `db` and its WAL `wal`
/// (empty when it has none) hold under `key`: every page decrypted, the
/// frames of the WAL's last commit and those before it laid over them, in
/// rollback mode, since a database opened from memory has no WAL.
pub fn decrypt(key: &Key, db: &[u8], wal: &[u8]) -> Result<Vec<u8>, Refusal> {
    let (pages, _) = db.as_chunks::<PAGE>();
    let Some(first) = pages.first() else {
        return Err(Refusal::Short);
    };
    if !opens(key, first) {
        return Err(Refusal::KeyDoesNotOpen);
    }
    let mut out = Vec::with_capacity(db.len());
    let mut number: u32 = 1;
    for page in pages {
        out.extend_from_slice(&decrypt_page(&key.enc_key, page, number));
        number = number.saturating_add(1);
    }
    let frames = committed(wal);
    for frame in &frames {
        lay(&mut out, frame.number, &decrypt_page(&key.enc_key, frame.page, frame.number));
    }
    if let Some(last) = frames.last() {
        out.truncate(pages_len(last.size));
        put32(&mut out, 28, last.size);
    }
    // Rollback, not WAL: what reads it from memory has no WAL beside it.
    set_byte(&mut out, 18, 1);
    set_byte(&mut out, 19, 1);
    Ok(out)
}

/// One frame of a WAL: the page it holds, its number, and for the frame
/// that ends a commit the database's size in pages after it (0 otherwise).
struct Frame<'a> {
    number: u32,
    size: u32,
    page: &'a [u8; PAGE],
}

/// The frames of `wal` SQLite would read: those of its current generation
/// (the header's salts) whose checksums hold, up to and including the last
/// that ends a commit. A WAL whose header is not one has none.
fn committed(wal: &[u8]) -> Vec<Frame<'_>> {
    let Some((header, rest)) = wal.split_first_chunk::<WAL_HEADER>() else {
        return Vec::new();
    };
    let magic = word(header, 0);
    if magic & !1 != WAL_MAGIC || word(header, 8) != PAGE as u32 {
        return Vec::new();
    }
    let big = magic & 1 == 1;
    let (head, _) = header.split_at(24);
    let mut sums = checksum(big, head, (0, 0));
    if sums != (word(header, 24), word(header, 28)) {
        return Vec::new();
    }
    let salts = (word(header, 16), word(header, 20));
    let mut frames = Vec::new();
    let mut through = 0;
    for frame in rest.as_chunks::<FRAME>().0 {
        let (head, page) = split_frame(frame);
        if (word(head, 8), word(head, 12)) != salts {
            break;
        }
        let (numbered, _) = head.split_at(8);
        sums = checksum(big, page, checksum(big, numbered, sums));
        if sums != (word(head, 16), word(head, 20)) || word(head, 0) == 0 {
            break;
        }
        frames.push(Frame { number: word(head, 0), size: word(head, 4), page });
        if word(head, 4) != 0 {
            through = frames.len();
        }
    }
    frames.truncate(through);
    frames
}

/// SQLite's WAL checksum of `data`, a whole number of 8-byte words, from
/// `sums`: its words read big- or little-endian as the magic says.
fn checksum(big: bool, data: &[u8], sums: (u32, u32)) -> (u32, u32) {
    let (mut s0, mut s1) = sums;
    for word in data.as_chunks::<8>().0 {
        let (a, b) = split4(word);
        let (x0, x1) = if big {
            (u32::from_be_bytes(*a), u32::from_be_bytes(*b))
        } else {
            (u32::from_le_bytes(*a), u32::from_le_bytes(*b))
        };
        s0 = s0.wrapping_add(x0).wrapping_add(s1);
        s1 = s1.wrapping_add(x1).wrapping_add(s0);
    }
    (s0, s1)
}

/// Page `number` of `out` replaced by `page`, `out` grown to hold it.
fn lay(out: &mut Vec<u8>, number: u32, page: &[u8; PAGE]) {
    let at = pages_len(number.saturating_sub(1));
    let end = at.saturating_add(PAGE);
    if out.len() < end {
        out.resize(end, 0);
    }
    for (i, &b) in page.iter().enumerate() {
        if let Some(slot) = out.get_mut(at.saturating_add(i)) {
            *slot = b;
        }
    }
}

/// How many bytes `pages` pages take.
fn pages_len(pages: u32) -> usize {
    (pages as usize).saturating_mul(PAGE)
}

/// The key a page's HMAC is made under, from the database's key and salt.
fn mac_key(key: &[u8; 32], salt: &[u8; SALT]) -> [u8; 32] {
    let mut mac_salt = *salt;
    for b in mac_salt.iter_mut() {
        *b ^= 0x3a;
    }
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha512>(key, &mac_salt, 2, &mut out);
    out
}

/// The HMAC of page `number`: of its bytes from `from` (past the salt, on
/// the first page) through its IV, and its number.
fn page_mac(mac_key: &[u8; 32], page: &[u8; PAGE], from: usize, number: u32) -> [u8; HMAC] {
    let mut mac = <Hmac<Sha512> as hmac::Mac>::new_from_slice(mac_key)
        .unwrap_or_else(|_| unreachable!("HMAC takes a key of any length"));
    mac.update(page.get(from..BODY_END + IV).unwrap_or_default());
    mac.update(&number.to_le_bytes());
    mac.finalize().into_bytes().into()
}

/// Page `number` decrypted: SQLite's own bytes, the first page's header
/// restored, the reserved bytes zeroed.
fn decrypt_page(key: &[u8; 32], page: &[u8; PAGE], number: u32) -> [u8; PAGE] {
    let from = if number == 1 { SALT } else { 0 };
    let mut out = [0u8; PAGE];
    let (body, rest) = out.split_at_mut(BODY_END);
    body.copy_from_slice(page.get(..BODY_END).unwrap_or_default());
    let (iv, _) = split16(rest_of(page));
    let _ = cbc::Decryptor::<aes::Aes256>::new(key.into(), iv.into())
        .decrypt_padded_mut::<NoPadding>(body.get_mut(from..).unwrap_or_default());
    rest.fill(0);
    if number == 1 {
        for (slot, &b) in out.iter_mut().zip(b"SQLite format 3\0") {
            *slot = b;
        }
    }
    out
}

/// `plain`, a SQLite database of 4096-byte pages that reserve 80 bytes
/// each, encrypted as WeChat encrypts one under `key` and `salt`, `iv`
/// making each page's IV from its number: what synthetic databases are made
/// with. A last part of a page is left out.
pub fn encrypt(
    plain: &[u8],
    key: &[u8; 32],
    salt: &[u8; SALT],
    iv: impl Fn(u32) -> [u8; IV],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(plain.len());
    let mut number: u32 = 1;
    for page in plain.as_chunks::<PAGE>().0 {
        out.extend_from_slice(&encrypt_page(page, key, salt, number, &iv(number)));
        number = number.saturating_add(1);
    }
    out
}

/// Page `number` of `plain` as `encrypt` makes it.
fn encrypt_page(
    page: &[u8; PAGE],
    key: &[u8; 32],
    salt: &[u8; SALT],
    number: u32,
    iv: &[u8; IV],
) -> [u8; PAGE] {
    let from = if number == 1 { SALT } else { 0 };
    let mut out = *page;
    {
        let (body, rest) = out.split_at_mut(BODY_END);
        let _ = cbc::Encryptor::<aes::Aes256>::new(key.into(), iv.into())
            .encrypt_padded_mut::<NoPadding>(
                body.get_mut(from..).unwrap_or_default(),
                BODY_END - from,
            );
        rest.fill(0);
        for (slot, &b) in rest.iter_mut().zip(iv) {
            *slot = b;
        }
    }
    if number == 1 {
        for (slot, &b) in out.iter_mut().zip(salt) {
            *slot = b;
        }
    }
    let mac = page_mac(&mac_key(key, salt), &out, from, number);
    let (_, stored) = out.split_at_mut(PAGE - HMAC);
    stored.copy_from_slice(&mac);
    out
}

/// A WAL of `frames`, each a page already encrypted, its number, and the
/// database's size in pages for one that ends a commit (0 otherwise), its
/// checksums and salts as SQLite writes them, `salts` its generation: what
/// synthetic WALs are made with.
pub fn wal(frames: &[(u32, u32, &[u8; PAGE])], salts: (u32, u32)) -> Vec<u8> {
    let mut header = [0u8; WAL_HEADER];
    for (at, word) in
        [(0, WAL_MAGIC | 1), (4, WAL_VERSION), (8, PAGE as u32), (16, salts.0), (20, salts.1)]
    {
        put32(&mut header, at, word);
    }
    let (head, _) = header.split_at(24);
    let mut sums = checksum(true, head, (0, 0));
    put32(&mut header, 24, sums.0);
    put32(&mut header, 28, sums.1);
    let mut out = header.to_vec();
    for &(number, size, page) in frames {
        let mut head = [0u8; FRAME_HEADER];
        for (at, word) in [(0, number), (4, size), (8, salts.0), (12, salts.1)] {
            put32(&mut head, at, word);
        }
        let (numbered, _) = head.split_at(8);
        sums = checksum(true, page, checksum(true, numbered, sums));
        put32(&mut head, 16, sums.0);
        put32(&mut head, 20, sums.1);
        out.extend_from_slice(&head);
        out.extend_from_slice(page);
    }
    out
}

/// The big-endian word at `at` of `bytes`, or 0 past their end.
fn word(bytes: &[u8], at: usize) -> u32 {
    let mut four = [0u8; 4];
    for (i, slot) in four.iter_mut().enumerate() {
        *slot = bytes.get(at.saturating_add(i)).copied().unwrap_or(0);
    }
    u32::from_be_bytes(four)
}

/// Writes `word` big-endian at `at` of `bytes`, as far as they reach.
fn put32(bytes: &mut [u8], at: usize, word: u32) {
    for (i, b) in word.to_be_bytes().into_iter().enumerate() {
        if let Some(slot) = bytes.get_mut(at.saturating_add(i)) {
            *slot = b;
        }
    }
}

fn set_byte(bytes: &mut [u8], at: usize, b: u8) {
    if let Some(slot) = bytes.get_mut(at) {
        *slot = b;
    }
}

fn split16(page: &[u8]) -> (&[u8; 16], &[u8]) {
    page.split_first_chunk::<16>().unwrap_or((&[0; 16], &[]))
}

fn split4(word: &[u8; 8]) -> (&[u8; 4], &[u8; 4]) {
    let (a, b) = word.split_first_chunk::<4>().unwrap_or((&[0; 4], &[]));
    (a, b.first_chunk::<4>().unwrap_or(&[0; 4]))
}

fn split_frame(frame: &[u8; FRAME]) -> (&[u8; FRAME_HEADER], &[u8; PAGE]) {
    let (head, page) =
        frame.split_first_chunk::<FRAME_HEADER>().unwrap_or((&[0; FRAME_HEADER], &[]));
    (head, page.first_chunk::<PAGE>().unwrap_or(&[0; PAGE]))
}

/// A page's bytes from its IV on.
fn rest_of(page: &[u8; PAGE]) -> &[u8] {
    page.get(BODY_END..).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const KEY: [u8; 32] = [3; 32];
    const SALT_OF: [u8; 16] = [9; 16];

    fn key() -> Key {
        Key { enc_key: KEY, salt: Some(SALT_OF) }
    }

    /// A plain database's pages: page 1 with SQLite's header in WAL mode,
    /// then `n - 1` more, each filled with `fill` and its number.
    fn plain(n: u8, fill: u8) -> Vec<u8> {
        let mut db = Vec::new();
        for p in 1..=n {
            let mut page = vec![fill ^ p; PAGE];
            if p == 1 {
                page[..16].copy_from_slice(b"SQLite format 3\0");
                page[18] = 2;
                page[19] = 2;
                page[28..32].copy_from_slice(&u32::from(n).to_be_bytes());
            }
            page[BODY_END..].fill(0);
            db.extend(page);
        }
        db
    }

    fn sealed(plain: &[u8]) -> Vec<u8> {
        encrypt(plain, &KEY, &SALT_OF, |n| [n as u8; 16])
    }

    fn page(bytes: &[u8], number: usize) -> &[u8; PAGE] {
        bytes[(number - 1) * PAGE..number * PAGE].try_into().unwrap()
    }

    /// `plain` in rollback mode, as `decrypt` answers it.
    fn rollback(mut plain: Vec<u8>) -> Vec<u8> {
        plain[18] = 1;
        plain[19] = 1;
        plain
    }

    #[test]
    fn a_database_decrypts_with_its_own_key() {
        let db = sealed(&plain(3, 0));
        assert_ne!(db[..16], plain(3, 0)[..16], "the salt is where the header was");
        assert_eq!(decrypt(&key(), &db, &[]), Ok(rollback(plain(3, 0))));
    }

    #[test]
    fn another_key_or_salt_does_not_open_it() {
        let db = sealed(&plain(2, 0));
        assert!(opens(&key(), page(&db, 1)));
        let other = Key { enc_key: [4; 32], salt: None };
        assert_eq!(decrypt(&other, &db, &[]), Err(Refusal::KeyDoesNotOpen));
        let salted = Key { enc_key: KEY, salt: Some([8; 16]) };
        assert_eq!(decrypt(&salted, &db, &[]), Err(Refusal::KeyDoesNotOpen));
        assert_eq!(decrypt(&Key { enc_key: KEY, salt: None }, &db, &[]), Ok(rollback(plain(2, 0))));
    }

    #[test]
    fn less_than_a_page_is_no_database() {
        assert_eq!(decrypt(&key(), &[0; PAGE - 1], &[]), Err(Refusal::Short));
    }

    #[test]
    fn committed_frames_are_laid_over_it_and_the_rest_left_out() {
        let before = plain(2, 0);
        let after = plain(3, 0x55);
        let new = sealed(&after);
        let stray = sealed(&plain(3, 0xaa));
        // Pages 1 and 2 rewritten and page 3 added in one commit, then page
        // 2 again in a commit that never ended.
        let wal = wal(
            &[
                (2, 0, page(&new, 2)),
                (3, 0, page(&new, 3)),
                (1, 3, page(&new, 1)),
                (2, 0, page(&stray, 2)),
            ],
            (7, 8),
        );
        assert_eq!(decrypt(&key(), &sealed(&before), &wal), Ok(rollback(after)));
    }

    #[test]
    fn a_frame_of_another_generation_or_a_bad_checksum_ends_the_wal() {
        let before = plain(2, 0);
        let new = sealed(&plain(2, 0x55));
        let mut changed = before.clone();
        changed[PAGE..2 * PAGE].copy_from_slice(&plain(2, 0x55)[PAGE..]);
        let first = wal(&[(2, 2, page(&new, 2))], (1, 1));
        assert_eq!(decrypt(&key(), &sealed(&before), &first), Ok(rollback(changed.clone())));
        let mut later = first.clone();
        later.extend_from_slice(&wal(&[(1, 2, page(&new, 1))], (2, 2))[WAL_HEADER..]);
        assert_eq!(decrypt(&key(), &sealed(&before), &later), Ok(rollback(changed.clone())));
        let mut broken = first.clone();
        broken[WAL_HEADER + FRAME_HEADER + 100] ^= 1;
        assert_eq!(decrypt(&key(), &sealed(&before), &broken), Ok(rollback(before.clone())));
        assert_eq!(decrypt(&key(), &sealed(&before), &first[..40]), Ok(rollback(before)));
    }

    proptest! {
        #[test]
        fn what_is_encrypted_decrypts_back(fill: u8, n in 1u8..5, iv: u8) {
            let plain = plain(n, fill);
            let db = encrypt(&plain, &KEY, &SALT_OF, |p| [iv ^ p as u8; 16]);
            prop_assert_eq!(decrypt(&key(), &db, &[]), Ok(rollback(plain)));
        }

        #[test]
        fn any_byte_changed_on_the_first_page_is_refused(at in 0usize..PAGE, flip in 1u8..) {
            let mut db = sealed(&plain(1, 0));
            db[at] ^= flip;
            prop_assert!(!opens(&key(), page(&db, 1)));
        }
    }
}
