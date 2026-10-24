//! The decoder against the SILK SDK's reference C decoder: `tone.silk`
//! (1.2 s of a 440 Hz tone, swelling at 3 Hz, 24 kHz, encoded with the
//! SDK's encoder as WeChat writes it, its leading 0x02 too: `rust-silk
//! encode --tencent`) decodes to the very samples the C decoder answers.

use sha2::{Digest, Sha256};

const TONE: &[u8] = include_bytes!("tone.silk");

#[test]
fn a_wechat_voice_decodes_to_the_reference_decoders_samples() {
    let pcm = silk::decode(TONE, 24_000).expect("decode tone.silk");
    assert_eq!(pcm.len(), 28_800, "1.2 s at 24 kHz");
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        // `rust-silk decode -i tone.silk -o tone.pcm`: the C decoder's.
        "e155bd2c1622b94428fd3f3574e34fd0e99bcf0ffdbf370711c5f38c957e6168"
    );
}

#[test]
fn what_is_not_silk_decodes_to_nothing() {
    for bytes in [&b""[..], b"#!AMR\n", b"\x02#!SILK_V3"] {
        assert_eq!(
            silk::decode(bytes, 24_000),
            Err(silk::SilkError::InvalidOrEmptySilkFile),
            "{bytes:?}"
        );
    }
    assert_eq!(silk::decode(TONE, 96_000), Err(silk::SilkError::InvalidSampleRate));
}
