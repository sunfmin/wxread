//! An image `.dat` as WeChat for Mac 4.x keeps it, and the key it is
//! sealed under. Three formats, as wechat-cli and wechat-decrypt found
//! them and bizkernel's wechat-intake reads them:
//!
//! - V2: a 15-byte header (`07 08 56 32 08 07`, the AES part's size and
//!   the XOR part's, each a little-endian u32, one byte more), then the
//!   image's first bytes AES-128-ECB under the account's image key with
//!   PKCS#7 padding, its middle as it is, and its last bytes XORed with
//!   the key's byte;
//! - V1: the same, its header `07 08 56 31 08 07`, under one key everybody
//!   shares;
//! - the older format: every byte XORed with one byte.
//!
//! The account's image key is derived without reading WeChat's memory:
//! the first 16 hex digits of the MD5 of `<code><account id>`, `code` the
//! number in the names of WeChat's kvcomm files (`key_<code>_….statistic`),
//! its low byte the XOR byte (found by wxkey, MIT). Which code and id it is
//! is told by the image it opens. Nothing here reads a file.

use aes::Aes128;
use aes::cipher::{BlockDecrypt as _, BlockEncrypt as _, KeyInit as _};

/// The directories under WeChat's Documents (the data directory's parent)
/// its kvcomm files are in, as WeChat for Mac 4.x versions have kept them.
pub const KVCOMM_DIRS: [&str; 4] = [
    "app_data/net/kvcomm",
    "xwechat/net/kvcomm",
    "app_data/ilink/kvcomm",
    "app_data/roam/ilink/kvcomm",
];
/// Under Documents, a directory of months whose each `kvcomm` holds them
/// too.
pub const RADIUM: &str = "app_data/radium/ilink";

/// A V2 or V1 header's length, and what it starts with.
const HEADER: usize = 15;
const BLOCK: usize = 16;
const V1: u8 = 0x31;
const V2: u8 = 0x32;
/// V1's key, everybody's: the first 16 hex digits of md5("0").
const V1_KEY: [u8; BLOCK] = *b"cfcd208495d565ef";
/// How much of an image `seal` puts under AES, and at most under XOR, as
/// WeChat does.
const SEALED: usize = 1024;
const XORED: usize = 1024;

/// An account's image key: AES-128's key, and the byte its tails are
/// XORed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageKey {
    pub aes: [u8; BLOCK],
    pub xor: u8,
}

impl ImageKey {
    /// The key derived from the kvcomm code `code` and the account id `id`.
    pub fn derive(code: u32, id: &str) -> ImageKey {
        let digest = format!("{:x}", md5::compute(format!("{code}{id}")));
        let mut aes = [0u8; BLOCK];
        for (slot, b) in aes.iter_mut().zip(digest.bytes()) {
            *slot = b;
        }
        ImageKey { aes, xor: (code & 0xff) as u8 }
    }

    /// Whether it opens `dat`, a V2 image: its first block decrypts to an
    /// image's first bytes.
    pub fn opens(&self, dat: &[u8]) -> bool {
        if version(dat) != Some(V2) {
            return false;
        }
        match dat.get(HEADER..).and_then(|b| b.first_chunk::<BLOCK>()) {
            Some(block) => is_image(&ecb_decrypt(&self.aes, block)),
            None => false,
        }
    }
}

/// The kvcomm code a kvcomm file's name `name` holds
/// (`key_<code>_….statistic`): None for any other file.
pub fn kvcomm_code(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("key_")?;
    let (code, _) = rest.split_once('_')?;
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

/// The key derived from one of `codes` and one of `ids` that opens
/// `sample`, a V2 image of the account: the first, in their order.
pub fn image_key(codes: &[u32], ids: &[&str], sample: &[u8]) -> Option<ImageKey> {
    for id in ids {
        for code in codes {
            let key = ImageKey::derive(*code, id);
            if key.opens(sample) {
                return Some(key);
            }
        }
    }
    None
}

/// Whether `dat` is V2, so needs the account's image key to be read.
pub fn needs_key(dat: &[u8]) -> bool {
    version(dat) == Some(V2)
}

/// Why an image `.dat` was not decrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// It is in none of WeChat's formats.
    NotWechats,
    /// It is V2, and no image key was found.
    NoKey,
    /// The key does not open it: what it decrypts to is no image.
    KeyDoesNotOpen,
    /// Its header's sizes do not fit the file: it is cut short.
    Short,
}

