//! What `wxread file get` and `wxread avatar` make of the media WeChat
//! for Mac 4.x keeps: where each kind's file lies under the account's
//! directory (the one above `db_storage`), how an image is decrypted
//! ([`image`]), what a file's first bytes say it is, and what an agent is
//! told when WeChat does not hold one. The effect crate reads the files
//! and writes the one asked for; nothing here does. Where each kind lies,
//! as bizkernel's wechat-intake found it:
//!
//! - an image: `msg/attach/<md5 of the chat>/<YYYY-MM>/Img/<name>.dat`
//!   (`<name>_h.dat` the full one when there is one, `<name>_t.dat` its
//!   thumbnail), sealed ([`image`]);
//! - a video: `msg/video/<YYYY-MM>/<name>.mp4`, its thumbnail
//!   `<name>_thumb.jpg`;
//! - a file: `msg/file/<YYYY-MM>/<name>`, the one of its size when several
//!   share its name;
//! - a voice message: its SILK bytes in `message/media_<n>.db`'s
//!   `VoiceInfo`, by the message's server id;
//! - a sticker: on WeChat's CDN, at its address, checked against its MD5;
//! - an item of a note sent into a chat: under the note's own directory,
//!   `msg/attach/<md5 of the chat>/<YYYY-MM>/Rec/<dir>`, `<dir>` in the
//!   message's `packed_info_data` (field 9, its field 1), by its place in
//!   the note: an image `Img/<place>` (sealed), its thumbnail
//!   `Img/<place>_t`; a file `F/<place>/<its name>`, the note's own body
//!   `Dat/<place>/<its name, else its place>`, neither sealed. Found on
//!   WeChat for Mac 4.1, every file of the 21 notes of one account named so;
//! - a Moment's image: `cache/<YYYY-MM>/Sns/Img/<k[..2]>/<k[2..]>`, no
//!   extension, sealed ([`image`]), `k` the MD5 in lowercase hex of
//!   `<post id>_<media id>_<n>`, `n` [`MOMENT_IMAGE`] (its thumbnail one
//!   of [`MOMENT_IMAGE_THUMBNAILS`], the smaller copies),
//!   the ids as `sns/sns.db`'s XML says them (the post's `<id>`, which is
//!   the row's `tid` read unsigned; the media's `<id>`) and the month the
//!   one it was posted in; a Moment's video is
//!   `cache/<YYYY-MM>/Sns/Video/<k[..2]>/<k[2..]>.mp4` by [`MOMENT_VIDEO`], its
//!   thumbnail the `.jpg` beside it. Found on WeChat for Mac 4.1.5, as
//!   docs/moment-cache.md records with the script that counts it again.

pub mod image;

/// The option that names where the file is written, and the one that asks
/// for the thumbnail.
pub const OUT_FLAG: &str = "--out";
pub const THUMBNAIL_FLAG: &str = "--thumbnail";

/// The copies WeChat caches of a Moment's media, each by the number that
/// ends the name it is cached under (`<post id>_<media id>_<n>`): an
/// image's own, its smaller ones in the order taken for its thumbnail, and
/// a video's (its thumbnail the `.jpg` beside it).
pub const MOMENT_IMAGE: u8 = 2;
pub const MOMENT_IMAGE_THUMBNAILS: [u8; 2] = [1, 6];
pub const MOMENT_VIDEO: u8 = 3;

/// A media message's file: what finds it, each part None when the message
/// does not say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Media {
    Image {
        name: Option<String>,
    },
    Video {
        name: Option<String>,
    },
    /// `name` the one it is stored as, `size` its bytes when the message
    /// says.
    File {
        name: Option<String>,
        size: Option<u64>,
    },
    Sticker {
        md5: Option<String>,
        url: Option<String>,
    },
    Voice,
    /// An image of a note sent into a chat, by the note's directory
    /// (`Rec/<rec>`) and the image's place in the note.
    NoteImage {
        rec: Option<String>,
        place: usize,
    },
    /// A file of a note sent into a chat, the note's own body among them,
    /// by the note's directory, the file's place in the note and its name.
    NoteFile {
        rec: Option<String>,
        place: usize,
        name: Option<String>,
    },
    /// A Moment's image, by the ids of its post and of its media, as
    /// `sns/sns.db` says them.
    MomentImage {
        post: u64,
        media: u64,
    },
    /// A Moment's video, by the ids of its post and of its media, as
    /// `sns/sns.db` says them.
    MomentVideo {
        post: u64,
        media: u64,
    },
}

