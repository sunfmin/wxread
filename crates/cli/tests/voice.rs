//! `wxread voice`: a voice message found in its chat, its SILK read from
//! the account's encrypted media databases and decoded in-process into a
//! WAV that plays, or written as WeChat keeps it, with WeChat's own
//! transcription beside it.

use serde_json::{Value, json};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Said, Voice, field};

/// 1.2 s of a 440 Hz tone at 24 kHz, encoded as WeChat encodes a voice
/// message: the SILK decoder's own reference.
const TONE: &[u8] = include_bytes!("../../silk/tests/tone.silk");

const T: i64 = 1_760_000_000;

/// A voice message of `length_ms` from Amy, server id `id`, its
/// `packed_info_data` holding `heard` when the account converted it.
fn voice(id: i64, length_ms: u32, heard: Option<&str>) -> Said {
    let mut said = Said::text(
        "wxid_amy",
        id,
        T + id,
        "wxid_amy",
        &format!(r#"<msg><voicemsg voicelength="{length_ms}" length="4090"/></msg>"#),
    );
    said.local_type = 34;
    if let Some(heard) = heard {
        let mut done = vec![0x08, 0x02];
        done.extend(field(2, heard.as_bytes()));
        said.packed = field(5, &done);
    }
    said
}

/// Amy's chat: the tone, converted to text; another voice, not converted,
/// whose sound WeChat does not hold; a third whose sound is not SILK; and
/// a text.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.messages = vec![
        voice(101, 1200, Some("周六见")),
        voice(102, 3000, None),
        voice(103, 900, None),
        Said::text("wxid_amy", 104, T + 104, "wxid_amy", "hello"),
    ];
    account.voices = vec![
        Voice("wxid_amy", 101, TONE.to_vec()),
        Voice("wxid_amy", 103, b"#!AMR\nnot silk".to_vec()),
    ];
    account.write();
    account
}

/// The path of `name` in the world's own directory, as wxread is told it.
fn out(world: &World, name: &str) -> String {
    world.root.join(name).display().to_string()
}