/// The image `dat` holds, in whichever of WeChat's formats it is, `key`
/// the account's image key when one was found: a JPEG, a PNG, a GIF or
/// WeChat's own wxgf.
pub fn decrypt(dat: &[u8], key: Option<&ImageKey>) -> Result<Vec<u8>, Refusal> {
    if version(dat).is_none() {
        return xored(dat).ok_or(Refusal::NotWechats);
    }
    let out = unseal(dat, key)?;
    if !is_image(&out) {
        return Err(Refusal::KeyDoesNotOpen);
    }
    Ok(out)
}

/// Whether `dat` is sealed in V1 or V2, whatever file it holds.
pub fn is_sealed(dat: &[u8]) -> bool {
    version(dat).is_some()
}

/// The file `dat` holds sealed in V1 or V2, whatever file it is (a
/// favorite's note or PDF is sealed as an image is), `key` the account's
/// image key when one was found. Only its padding tells the key opens it:
/// what it holds is the caller's to check.
pub fn unseal(dat: &[u8], key: Option<&ImageKey>) -> Result<Vec<u8>, Refusal> {
    let v = version(dat).ok_or(Refusal::NotWechats)?;
    let (aes, xor) = if v == V2 {
        let key = key.ok_or(Refusal::NoKey)?;
        (key.aes, key.xor)
    } else {
        (V1_KEY, key.map_or(0, |k| k.xor))
    };
    let aes_size = le32(dat, 6) as usize;
    let xor_size = le32(dat, 10) as usize;
    let body = dat.get(HEADER..).unwrap_or_default();
    let sealed = padded(aes_size).ok_or(Refusal::Short)?;
    let (head, rest) = body.split_at_checked(sealed).ok_or(Refusal::Short)?;
    let middle_len = rest.len().checked_sub(xor_size).ok_or(Refusal::Short)?;
    let (middle, tail) = rest.split_at_checked(middle_len).ok_or(Refusal::Short)?;
    let mut out = Vec::with_capacity(body.len());
    for block in head.as_chunks::<BLOCK>().0 {
        out.extend_from_slice(&ecb_decrypt(&aes, block));
    }
    let pad = out.last().copied().unwrap_or(0);
    let pad_len = usize::from(pad);
    if pad_len == 0 || pad_len > BLOCK {
        return Err(Refusal::KeyDoesNotOpen);
    }
    let keep = out.len().checked_sub(pad_len).ok_or(Refusal::KeyDoesNotOpen)?;
    if out.get(keep..).is_none_or(|p| p.iter().any(|b| *b != pad)) {
        return Err(Refusal::KeyDoesNotOpen);
    }
    out.truncate(keep);
    out.extend_from_slice(middle);
    out.extend(tail.iter().map(|b| b ^ xor));
    Ok(out)
}

/// How an image is sealed: V2 under an account's key, V1 with its tail
/// XORed with a byte, or the older format under one byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    V2(ImageKey),
    V1 { xor: u8 },
    Xor(u8),
}

/// `image` sealed as WeChat seals one in `format`: what the tests make an
/// account's images with, as `wxread_sqlcipher::encrypt` its databases.
pub fn seal(image: &[u8], format: Format) -> Vec<u8> {
    let (v, aes, xor) = match format {
        Format::Xor(k) => return image.iter().map(|b| b ^ k).collect(),
        Format::V2(key) => (V2, key.aes, key.xor),
        Format::V1 { xor } => (V1, V1_KEY, xor),
    };
    let aes_size = image.len().min(SEALED);
    let (head, rest) = image.split_at(aes_size);
    let xor_size = rest.len().min(XORED);
    let (middle, tail) = rest.split_at(rest.len().saturating_sub(xor_size));
    let mut padded_head = head.to_vec();
    let pad = BLOCK.saturating_sub(aes_size % BLOCK);
    padded_head.extend(std::iter::repeat_n(pad as u8, pad));
    let mut out = vec![0x07, 0x08, 0x56, v, 0x08, 0x07];
    out.extend((aes_size as u32).to_le_bytes());
    out.extend((xor_size as u32).to_le_bytes());
    out.push(1);
    for block in padded_head.as_chunks::<BLOCK>().0 {
        out.extend_from_slice(&ecb_encrypt(&aes, block));
    }
    out.extend_from_slice(middle);
    out.extend(tail.iter().map(|b| b ^ xor));
    out
}

/// The version byte of a V1 or V2 header `dat` starts with.
fn version(dat: &[u8]) -> Option<u8> {
    match dat.first_chunk::<HEADER>() {
        Some([0x07, 0x08, 0x56, v, 0x08, 0x07, ..]) if *v == V1 || *v == V2 => Some(*v),
        _ => None,
    }
}