impl Media {
    /// Its kind, as `wxread messages` names it.
    pub fn kind(&self) -> &'static str {
        match self {
            Media::Image { .. } | Media::NoteImage { .. } | Media::MomentImage { .. } => "image",
            Media::Video { .. } | Media::MomentVideo { .. } => "video",
            Media::File { .. } | Media::NoteFile { .. } => "file",
            Media::Sticker { .. } => "sticker",
            Media::Voice => "voice",
        }
    }

    /// Whether WeChat keeps a thumbnail of its kind: an image's and a
    /// video's, a message's, a note's or a Moment's.
    pub fn has_thumbnail(&self) -> bool {
        matches!(
            self,
            Media::Image { .. }
                | Media::NoteImage { .. }
                | Media::Video { .. }
                | Media::MomentImage { .. }
                | Media::MomentVideo { .. }
        )
    }

    /// The directory under the account's whose months (`YYYY-MM`) hold its
    /// file, for a message of the chat whose [`chat_dir`] is `chat_dir`
    /// (a Moment's has none): None for a voice message and a sticker,
    /// which are not kept in one.
    pub fn shelf(&self, chat_dir: &str) -> Option<String> {
        match self {
            Media::Image { .. } | Media::NoteImage { .. } | Media::NoteFile { .. } => {
                Some(format!("msg/attach/{chat_dir}"))
            }
            Media::Video { .. } => Some("msg/video".into()),
            Media::File { .. } => Some("msg/file".into()),
            Media::MomentImage { .. } | Media::MomentVideo { .. } => Some("cache".into()),
            Media::Sticker { .. } | Media::Voice => None,
        }
    }

    /// The paths in a month of its shelf that are its file, or with
    /// `thumbnail` its thumbnail, the one to take first first: none when
    /// the message names none.
    pub fn names(&self, thumbnail: bool) -> Vec<String> {
        match (self, thumbnail) {
            (Media::Image { name: Some(n) }, false) => {
                vec![format!("Img/{n}_h.dat"), format!("Img/{n}.dat")]
            }
            (Media::Image { name: Some(n) }, true) => vec![format!("Img/{n}_t.dat")],
            (Media::Video { name: Some(n) }, false) => vec![format!("{n}.mp4")],
            (Media::Video { name: Some(n) }, true) => vec![format!("{n}_thumb.jpg")],
            (Media::File { name: Some(n), .. }, false) if is_one_name(n) => vec![n.clone()],
            (Media::NoteImage { rec: Some(r), place }, thumbnail) if is_one_name(r) => {
                let t = if thumbnail { "_t" } else { "" };
                vec![format!("Rec/{r}/Img/{place}{t}")]
            }
            (Media::NoteFile { rec: Some(r), place, name }, false) if is_one_name(r) => {
                let n = name.clone().unwrap_or_else(|| place.to_string());
                if !is_one_name(&n) {
                    return Vec::new();
                }
                vec![format!("Rec/{r}/F/{place}/{n}"), format!("Rec/{r}/Dat/{place}/{n}")]
            }
            (Media::MomentImage { post, media }, false) => {
                vec![moment("Img", *post, *media, MOMENT_IMAGE, "")]
            }
            (Media::MomentImage { post, media }, true) => MOMENT_IMAGE_THUMBNAILS
                .iter()
                .map(|&n| moment("Img", *post, *media, n, ""))
                .collect(),
            (Media::MomentVideo { post, media }, false) => {
                vec![moment("Video", *post, *media, MOMENT_VIDEO, ".mp4")]
            }
            (Media::MomentVideo { post, media }, true) => {
                vec![moment("Video", *post, *media, MOMENT_VIDEO, ".jpg")]
            }
            _ => Vec::new(),
        }
    }

    /// Whether the bytes found, its file's or its thumbnail's, are sealed
    /// as an image is ([`image`]), so are decrypted before they are
    /// written.
    pub fn sealed(&self) -> bool {
        matches!(self, Media::Image { .. } | Media::NoteImage { .. } | Media::MomentImage { .. })
    }

    /// Of the files found, each with its size, the one to take: a file's of
    /// the size the message says, else the first.
    pub fn pick<'a, P>(&self, found: &'a [(P, u64)]) -> Option<&'a P> {
        let size = match self {
            Media::File { size, .. } => *size,
            _ => None,
        };
        found
            .iter()
            .find(|(_, s)| size.is_some_and(|want| *s == want))
            .or(found.first())
            .map(|(p, _)| p)
    }
}

