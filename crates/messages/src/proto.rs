//! Protobuf, as far as wxread reads it: the length-delimited fields of a
//! message (bytes, strings and messages nested in it), found by number.
//! What does not read as protobuf ends the reading, never a panic.

/// The bytes of every length-delimited field numbered `number` in `bytes`,
/// in order: a repeated field's values.
pub fn fields(bytes: &[u8], number: u64) -> Vec<&[u8]> {
    let mut found = Vec::new();
    let mut rest = bytes;
    while let Some((field, value, after)) = next(rest) {
        if field == number
            && let Some(value) = value
        {
            found.push(value);
        }
        rest = after;
    }
    found
}

/// The bytes of the length-delimited field at `path` (each a field number,
/// the next inside it) in `bytes`: the first of each; None when it is not
/// there.
pub fn field<'a>(bytes: &'a [u8], path: &[u64]) -> Option<&'a [u8]> {
    let (&first, rest) = path.split_first()?;
    let value = fields(bytes, first).into_iter().next()?;
    if rest.is_empty() { Some(value) } else { field(value, rest) }
}

/// The text of the field at `path`, trimmed: None when it is not there,
/// not UTF-8, or empty.
pub fn text(bytes: &[u8], path: &[u64]) -> Option<String> {
    let text = std::str::from_utf8(field(bytes, path)?).ok()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A field: its number, its bytes when it is length-delimited, and what
/// follows it.
type Field<'a> = (u64, Option<&'a [u8]>, &'a [u8]);

/// The first field of `bytes`: None at the end, or where it does not read.
fn next(bytes: &[u8]) -> Option<Field<'_>> {
    let (tag, rest) = varint(bytes)?;
    let number = tag.checked_shr(3)?;
    match tag & 7 {
        0 => {
            let (_, rest) = varint(rest)?;
            Some((number, None, rest))
        }
        1 => Some((number, None, rest.get(8..)?)),
        5 => Some((number, None, rest.get(4..)?)),
        2 => {
            let (len, rest) = varint(rest)?;
            let len = usize::try_from(len).ok()?;
            let value = rest.get(..len)?;
            Some((number, Some(value), rest.get(len..)?))
        }
        _ => None,
    }
}

/// The varint `bytes` starts with, and what follows it.
fn varint(bytes: &[u8]) -> Option<(u64, &[u8])> {
    let mut n: u64 = 0;
    let mut shift: u32 = 0;
    let mut rest = bytes;
    while let Some((&b, after)) = rest.split_first() {
        rest = after;
        n |= u64::from(b & 0x7f).checked_shl(shift)?;
        if b & 0x80 == 0 {
            return Some((n, rest));
        }
        shift = shift.checked_add(7).filter(|s| *s < 64)?;
    }
    None
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A varint's bytes.
    pub fn varint_of(mut n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// Field `number` holding `value`, length-delimited.
    pub fn bytes_field(number: u64, value: &[u8]) -> Vec<u8> {
        let mut out = varint_of(number << 3 | 2);
        out.extend(varint_of(value.len() as u64));
        out.extend_from_slice(value);
        out
    }

    #[test]
    fn fields_are_found_by_number_through_others() {
        let mut m = varint_of(1 << 3);
        m.extend(varint_of(300));
        m.extend(bytes_field(3, &bytes_field(4, b"abc")));
        m.extend(varint_of(2 << 3 | 1));
        m.extend([0; 8]);
        m.extend(bytes_field(3, b"second"));
        assert_eq!(fields(&m, 3).len(), 2);
        assert_eq!(text(&m, &[3, 4]).as_deref(), Some("abc"));
        assert_eq!(field(&m, &[9]), None);
    }

    #[test]
    fn what_is_not_protobuf_ends_the_reading() {
        assert!(fields(&[0xff; 12], 1).is_empty());
        assert!(fields(&bytes_field(1, b"abc")[..3], 1).is_empty());
        assert_eq!(fields(&[0x0a, 0x7f, 0x01], 1), Vec::<&[u8]>::new());
    }
}