/// The older format: every byte XORed with the one that makes its first
/// bytes a JPEG's, a PNG's or a GIF's.
fn xored(dat: &[u8]) -> Option<Vec<u8>> {
    let heads: [&[u8]; 3] = [&[0xff, 0xd8, 0xff], &[0x89, 0x50, 0x4e, 0x47], b"GIF8"];
    let first = *dat.first()?;
    for head in heads {
        let k = first ^ head.first().copied().unwrap_or(0);
        if dat.len() > head.len() && dat.iter().zip(head).all(|(d, h)| d ^ k == *h) {
            return Some(dat.iter().map(|b| b ^ k).collect());
        }
    }
    None
}

/// `size` bytes padded as PKCS#7 pads them: up to the next whole block,
/// a whole block more when they already end one.
fn padded(size: usize) -> Option<usize> {
    (size / BLOCK).checked_add(1)?.checked_mul(BLOCK)
}

/// The little-endian word at `at` of `bytes`, 0 past their end.
fn le32(bytes: &[u8], at: usize) -> u32 {
    let mut four = [0u8; 4];
    for (i, slot) in four.iter_mut().enumerate() {
        *slot = bytes.get(at.saturating_add(i)).copied().unwrap_or(0);
    }
    u32::from_le_bytes(four)
}

fn ecb_decrypt(key: &[u8; BLOCK], block: &[u8; BLOCK]) -> [u8; BLOCK] {
    let mut b = aes::Block::from(*block);
    Aes128::new(&(*key).into()).decrypt_block(&mut b);
    b.into()
}

fn ecb_encrypt(key: &[u8; BLOCK], block: &[u8; BLOCK]) -> [u8; BLOCK] {
    let mut b = aes::Block::from(*block);
    Aes128::new(&(*key).into()).encrypt_block(&mut b);
    b.into()
}