/// Whether WeChat holds a media message's file, and its thumbnail when
/// its kind has one (None when it has not): what `wxread messages` shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub file: bool,
    pub thumbnail: Option<bool>,
}

/// The path in a month of `cache`, under `Sns/<shelf>` and ending `ext`,
/// of the copy `copy` (one of [`MOMENT_IMAGE`] and the others beside it)
/// WeChat caches of media `media` of Moment `post`.
fn moment(shelf: &str, post: u64, media: u64, copy: u8, ext: &str) -> String {
    let k = format!("{:x}", md5::compute(format!("{post}_{media}_{copy}")));
    let (dir, name) = k.split_at(2);
    format!("Sns/{shelf}/{dir}/{name}{ext}")
}

/// Whether `name` is one name in a directory, not a path out of it.
fn is_one_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// The directory WeChat keeps `chat`'s images under: the MD5 of its id,
/// in lowercase hex, as its messages' table is named.
pub fn chat_dir(chat: &str) -> String {
    format!("{:x}", md5::compute(chat.as_bytes()))
}

/// Whether a sticker's bytes are the ones whose MD5 `md5` its message
/// names.
pub fn is_sticker(bytes: &[u8], md5: &str) -> bool {
    format!("{:x}", md5::compute(bytes)).eq_ignore_ascii_case(md5)
}

