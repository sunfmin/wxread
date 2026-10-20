//! What `wxread voice` decides of a voice message: the message id it was
//! given, which of the account's media databases hold its sound, the WAV
//! its decoded samples are written as and how long they last, and each
//! refusal with its fix. The effect crate reads the databases, decodes the
//! SILK (docs/adr/0005-voice-is-decoded-in-process.md) and writes the file;
//! nothing here reads or writes.

/// The options of `wxread voice`.
pub const OUT_FLAG: &str = "--out";
pub const RAW_FLAG: &str = "--raw";

/// The rate the SILK is decoded at, and the WAV plays at: WeChat records
/// its voice messages at 24 kHz.
pub const SAMPLE_RATE: u32 = 24_000;

/// What the file written is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The decoded sound: 16-bit mono PCM in a WAV, which every player
    /// plays.
    Wav,
    /// The SILK as WeChat keeps it, byte for byte.
    Silk,
}

impl Format {
    /// The format `--raw` asks for.
    pub fn of(raw: bool) -> Format {
        if raw { Format::Silk } else { Format::Wav }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Silk => "silk",
        }
    }
}

/// The server id `text` names a message by, or what fixes it.
pub fn message_id(text: &str) -> Result<i64, String> {
    match text.parse::<i64>() {
        Ok(id) if id > 0 => Ok(id),
        _ => Err(format!(
            "{text} is not a message id: pass the id of a voice message as `wxread messages` answers it, a number such as 7234567890123456789"
        )),
    }
}

/// Whether `bytes` are a SILK v3 stream: with Tencent's leading 0x02, or
/// without it.
pub fn is_silk(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x02#!SILK_V3") || bytes.starts_with(b"#!SILK_V3")
}

