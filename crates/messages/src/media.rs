//! What a media message is made of: what its file is found by, as WeChat
//! for Mac 4.x keeps it, under the account's directory (the one above
//! `db_storage`), as bizkernel's wechat-intake found it:
//!
//! - an image: `msg/attach/<md5 of the chat>/<YYYY-MM>/Img/<name>.dat`
//!   (`_h.dat` the full one, `_t.dat` its thumbnail), encrypted; `<name>`
//!   is in `packed_info_data` (field 3, its field 4);
//! - a video: `msg/video/<YYYY-MM>/<name>.mp4` and `<name>_thumb.jpg`,
//!   `<name>` in `packed_info_data` (field 4, its field 8);
//! - a file: `msg/file/<YYYY-MM>/<name>`, `<name>` in `packed_info_data`
//!   (field 7, its field 2), else its title;
//! - a voice message: its SILK in `message/media_<n>.db`, by its chat and
//!   the message's server id; and the text WeChat heard in it, once the
//!   account converted it to text (转文字), in `packed_info_data` (field 5,
//!   its field 2; its field 1 is 2 once done, 0 while it holds nothing, as
//!   WeChat for Mac 4.1 keeps it);
//! - a sticker: on WeChat's CDN, its address, checked against its MD5.

use serde_json::{Map, Value, json};
use wxread_media::Media;

use crate::Kind;
use crate::kinds::{attr, child, text, xml};
use crate::proto;