/// What a file is, from its first bytes: a content type,
/// application/octet-stream for what is not known.
pub fn content_type(head: &[u8]) -> &'static str {
    let at = |from: usize, magic: &[u8]| head.get(from..).is_some_and(|h| h.starts_with(magic));
    if at(0, &[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if at(0, &[0x89, 0x50, 0x4e, 0x47]) {
        "image/png"
    } else if at(0, b"GIF8") {
        "image/gif"
    } else if at(0, b"wxgf") {
        // WeChat's own image format, named as such: few viewers read it.
        "image/wxgf"
    } else if at(0, b"RIFF") && at(8, b"WEBP") {
        "image/webp"
    } else if at(4, b"ftypqt") {
        "video/quicktime"
    } else if at(4, b"ftyp") {
        "video/mp4"
    } else if at(0, b"#!SILK") || at(0, b"\x02#!SILK") {
        "audio/silk"
    } else if at(0, b"#!AMR") {
        "audio/amr"
    } else if at(0, b"%PDF") {
        "application/pdf"
    } else if at(0, b"PK\x03\x04") {
        "application/zip"
    } else {
        "application/octet-stream"
    }
}

/// What makes WeChat fetch a `kind`'s file it does not hold, the message
/// `id` of `chat` sent at `at`; `thumbnail` whether the answer holds the
/// thumbnail in its place.
pub fn fetch_fix(kind: &str, id: &str, chat: &str, at: &str, thumbnail: bool) -> String {
    let instead = if thumbnail {
        format!("; until then {OUT_FLAG} holds its thumbnail, which WeChat does hold")
    } else {
        String::new()
    };
    format!(
        "WeChat for Mac on this Mac does not hold this {kind}'s file (it never downloaded it, or cleared it): open message {id} in WeChat (chat {chat}, sent {at}) and let it download, then run this again{instead}"
    )
}

/// What makes WeChat fetch a thumbnail of message `id` it does not hold.
pub fn thumbnail_fix(kind: &str, id: &str, chat: &str, at: &str) -> String {
    format!(
        "WeChat for Mac on this Mac holds no thumbnail of this {kind}: open message {id} in WeChat (chat {chat}, sent {at}), which fetches it, then run this again; or leave {THUMBNAIL_FLAG} out for the file itself"
    )
}

/// What an agent is told of a sticker whose message names no address, or
/// whose address WeChat's CDN no longer serves.
pub fn sticker_fix(id: &str, served: bool) -> String {
    let why = if served {
        "WeChat's CDN no longer serves this sticker at the address its message names"
    } else {
        "this sticker's message names no address on WeChat's CDN"
    };
    format!(
        "{why}, and the copy WeChat caches is sealed under a key nobody has read: open message {id} in WeChat and save the sticker from there"
    )
}

/// What makes WeChat load a `kind` of a Moment it does not hold: media
/// `id` of the Moment `author` (as WeChat shows them) posted at `at`;
/// `thumbnail` whether the answer holds the thumbnail in its place.
pub fn moment_fix(kind: &str, id: &str, author: &str, at: &str, thumbnail: bool) -> String {
    let instead = if thumbnail {
        format!("; until then {OUT_FLAG} holds its thumbnail, which WeChat does hold")
    } else {
        String::new()
    };
    format!(
        "WeChat for Mac on this Mac does not hold this {kind} of a Moment (it keeps only what it loaded while Moments were viewed, and clears that): open {author}'s Moment of {at} in WeChat's Moments and let the {kind} load, then run this again for {id}{instead}"
    )
}

/// What makes WeChat fetch an avatar of `contact` it does not hold.
pub fn avatar_fix(contact: &str) -> String {
    format!(
        "WeChat for Mac on this Mac holds no avatar of {contact}: open their profile in WeChat, which fetches it, then run this again"
    )
}

/// What fixes asking for the thumbnail of a kind WeChat keeps none of.
pub fn no_thumbnail(kind: &str, id: &str) -> String {
    format!(
        "message {id} is a {kind}, of which WeChat keeps no thumbnail (only images and videos have one): leave {THUMBNAIL_FLAG} out"
    )
}

/// What fixes naming a message that carries no file.
pub fn not_media(id: &str, kind: &str) -> String {
    format!(
        "message {id} is a {kind}, which carries no file: pass the id of an image, video, file, sticker or voice message, as `wxread messages` answers it"
    )
}

/// What fixes a message id that is not one.
pub fn not_an_id(id: &str) -> String {
    format!(
        "{id} is not a message's id: pass one as `wxread messages` answers it (its \"id\", WeChat's server id, digits), a note's item's file_id as it answers it under note.items (note:<message>:<item>), a favorite's file_id as `wxread favorites` answers it (fav:<favorite>:<item>), or a Moment's medium's as `wxread moments` answers it under media (<the Moment's id>:<its place>)"
    )
}

/// What fixes naming a message no message database holds.
pub fn no_such_message(id: &str, account: &str) -> String {
    format!(
        "account {account} has no message {id}: pass a message's id as `wxread messages` answers it (its \"id\", WeChat's server id)"
    )
}

/// What fixes an image no key derived from the kvcomm files opens.
pub fn no_image_key(account: &str, documents: &str) -> String {
    format!(
        "account {account}'s images are sealed under a key derived from WeChat's kvcomm files (key_<code>_….statistic, under {documents}/{}), and none derived from those there opens them: log in to WeChat for Mac on this Mac, which writes them, then run this again",
        image::KVCOMM_DIRS[0]
    )
}

/// What fixes a file `path` that did not decrypt for `refusal`, `source`
/// what holds it in WeChat: "the message", or a favorite.
pub fn image_refused(path: &str, refusal: image::Refusal, source: &str) -> String {
    let why = match refusal {
        image::Refusal::NotWechats => "is in none of the formats WeChat seals images in",
        image::Refusal::NoKey | image::Refusal::KeyDoesNotOpen => {
            "does not decrypt under the account's image key"
        }
        image::Refusal::Short => "is cut short: its header says more bytes than it holds",
    };
    format!(
        "{path} {why}: open {source} in WeChat to have it download it again; if it still fails, tell wxread's maintainers which WeChat version wrote it (https://github.com/sunfmin/wxread/issues)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_lies_in_its_shelf_under_its_names() {
        let image = Media::Image { name: Some("a1".into()) };
        assert_eq!(
            image.shelf(&chat_dir("1@chatroom")),
            Some(format!("msg/attach/{}", chat_dir("1@chatroom")))
        );
        assert_eq!(image.names(false), ["Img/a1_h.dat", "Img/a1.dat"]);
        assert_eq!(image.names(true), ["Img/a1_t.dat"]);
        let video = Media::Video { name: Some("v9".into()) };
        assert_eq!(
            (video.shelf("x"), video.names(true)),
            (Some("msg/video".into()), vec!["v9_thumb.jpg".into()])
        );
        let file = Media::File { name: Some("topo(1).pdf".into()), size: Some(3) };
        assert_eq!(
            (file.shelf("x"), file.names(false)),
            (Some("msg/file".into()), vec!["topo(1).pdf".into()])
        );
        assert!(file.names(true).is_empty() && !file.has_thumbnail());
        assert_eq!(Media::Voice.shelf("x"), None);
        assert!(Media::Image { name: None }.names(false).is_empty());
    }

    #[test]
    fn a_notes_item_lies_in_its_place_under_the_notes_directory() {
        let rec = || Some("58d452fe52a3cf23".to_string());
        let image = Media::NoteImage { rec: rec(), place: 3 };
        assert_eq!(
            image.shelf(&chat_dir("1@chatroom")),
            Some(format!("msg/attach/{}", chat_dir("1@chatroom")))
        );
        assert_eq!(image.names(false), ["Rec/58d452fe52a3cf23/Img/3"]);
        assert_eq!(image.names(true), ["Rec/58d452fe52a3cf23/Img/3_t"]);
        assert!(image.sealed() && image.has_thumbnail() && image.kind() == "image");
        let file = Media::NoteFile { rec: rec(), place: 4, name: Some("径山.gpx".into()) };
        assert_eq!(
            file.names(false),
            ["Rec/58d452fe52a3cf23/F/4/径山.gpx", "Rec/58d452fe52a3cf23/Dat/4/径山.gpx"]
        );
        // A note's body, which names no file of its own, by its place.
        let body = Media::NoteFile { rec: rec(), place: 0, name: None };
        assert_eq!(
            body.names(false),
            ["Rec/58d452fe52a3cf23/F/0/0", "Rec/58d452fe52a3cf23/Dat/0/0"]
        );
        assert!(!body.has_thumbnail() && body.names(true).is_empty() && body.kind() == "file");
        // Nothing found where the message names no directory, or one that
        // leads out of it.
        for rec in [None, Some("../x".to_string()), Some("..".to_string())] {
            assert!(Media::NoteImage { rec: rec.clone(), place: 1 }.names(false).is_empty());
            assert!(Media::NoteFile { rec, place: 1, name: None }.names(false).is_empty());
        }
        let out = Media::NoteFile { rec: rec(), place: 1, name: Some("../../db".into()) };
        assert!(out.names(false).is_empty());
    }

    #[test]
    fn a_moments_image_is_named_by_its_post_and_media_ids() {
        // md5("18000000000000000001_18000000000000000777_<n>"), n 2 the
        // image, 1 and 6 its smaller copies.
        let image = Media::MomentImage { post: 18000000000000000001, media: 18000000000000000777 };
        assert_eq!(image.shelf("ignored"), Some("cache".into()));
        assert_eq!(image.names(false), ["Sns/Img/cb/47aea569075262588e3e05eb6a268c"]);
        assert_eq!(
            image.names(true),
            [
                "Sns/Img/99/0defbbcd85aab7389447c3d08bec42",
                "Sns/Img/9f/38ac6f3323d5c8922c7c963d0a5f52"
            ]
        );
        assert!(image.sealed() && image.has_thumbnail());
        assert_eq!(image.kind(), "image");
    }

    #[test]
    fn a_moments_video_is_named_by_its_post_and_media_ids() {
        let video = Media::MomentVideo { post: 14720510141367799999, media: 14720510155579726 };
        let k = format!("{:x}", md5::compute("14720510141367799999_14720510155579726_3"));
        let (head, rest) = k.split_at(2);
        assert_eq!(video.shelf("ignored"), Some("cache".into()));
        assert_eq!(video.names(false), [format!("Sns/Video/{head}/{rest}.mp4")]);
        assert_eq!(video.names(true), [format!("Sns/Video/{head}/{rest}.jpg")]);
        assert!(video.has_thumbnail() && !video.sealed() && video.kind() == "video");
    }

    #[test]
    fn a_files_name_never_leads_out_of_its_month() {
        for name in ["../../db_storage/x", "a/b", "..", "", "."] {
            let file = Media::File { name: Some(name.into()), size: None };
            assert!(file.names(false).is_empty(), "{name}");
        }
    }

    #[test]
    fn a_file_of_the_size_its_message_says_is_taken_first() {
        let file = Media::File { name: Some("a".into()), size: Some(5) };
        assert_eq!(file.pick(&[("new", 9), ("old", 5)]), Some(&"old"));
        assert_eq!(file.pick(&[("new", 9)]), Some(&"new"));
        assert_eq!(Media::Video { name: None }.pick::<&str>(&[]), None);
    }

    #[test]
    fn a_files_first_bytes_name_its_type() {
        assert_eq!(content_type(&[0xff, 0xd8, 0xff, 0xe0]), "image/jpeg");
        assert_eq!(content_type(b"wxgf\x01"), "image/wxgf");
        assert_eq!(content_type(b"\0\0\0\x1cftypmp42"), "video/mp4");
        assert_eq!(content_type(b"\x02#!SILK_V3"), "audio/silk");
        assert_eq!(content_type(b"%PDF-1.7"), "application/pdf");
        assert_eq!(content_type(b"RIFF\0\0\0\0WEBPVP8"), "image/webp");
        assert_eq!(content_type(b""), "application/octet-stream");
    }

    #[test]
    fn a_sticker_is_checked_against_its_md5() {
        assert!(is_sticker(b"hi", "49F68A5C8493EC2C0BF489821C21FC3B"));
        assert!(!is_sticker(b"ho", "49f68a5c8493ec2c0bf489821c21fc3b"));
    }

    #[test]
    fn what_wechat_does_not_hold_says_how_to_make_it_fetch_it() {
        let fix = fetch_fix("image", "9015", "1@chatroom", "2025-10-09T08:53:35Z", true);
        assert!(
            fix.contains("open message 9015 in WeChat")
                && fix.contains("--out holds its thumbnail"),
            "{fix}"
        );
        assert!(!fetch_fix("video", "1", "c", "t", false).contains("thumbnail"));
        assert!(no_thumbnail("file", "7").contains("leave --thumbnail out"));
        let fix = moment_fix("video", "1:0", "Amy", "2025-10-09T08:53:35Z", true);
        assert!(
            fix.contains("open Amy's Moment of 2025-10-09T08:53:35Z")
                && fix.contains("--out holds its thumbnail"),
            "{fix}"
        );
    }
}