/// Whether `bytes` start as an image does: a JPEG, a PNG, a GIF, or
/// WeChat's own wxgf.
fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47])
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"wxgf")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A JPEG of `len` bytes past its marker.
    fn jpeg(len: usize) -> Vec<u8> {
        let mut image = vec![0xff, 0xd8, 0xff, 0xe0];
        image.extend((0..len).map(|i| (i * 7 % 251) as u8));
        image
    }

    #[test]
    fn the_key_is_the_md5_of_the_code_and_the_id() {
        // md5("1038088560wxid_amy") = 6f6c0c8c…, as wxkey derives it.
        let key = ImageKey::derive(1038088560, "wxid_amy");
        let hex = format!("{:x}", md5::compute("1038088560wxid_amy"));
        assert_eq!(&key.aes[..], &hex.as_bytes()[..16]);
        assert_eq!(key.xor, (1038088560u32 & 0xff) as u8);
    }

    #[test]
    fn a_kvcomm_files_name_holds_its_code() {
        assert_eq!(kvcomm_code("key_1038088560_2.statistic"), Some(1038088560));
        assert_eq!(kvcomm_code("key_12_x"), Some(12));
        for other in ["key_.statistic", "key_x1_2", "monitor_1_2", "key_99999999999_1", "key_12"] {
            assert_eq!(kvcomm_code(other), None, "{other}");
        }
    }

    #[test]
    fn a_v2_image_opens_under_its_key_only() {
        let key = ImageKey::derive(77, "wxid_amy");
        let image = jpeg(3000);
        let dat = seal(&image, Format::V2(key));
        assert!(needs_key(&dat) && key.opens(&dat));
        assert_eq!(decrypt(&dat, Some(&key)), Ok(image));
        let other = ImageKey::derive(78, "wxid_amy");
        assert!(!other.opens(&dat));
        assert_eq!(decrypt(&dat, Some(&other)), Err(Refusal::KeyDoesNotOpen));
        assert_eq!(decrypt(&dat, None), Err(Refusal::NoKey));
    }

    #[test]
    fn the_key_is_the_one_whose_code_and_id_open_the_sample() {
        let key = ImageKey::derive(5, "wxid_me");
        let sample = seal(&jpeg(40), Format::V2(key));
        assert_eq!(image_key(&[1, 5, 9], &["wxid_me_1a2b", "wxid_me"], &sample), Some(key));
        assert_eq!(image_key(&[1, 9], &["wxid_me"], &sample), None);
        assert_eq!(image_key(&[5], &["wxid_me"], &jpeg(40)), None);
    }

    #[test]
    fn v1_and_the_older_format_need_no_key() {
        let image = jpeg(500);
        assert_eq!(decrypt(&seal(&image, Format::V1 { xor: 0 }), None), Ok(image.clone()));
        let key = ImageKey::derive(0x1234, "x");
        let v1 = seal(&image, Format::V1 { xor: key.xor });
        assert!(!needs_key(&v1));
        assert_eq!(decrypt(&v1, Some(&key)), Ok(image.clone()));
        let png = [&[0x89, 0x50, 0x4e, 0x47][..], b"\r\n\x1a\nrest"].concat();
        assert_eq!(decrypt(&seal(&png, Format::Xor(0x5a)), None), Ok(png));
        assert_eq!(decrypt(&seal(&image, Format::Xor(0)), None), Ok(image));
    }

    #[test]
    fn wechats_own_wxgf_is_an_image() {
        let key = ImageKey::derive(1, "a");
        let wxgf = [&b"wxgf"[..], &[1; 60]].concat();
        assert_eq!(decrypt(&seal(&wxgf, Format::V2(key)), Some(&key)), Ok(wxgf));
    }

    #[test]
    fn what_is_not_an_image_or_is_cut_short_is_refused() {
        assert_eq!(decrypt(b"not an image", None), Err(Refusal::NotWechats));
        assert_eq!(decrypt(&[], None), Err(Refusal::NotWechats));
        let key = ImageKey::derive(3, "a");
        let dat = seal(&jpeg(2000), Format::V2(key));
        assert_eq!(decrypt(&dat[..HEADER + 100], Some(&key)), Err(Refusal::Short));
        let mut huge = dat.clone();
        huge[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decrypt(&huge, Some(&key)), Err(Refusal::Short));
        let mut wrong = dat;
        wrong[HEADER] ^= 1;
        assert_eq!(decrypt(&wrong, Some(&key)), Err(Refusal::KeyDoesNotOpen));
    }

    #[test]
    fn a_sealed_file_of_any_kind_unseals_and_only_under_its_key() {
        let key = ImageKey::derive(9, "wxid_me");
        let note = b"<html><body>a note</body></html>".to_vec();
        let dat = seal(&note, Format::V2(key));
        assert!(is_sealed(&dat));
        assert_eq!(unseal(&dat, Some(&key)), Ok(note.clone()));
        assert_eq!(decrypt(&dat, Some(&key)), Err(Refusal::KeyDoesNotOpen));
        let pdf = b"%PDF-1.7 topo".to_vec();
        assert_eq!(unseal(&seal(&pdf, Format::V1 { xor: 0 }), None), Ok(pdf));
        assert_eq!(unseal(&dat, None), Err(Refusal::NoKey));
        // Only the last sealed block's padding tells a wrong key.
        let mut wrong = dat;
        wrong[HEADER + note.len()] ^= 1;
        assert_eq!(unseal(&wrong, Some(&key)), Err(Refusal::KeyDoesNotOpen));
        assert!(!is_sealed(b"%PDF-1.7 as it is"));
        assert_eq!(unseal(b"%PDF-1.7 as it is", None), Err(Refusal::NotWechats));
    }

    proptest! {
        #[test]
        fn every_file_seals_and_unseals_back(
            file in proptest::collection::vec(any::<u8>(), 0..3000),
            code in any::<u32>(),
            v2 in any::<bool>(),
        ) {
            let key = ImageKey::derive(code, "wxid_p");
            let format = if v2 { Format::V2(key) } else { Format::V1 { xor: key.xor } };
            prop_assert_eq!(unseal(&seal(&file, format), Some(&key)), Ok(file));
        }

        #[test]
        fn every_image_seals_and_decrypts_back(
            len in 0usize..4000,
            code in any::<u32>(),
            v2 in any::<bool>(),
        ) {
            let image = jpeg(len);
            let key = ImageKey::derive(code, "wxid_p");
            let format = if v2 { Format::V2(key) } else { Format::V1 { xor: key.xor } };
            prop_assert_eq!(decrypt(&seal(&image, format), Some(&key)), Ok(image));
        }

        #[test]
        fn no_bytes_panic_it(dat in proptest::collection::vec(any::<u8>(), 0..200), v2 in any::<bool>()) {
            let mut dat = dat;
            if dat.len() >= HEADER {
                dat[..6].copy_from_slice(&[0x07, 0x08, 0x56, if v2 { V2 } else { V1 }, 0x08, 0x07]);
            }
            let _ = decrypt(&dat, Some(&ImageKey::derive(1, "a")));
        }
    }
}