/// The media databases `databases` (each a path under `db_storage`) holds,
/// by path (`message/media_0.db`), each by its number: where WeChat keeps
/// the sound of voice messages.
pub fn media_dbs(databases: &[String]) -> Vec<String> {
    let mut found: Vec<(u32, String)> = databases
        .iter()
        .filter_map(|path| {
            let n = path.strip_prefix(wxread_schema::MEDIA_DBS)?.strip_suffix(".db")?;
            if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some((n.parse().ok()?, path.clone()))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
}

/// The media databases, as a refusal of missing keys names them.
pub const MEDIA_DB_PATHS: &str = "message/media_<n>.db";

/// How long `samples` samples last at `rate` Hz, in milliseconds, rounded
/// down; None for a rate of 0.
pub fn duration_ms(samples: usize, rate: u32) -> Option<u64> {
    if rate == 0 {
        return None;
    }
    u64::try_from(samples).ok()?.checked_mul(1000)?.checked_div(u64::from(rate))
}

/// `pcm`, 16-bit mono samples at `rate` Hz, as a WAV file; None when there
/// are more than a WAV holds (4 GiB).
pub fn wav(pcm: &[i16], rate: u32) -> Option<Vec<u8>> {
    let data = u32::try_from(pcm.len()).ok()?.checked_mul(2)?;
    let riff = data.checked_add(36)?;
    let byte_rate = rate.checked_mul(2)?;
    let mut out = Vec::with_capacity(usize::try_from(riff).ok()?.checked_add(8)?);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff.to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    // Two bytes a frame, sixteen bits a sample.
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    Some(out)
}

/// What fixes a run with no `--out`.
pub fn no_out() -> String {
    format!(
        "no file to write was named: pass {OUT_FLAG} <path>, where the voice message is written (a WAV; with {RAW_FLAG}, the SILK as WeChat keeps it)"
    )
}

/// What fixes a message `id` that chat `chat` of `account` does not have.
pub fn no_such_message(id: i64, chat: &str, account: &str) -> String {
    format!(
        "chat {chat} of account {account} has no message {id}: pass the id of one of its voice messages, as `wxread messages --chat {chat}` answers it"
    )
}

/// What fixes a message `id` of `chat` that is of kind `kind`, not voice.
pub fn not_a_voice(id: i64, chat: &str, kind: &str) -> String {
    format!(
        "message {id} of chat {chat} is {kind}, not voice: pass the id of a message `wxread messages --chat {chat}` answers as \"kind\": \"voice\""
    )
}

/// What fixes a voice message `id` of `chat` whose sound no media database
/// of `account` holds.
pub fn no_sound(id: i64, chat: &str, account: &str) -> String {
    format!(
        "no media database of account {account} holds the sound of voice message {id} of chat {chat}: WeChat has not downloaded it, or has cleared it; play it once in WeChat on this Mac, so WeChat keeps it, and run this again"
    )
}

/// What fixes a voice message `id` of `chat` whose sound is not SILK, or
/// does not decode: `why` the decoder's reason.
pub fn undecodable(id: i64, chat: &str, why: &str) -> String {
    format!(
        "the sound of voice message {id} of chat {chat} does not decode as SILK ({why}): pass {RAW_FLAG} to write it as WeChat keeps it"
    )
}

/// What fixes a file `path` that could not be written, as the system said.
pub fn unwritable(path: &str, why: &str) -> String {
    format!(
        "{path} cannot be written ({why}): pass {OUT_FLAG} a path in a directory that exists and may be written"
    )
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_message_id_is_a_positive_number() {
        assert_eq!(message_id("7234567890123456789"), Ok(7_234_567_890_123_456_789));
        for bad in ["", "0", "-3", "abc", "1.5"] {
            assert!(message_id(bad).unwrap_err().contains("wxread messages"), "{bad}");
        }
    }

    #[test]
    fn silk_is_known_by_its_header() {
        assert!(is_silk(b"\x02#!SILK_V3\x0a\x00"));
        assert!(is_silk(b"#!SILK_V3"));
        assert!(!is_silk(b"#!AMR\n"));
        assert!(!is_silk(b""));
    }

    #[test]
    fn the_media_databases_are_in_their_numbers_order() {
        let all: Vec<String> = [
            "message/media_10.db",
            "message/message_0.db",
            "message/media_2.db",
            "message/media_.db",
            "message/media_x.db",
            "message/media_0.db",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            media_dbs(&all),
            ["message/media_0.db", "message/media_2.db", "message/media_10.db"]
        );
    }

    #[test]
    fn a_duration_is_its_samples_over_the_rate() {
        assert_eq!(duration_ms(28_800, SAMPLE_RATE), Some(1200));
        assert_eq!(duration_ms(23_999, SAMPLE_RATE), Some(999));
        assert_eq!(duration_ms(1, 0), None);
    }

    #[test]
    fn the_format_is_what_raw_says() {
        assert_eq!(Format::of(false).name(), "wav");
        assert_eq!(Format::of(true).name(), "silk");
    }

    proptest! {
        #[test]
        fn a_wav_holds_its_samples_after_a_header_that_says_them(
            pcm in proptest::collection::vec(any::<i16>(), 0..2000),
            rate in 8_000u32..48_000,
        ) {
            let wav = wav(&pcm, rate).unwrap();
            let u32_at = |at: usize| u32::from_le_bytes(wav[at..at + 4].try_into().unwrap());
            let u16_at = |at: usize| u16::from_le_bytes(wav[at..at + 2].try_into().unwrap());
            prop_assert_eq!(&wav[..4], b"RIFF");
            prop_assert_eq!(u32_at(4) as usize, wav.len() - 8);
            prop_assert_eq!(&wav[8..16], b"WAVEfmt ");
            prop_assert_eq!((u16_at(20), u16_at(22)), (1, 1));
            prop_assert_eq!(u32_at(24), rate);
            prop_assert_eq!(u32_at(28), rate * 2);
            prop_assert_eq!((u16_at(32), u16_at(34)), (2, 16));
            prop_assert_eq!(&wav[36..40], b"data");
            prop_assert_eq!(u32_at(40) as usize, pcm.len() * 2);
            let samples: Vec<i16> =
                wav[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            prop_assert_eq!(samples, pcm);
        }
    }
}