#[test]
fn a_voice_message_decodes_to_a_wav_as_long_as_the_message_and_nothing_else_is_written() {
    let world = World::new();
    let account = account(&world);
    let before = snapshot(&world.data_dir());
    let path = out(&world, "amy.wav");
    let json = world
        .run(
            &["voice", "101", "--chat", "wxid_amy", "--out", &path],
            &[("WXREAD_KEYS", &account.all_keys())],
        )
        .ok();
    let wav = std::fs::read(&path).unwrap();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "chat": {"id": "wxid_amy", "display_name": ""},
            "id": "101",
            "path": path,
            "format": "wav",
            "duration_ms": 1200,
            "size": wav.len(),
            "length_ms": 1200,
            "transcription": "周六见",
        })
    );
    assert_eq!(json["duration_ms"], json["length_ms"], "as long as WeChat says it is");
    // A WAV a player plays: RIFF, PCM, mono, 24 kHz, 16 bits, and the
    // samples the decoder answers.
    assert_eq!((&wav[..4], &wav[8..12], &wav[36..40]), (&b"RIFF"[..], &b"WAVE"[..], &b"data"[..]));
    let u16_at = |at: usize| u16::from_le_bytes([wav[at], wav[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(wav[at..at + 4].try_into().unwrap());
    assert_eq!((u16_at(20), u16_at(22), u32_at(24), u16_at(34)), (1, 1, 24_000, 16));
    assert_eq!(u32_at(40) as usize, 2 * 28_800, "1.2 s of samples");
    let pcm = silk::decode(TONE, 24_000).unwrap();
    assert_eq!(wav, wxread_voice::wav(&pcm, 24_000).unwrap());
    assert_eq!(snapshot(&world.data_dir()), before, "WeChat's files are untouched");
    assert_eq!(std::fs::read_dir(&world.root).unwrap().count(), 2, "home and the WAV alone");
}

#[test]
fn raw_writes_the_silk_byte_for_byte() {
    let world = World::new();
    let account = account(&world);
    let path = out(&world, "amy.silk");
    let json = world
        .run(
            &["voice", "101", "--chat", "wxid_amy", "--out", &path, "--raw"],
            &[("WXREAD_KEYS", &account.all_keys())],
        )
        .ok();
    assert_eq!(std::fs::read(&path).unwrap(), TONE);
    assert_eq!(json["format"], "silk");
    assert_eq!(json["size"], TONE.len());
    assert_eq!(json["duration_ms"], 1200);
    // Sound that is not SILK is still written as WeChat keeps it, its
    // duration unknown; as a WAV it is refused.
    let path = out(&world, "odd.silk");
    let env = [("WXREAD_KEYS", account.all_keys())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let json =
        world.run(&["voice", "103", "--chat", "wxid_amy", "--out", &path, "--raw"], &env).ok();
    assert_eq!(std::fs::read(&path).unwrap(), b"#!AMR\nnot silk");
    assert_eq!(json["duration_ms"], Value::Null);
    let wav = out(&world, "odd.wav");
    let error = world.run(&["voice", "103", "--chat", "wxid_amy", "--out", &wav], &env).error();
    assert!(error.contains("does not decode as SILK") && error.contains("--raw"), "{error}");
    assert!(!std::path::Path::new(&wav).exists(), "nothing written on a refusal");
}

#[test]
fn wechats_transcription_is_on_the_message_when_the_account_made_one_and_null_otherwise() {
    let world = World::new();
    let account = account(&world);
    let env = [("WXREAD_KEYS", account.all_keys())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let json = world.run(&["messages", "--chat", "wxid_amy"], &env).ok();
    let voices: Vec<(&str, &Value)> = json["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] == "voice")
        .map(|m| (m["id"].as_str().unwrap(), &m["voice"]))
        .collect();
    assert_eq!(
        voices,
        [
            (
                "103",
                &json!({ "length_ms": 900, "transcription": null, "file_present": true, "thumbnail_present": null })
            ),
            (
                "102",
                &json!({ "length_ms": 3000, "transcription": null, "file_present": false, "thumbnail_present": null })
            ),
            (
                "101",
                &json!({ "length_ms": 1200, "transcription": "周六见", "file_present": true, "thumbnail_present": null })
            ),
        ]
    );
    let path = out(&world, "odd.silk");
    let json =
        world.run(&["voice", "103", "--chat", "wxid_amy", "--out", &path, "--raw"], &env).ok();
    assert_eq!(json["transcription"], Value::Null);
}

#[test]
fn each_refusal_names_its_fix_and_writes_nothing() {
    let world = World::new();
    let account = account(&world);
    let env = [("WXREAD_KEYS", account.all_keys())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let path = out(&world, "x.wav");
    let refused = |args: &[&str], says: &[&str]| {
        let error = world.run(args, &env).error();
        for words in says {
            assert!(error.contains(words), "{args:?}: {words:?} in {error}");
        }
    };
    refused(&["voice", "101", "--out", &path], &["no chat was named", "--chat"]);
    refused(&["voice", "101", "--chat", "wxid_amy"], &["--out <path>"]);
    refused(
        &["voice", "abc", "--chat", "wxid_amy", "--out", &path],
        &["abc is not a message id", "wxread messages"],
    );
    refused(
        &["voice", "999", "--chat", "wxid_amy", "--out", &path],
        &["has no message 999", "wxread messages --chat wxid_amy"],
    );
    refused(
        &["voice", "104", "--chat", "wxid_amy", "--out", &path],
        &["is text, not voice", "\"kind\": \"voice\""],
    );
    refused(
        &["voice", "102", "--chat", "wxid_amy", "--out", &path],
        &["holds the sound of voice message 102", "play it once in WeChat"],
    );
    refused(&["voice", "101", "--chat", "wxid_bob", "--out", &path], &["wxread chats"]);
    let nowhere = out(&world, "no/such/dir/x.wav");
    refused(
        &["voice", "101", "--chat", "wxid_amy", "--out", &nowhere],
        &["cannot be written", "--out"],
    );
    assert!(!std::path::Path::new(&path).exists());
    // The media databases' keys are needed like the message databases'.
    let keys = account.keys(&["message/message_0.db"]);
    let error = world
        .run(&["voice", "101", "--chat", "wxid_amy", "--out", &path], &[("WXREAD_KEYS", &keys)])
        .error();
    assert!(error.contains("message/media_0.db") && error.contains("sudo wxread keys"), "{error}");
}