/// What a message of the media kind `kind`, saying `content` with `packed`
/// its `packed_info_data`, is made of; each field null when the message
/// does not say it.
pub fn fields(kind: Kind, content: &str, packed: &[u8]) -> Map<String, Value> {
    let read = |tag: &str, name: Option<&str>| {
        xml(content, |root| {
            let mut found = None;
            for n in root.descendants() {
                if n.has_tag_name(tag) {
                    found = Some(n);
                    break;
                }
            }
            let node = found?;
            match name {
                Some(name) => attr(node, name),
                None => text(node.parent()?, tag),
            }
        })
    };
    let number = |tag, name| read(tag, Some(name)).and_then(|n| n.parse::<u64>().ok());
    let fields = match kind {
        Kind::Image => {
            json!({ "name": proto::text(packed, &[3, 4]), "md5": read("img", Some("md5")) })
        }
        Kind::Video => json!({
            "name": proto::text(packed, &[4, 8]),
            "seconds": number("videomsg", "playlength"),
            "md5": read("videomsg", Some("md5")),
        }),
        Kind::File => {
            let title = xml(content, |root| text(child(root, "appmsg")?, "title"));
            json!({
                "name": title,
                "stored_as": proto::text(packed, &[7, 2]).or(title.clone()),
                "size": read("totallen", None).and_then(|n| n.parse::<u64>().ok()),
                "extension": read("fileext", None),
            })
        }
        Kind::Voice => json!({
            "length_ms": number("voicemsg", "voicelength"),
            "transcription": proto::text(packed, &[5, 2]),
        }),
        Kind::Sticker => {
            let md5 = read("emoji", Some("md5")).or_else(|| read("emoticonmd5", None));
            let url = read("emoji", Some("cdnurl")).or_else(|| {
                // An app message's sticker keeps its address in
                // `emojiinfo`, base64 of a protobuf: its field 2.
                proto::text(&base64(&read("emojiinfo", None)?)?, &[2])
            });
            json!({ "md5": md5.map(|m| m.to_ascii_lowercase()), "url": url })
        }
        _ => json!({}),
    };
    match fields {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// What finds the file of a message of the media kind `kind` whose
/// fields [`fields`] read as `fields`: None for a kind that carries none.
pub fn locate(kind: Kind, fields: &Map<String, Value>) -> Option<Media> {
    let text = |name: &str| fields.get(name).and_then(Value::as_str).map(str::to_string);
    match kind {
        Kind::Image => Some(Media::Image { name: text("name") }),
        Kind::Video => Some(Media::Video { name: text("name") }),
        Kind::File => Some(Media::File {
            name: text("stored_as"),
            size: fields.get("size").and_then(Value::as_u64),
        }),
        Kind::Sticker => Some(Media::Sticker { md5: text("md5"), url: text("url") }),
        Kind::Voice => Some(Media::Voice),
        _ => None,
    }
}

/// The bytes base64 `text` writes, whitespace and padding passed over:
/// None when a character is not base64's.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc & 0xff_ffff).checked_shl(6)? | u32::from(v);
        bits = bits.checked_add(6)?;
        if bits >= 8 {
            bits = bits.checked_sub(8)?;
            out.push(u8::try_from(acc.checked_shr(bits)? & 0xff).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::tests::bytes_field;

    #[test]
    fn each_media_kind_is_found_by_its_fields() {
        let packed = bytes_field(3, &bytes_field(4, b"abc123"));
        let image = fields(Kind::Image, "<msg><img/></msg>", &packed);
        assert_eq!(locate(Kind::Image, &image), Some(Media::Image { name: Some("abc123".into()) }));
        let xml = "<msg><appmsg><title>topo.pdf</title><type>6</type><appattach><totallen>2048</totallen></appattach></appmsg></msg>";
        assert_eq!(
            locate(Kind::File, &fields(Kind::File, xml, &[])),
            Some(Media::File { name: Some("topo.pdf".into()), size: Some(2048) })
        );
        let voice = fields(Kind::Voice, "<msg/>", &[]);
        assert_eq!(locate(Kind::Voice, &voice), Some(Media::Voice));
        assert_eq!(locate(Kind::Text, &Map::new()), None);
    }

    #[test]
    fn base64_reads_what_it_writes() {
        assert_eq!(base64("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64("aGVs\nbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("a*b"), None);
    }

    #[test]
    fn an_image_and_a_video_are_found_by_the_names_their_packed_info_holds() {
        let packed = bytes_field(3, &bytes_field(4, b"abc123"));
        let image = fields(Kind::Image, r#"<msg><img md5="m1" length="10"/></msg>"#, &packed);
        assert_eq!(Value::Object(image), json!({ "name": "abc123", "md5": "m1" }));
        let packed = bytes_field(4, &bytes_field(8, b"vid9"));
        let video =
            fields(Kind::Video, r#"<msg><videomsg playlength="12" md5="m2"/></msg>"#, &packed);
        assert_eq!(Value::Object(video), json!({ "name": "vid9", "seconds": 12, "md5": "m2" }));
        let bare = fields(Kind::Image, "not xml", &[]);
        assert_eq!(Value::Object(bare), json!({ "name": null, "md5": null }));
    }

    #[test]
    fn a_file_has_its_name_size_and_where_it_is_kept() {
        let xml = "<msg><appmsg><title>topo.pdf</title><type>6</type><appattach><totallen>2048</totallen><fileext>pdf</fileext></appattach></appmsg></msg>";
        assert_eq!(
            Value::Object(fields(Kind::File, xml, &[])),
            json!({ "name": "topo.pdf", "stored_as": "topo.pdf", "size": 2048, "extension": "pdf" })
        );
        let packed = bytes_field(7, &bytes_field(2, b"topo(1).pdf"));
        assert_eq!(fields(Kind::File, xml, &packed)["stored_as"], "topo(1).pdf");
    }

    #[test]
    fn a_voice_message_has_its_length_and_a_sticker_its_address() {
        let voice = fields(Kind::Voice, r#"<msg><voicemsg voicelength="3200"/></msg>"#, &[]);
        assert_eq!(Value::Object(voice), json!({ "length_ms": 3200, "transcription": null }));
        let sticker = fields(
            Kind::Sticker,
            r#"<msg><emoji md5="ABCD" cdnurl="https://cdn.example/s"/></msg>"#,
            &[],
        );
        assert_eq!(
            Value::Object(sticker),
            json!({ "md5": "abcd", "url": "https://cdn.example/s" })
        );
        // An app message's sticker: its address in emojiinfo.
        let info = bytes_field(2, b"https://cdn.example/t");
        let encoded = encode(&info);
        let app = format!(
            "<msg><appmsg><type>8</type><appattach><emoticonmd5>ef01</emoticonmd5><emojiinfo>{encoded}</emojiinfo></appattach></appmsg></msg>"
        );
        assert_eq!(
            Value::Object(fields(Kind::Sticker, &app, &[])),
            json!({ "md5": "ef01", "url": "https://cdn.example/t" })
        );
    }

    #[test]
    fn a_voice_message_has_the_text_wechat_heard_in_it_once_the_account_converted_it() {
        let xml = r#"<msg><voicemsg voicelength="1200"/></msg>"#;
        let state = |n: u8| [0x08, n];
        let mut heard = state(2).to_vec();
        heard.extend(bytes_field(2, " 周六见 ".as_bytes()));
        assert_eq!(fields(Kind::Voice, xml, &bytes_field(5, &heard))["transcription"], "周六见");
        // Not converted, converting, converted to nothing, or not
        // protobuf: none.
        for packed in [
            Vec::new(),
            bytes_field(5, &state(0)),
            bytes_field(5, &bytes_field(2, b"  ")),
            vec![0xff; 4],
        ] {
            assert_eq!(fields(Kind::Voice, xml, &packed)["transcription"], Value::Null);
        }
    }

    fn encode(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n =
                chunk.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
            for i in 0..=chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }
}
