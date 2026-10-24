//! `wxread file get` and `wxread avatar`: each media kind written at
//! `--out` as the file it is, an image decrypted under the key derived from
//! the synthetic kvcomm files, what WeChat does not hold answered as
//! missing, and nothing decrypted left anywhere else.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use wxread_media::image::{Format, ImageKey, seal};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Said, Voice, avatar_of, field};

const GROUP: &str = "123@chatroom";
const T: i64 = 1_760_000_000;
/// The kvcomm code the account's images are sealed under, beside one that
/// opens none.
const CODE: u32 = 4242;

fn key() -> ImageKey {
    ImageKey::derive(CODE, "wxid_me")
}

/// A JPEG of its own, `n` telling it apart.
fn jpeg(n: u8, len: usize) -> Vec<u8> {
    let mut image = vec![0xff, 0xd8, 0xff, 0xe0];
    image.extend((0..len).map(|i| (i as u8).wrapping_mul(n)));
    image
}

fn png(n: u8) -> Vec<u8> {
    [&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a][..], &[n; 300]].concat()
}

const MP4: &[u8] = b"\0\0\0\x18ftypmp42\0\0\0\0mp42isomthe video";
const SILK: &[u8] = b"\x02#!SILK_V3 the sound";
const PDF: &[u8] = b"%PDF-1.7 topo";
const STICKER: &[u8] = b"GIF89a the sticker";

/// Serves `STICKER` at `/s`, other bytes at `/bad`, and nothing else,
/// as WeChat's CDN would, for as long as the tests run; its address.
fn cdn() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap_or_default();
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                    break;
                }
            }
            let (status, body): (&str, &[u8]) = match path.as_str() {
                "/s" => ("200 OK", STICKER),
                "/bad" => ("200 OK", b"not the sticker"),
                _ => ("404 Not Found", b""),
            };
            let mut stream = &stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    at
}

/// A message of the group, Bob's, of `local_type`, with `packed`.
fn said(id: i64, local_type: i64, content: &str, packed: Vec<u8>) -> Said {
    let mut said = Said::text(GROUP, id, T + id % 100, "wxid_bob", content);
    said.local_type = local_type;
    said.content = format!("wxid_bob:\n{content}");
    said.packed = packed;
    said
}

fn image(id: i64, name: &str) -> Said {
    said(id, 3, r#"<msg><img md5="m"/></msg>"#, field(3, &field(4, name.as_bytes())))
}

fn sticker(id: i64, url: &str, md5: &str) -> Said {
    said(id, 47, &format!(r#"<msg><emoji md5="{md5}" cdnurl="{url}"/></msg>"#), Vec::new())
}

/// An account whose group holds a message of each media kind, its files
/// where WeChat keeps them, some of them missing.
fn account(world: &World) -> Account {
    let cdn = cdn();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", ""),
        Contact("wxid_bob", "Bob", ""),
        Contact(GROUP, "Hiking", ""),
    ];
    account.avatars = vec!["wxid_amy"];
    account.kvcomm = vec![111, CODE];
    let sticker_md5 = format!("{:x}", md5::compute(STICKER));
    account.messages = vec![
        image(9101, "img1"),
        image(9102, "img2"),
        image(9103, "img3"),
        image(9104, "img4"),
        image(9105, "img5"),
        said(9106, 43, r#"<msg><videomsg playlength="3"/></msg>"#, field(4, &field(8, b"vid1"))),
        said(
            9107,
            49 | 6 << 32,
            "<msg><appmsg><title>topo.pdf</title><type>6</type><appattach><totallen>13</totallen></appattach></appmsg></msg>",
            field(7, &field(2, b"topo(1).pdf")),
        ),
        sticker(9108, &format!("{cdn}/s"), &sticker_md5.to_uppercase()),
        sticker(9109, &format!("{cdn}/gone"), &sticker_md5),
        sticker(9110, &format!("{cdn}/bad"), &sticker_md5),
        said(9111, 34, r#"<msg><voicemsg voicelength="900"/></msg>"#, Vec::new()),
        said(9112, 34, r#"<msg><voicemsg voicelength="900"/></msg>"#, Vec::new()),
        Said::text(GROUP, 9113, T, "wxid_bob", "wxid_bob:\nhello"),
    ];
    account.voices = vec![Voice(GROUP, 9111, SILK.to_vec())];
    let img = |month: &str, name: &str| {
        format!("msg/attach/{}/{month}/Img/{name}", wxread_media::chat_dir(GROUP))
    };
    account.files = vec![
        (img("2025-10", "img1_h.dat"), seal(&jpeg(1, 3000), Format::V2(key()))),
        (img("2025-10", "img1_t.dat"), seal(&png(1), Format::V2(key()))),
        (img("2025-09", "img2.dat"), seal(&jpeg(2, 500), Format::Xor(0x5a))),
        (img("2025-10", "img3.dat"), seal(&jpeg(3, 1500), Format::V1 { xor: 0 })),
        (img("2025-10", "img4_t.dat"), seal(&png(4), Format::V2(key()))),
        ("msg/video/2025-10/vid1.mp4".into(), MP4.to_vec()),
        ("msg/video/2025-10/vid1_thumb.jpg".into(), jpeg(6, 100)),
        // Two of its name: the one of the size its message says is it.
        ("msg/file/2025-10/topo(1).pdf".into(), b"%PDF other".to_vec()),
        ("msg/file/2025-09/topo(1).pdf".into(), PDF.to_vec()),
    ];
    account.write();
    account
}

/// What a written file is answered as.
fn written(path: &Path, bytes: &[u8], content_type: &str) -> Value {
    json!({
        "path": path.display().to_string(),
        "size": bytes.len(),
        "sha256": format!("{:x}", Sha256::digest(bytes)),
        "content_type": content_type,
    })
}

struct Run<'a> {
    world: &'a World,
    keys: String,
}

impl<'a> Run<'a> {
    fn new(world: &'a World, account: &Account) -> Run<'a> {
        Run { world, keys: account.all_keys() }
    }

    fn get(&self, id: &str, out: &Path, extra: &[&str]) -> crate::common::Answer {
        let mut args = vec!["file", "get", id, "--out", out.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.world.run(&args, &[("WXREAD_KEYS", &self.keys)])
    }
}

fn out(world: &World, name: &str) -> PathBuf {
    world.root.join(name)
}

#[test]
fn each_media_kind_is_written_byte_for_byte_as_the_file_it_is() {
    let world = World::new();
    let account = account(&world);
    let run = Run::new(&world, &account);
    for (id, thumbnail, bytes, content_type) in [
        ("9101", false, jpeg(1, 3000), "image/jpeg"),
        ("9101", true, png(1), "image/png"),
        ("9102", false, jpeg(2, 500), "image/jpeg"),
        ("9103", false, jpeg(3, 1500), "image/jpeg"),
        ("9106", false, MP4.to_vec(), "video/mp4"),
        ("9106", true, jpeg(6, 100), "image/jpeg"),
        ("9107", false, PDF.to_vec(), "application/pdf"),
        ("9108", false, STICKER.to_vec(), "image/gif"),
        ("9111", false, SILK.to_vec(), "audio/silk"),
    ] {
        let path = out(&world, &format!("{id}-{thumbnail}"));
        let extra: &[&str] = if thumbnail { &["--thumbnail"] } else { &[] };
        let json = run.get(id, &path, extra).ok();
        assert_eq!(json, written(&path, &bytes, content_type), "{id} {thumbnail}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "{id} {thumbnail}");
    }
}

#[test]
fn a_file_wechat_does_not_hold_is_missing_with_its_thumbnail_and_how_to_fetch_it() {
    let world = World::new();
    let account = account(&world);
    let run = Run::new(&world, &account);
    let path = out(&world, "img4.jpg");
    let json = run.get("9104", &path, &[]).ok();
    assert_eq!(json["missing"], true);
    assert_eq!(json["thumbnail"], written(&path, &png(4), "image/png"));
    let fix = json["fix"].as_str().unwrap();
    assert!(fix.contains("open message 9104 in WeChat") && fix.contains(GROUP), "{fix}");
    assert_eq!(std::fs::read(&path).unwrap(), png(4));
    for (id, extra) in [("9105", &[][..]), ("9105", &["--thumbnail"][..]), ("9112", &[][..])] {
        let path = out(&world, &format!("{id}-{}", extra.len()));
        let json = run.get(id, &path, extra).ok();
        assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &Value::Null), "{id}");
        assert!(json["fix"].as_str().unwrap().contains("in WeChat"), "{json}");
        assert!(!path.exists(), "nothing is written for {id}");
    }
    let json = run.get("9109", &out(&world, "gone"), &[]).ok();
    assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &Value::Null));
    assert!(json["fix"].as_str().unwrap().contains("no longer serves"), "{json}");
}

#[test]
fn no_decrypted_image_is_left_anywhere_but_out() {
    let world = World::new();
    let account = account(&world);
    let run = Run::new(&world, &account);
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let outs = world.root.join("outs");
    std::fs::create_dir(&outs).unwrap();
    let before = snapshot(&world.home);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    for (id, extra) in
        [("9101", &[][..]), ("9101", &["--thumbnail"][..]), ("9104", &[]), ("9108", &[])]
    {
        let path = outs.join(format!("{id}-{}", extra.len()));
        let mut args = vec!["file", "get", id, "--out", path.to_str().unwrap()];
        args.extend_from_slice(extra);
        run.world.run(&args, &env).ok();
    }
    assert_eq!(snapshot(&world.home), before, "WeChat's files are as they were");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
    let mut written: Vec<String> = std::fs::read_dir(&outs)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    written.sort();
    assert_eq!(written, ["9101-0", "9101-1", "9104-0", "9108-0"]);
}

#[test]
fn writing_the_same_file_again_leaves_the_same_bytes() {
    let world = World::new();
    let account = account(&world);
    let run = Run::new(&world, &account);
    let path = out(&world, "again.jpg");
    let first = run.get("9101", &path, &[]).ok();
    let when = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(run.get("9101", &path, &[]).ok(), first);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), when, "left as it was");
    std::fs::write(&path, b"stale").unwrap();
    assert_eq!(run.get("9101", &path, &[]).ok(), first);
    assert_eq!(std::fs::read(&path).unwrap(), jpeg(1, 3000));
}

#[test]
fn what_cannot_be_written_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let run = Run::new(&world, &account);
    let at = out(&world, "x");
    let refused = |id: &str, extra: &[&str]| run.get(id, &at, extra).error();
    assert!(refused("9113", &[]).contains("is a text, which carries no file"));
    assert!(refused("9107", &["--thumbnail"]).contains("leave --thumbnail out"));
    assert!(refused("9999", &[]).contains("has no message 9999"));
    assert!(refused("abc", &[]).contains("abc is not a message's id"));
    let bad = refused("9110", &[]);
    assert!(bad.contains("is not sticker 9110") && bad.contains("Run this again later"), "{bad}");
    assert!(!at.exists(), "nothing was written");
    let error = run.get("9101", &world.root, &[]).error();
    assert!(error.contains("is a directory"), "{error}");
    let error = run.get("9101", &world.root.join("nowhere/x.jpg"), &[]).error();
    assert!(error.contains("create the directory it is in"), "{error}");
    let error =
        world.run(&["file", "get", "9101"], &[("WXREAD_KEYS", &account.all_keys())]).error();
    assert!(error.contains("pass --out <path>"), "{error}");
}

#[test]
fn an_image_no_kvcomm_key_opens_says_to_log_in_to_wechat_here() {
    let world = World::new();
    let mut account = account(&world);
    account.kvcomm = vec![];
    std::fs::remove_dir_all(account.documents().join("app_data")).unwrap();
    let run = Run::new(&world, &account);
    let error = run.get("9101", &out(&world, "x"), &[]).error();
    assert!(
        error.contains("kvcomm") && error.contains("log in to WeChat for Mac on this Mac"),
        "{error}"
    );
    // The older formats need no key.
    run.get("9102", &out(&world, "y"), &[]).ok();
}

#[test]
fn messages_says_whether_each_media_file_and_thumbnail_is_held() {
    let world = World::new();
    let account = account(&world);
    let json =
        world.run(&["messages", "--chat", GROUP], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let by_id = |id: &str| {
        let m = json["messages"].as_array().unwrap().iter().find(|m| m["id"] == id).unwrap();
        let kind = m["kind"].as_str().unwrap();
        (m[kind]["file_present"].clone(), m[kind]["thumbnail_present"].clone())
    };
    for (id, file, thumbnail) in [
        ("9101", json!(true), json!(true)),
        ("9104", json!(false), json!(true)),
        ("9105", json!(false), json!(false)),
        ("9106", json!(true), json!(true)),
        ("9107", json!(true), Value::Null),
        ("9108", json!(true), Value::Null),
        ("9111", json!(true), Value::Null),
        ("9112", json!(false), Value::Null),
        ("9113", Value::Null, Value::Null),
    ] {
        assert_eq!(by_id(id), (file, thumbnail), "{id}");
    }
}

#[test]
fn avatar_writes_the_image_wechat_holds_or_answers_it_missing() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let path = out(&world, "amy.jpg");
    let json = world.run(&["avatar", "wxid_amy", "--out", path.to_str().unwrap()], &env).ok();
    assert_eq!(json, written(&path, &avatar_of("wxid_amy"), "image/jpeg"));
    assert_eq!(std::fs::read(&path).unwrap(), avatar_of("wxid_amy"));
    let bob = out(&world, "bob.jpg");
    let json = world.run(&["avatar", "wxid_bob", "--out", bob.to_str().unwrap()], &env).ok();
    assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &Value::Null));
    assert!(json["fix"].as_str().unwrap().contains("open their profile in WeChat"), "{json}");
    assert!(!bob.exists());
    let error = world.run(&["avatar", "wxid_nobody", "--out", bob.to_str().unwrap()], &env).error();
    assert!(
        error.contains("no contact wxid_nobody") && error.contains("wxread contacts"),
        "{error}"
    );
    let error = world.run(&["avatar", "wxid_amy"], &env).error();
    assert!(error.contains("pass --out <path>"), "{error}");
}

/// A note of the group, Bob's, holding its body, a paragraph, an image, a
/// file and a video, its items' files under its directory `rec`.
fn note(id: i64, rec: &str) -> Said {
    let record = r#"<recordinfo><datalist count="5"><dataitem htmlid="WeNoteHtmlFile" datatype="8" dataid="body1"><datafmt>.htm</datafmt><datasize>12</datasize></dataitem><dataitem datatype="1" dataid="para1"><datadesc>Meet at the gate</datadesc></dataitem><dataitem datatype="2" dataid="img1"><datafmt>jpg</datafmt><datasize>3004</datasize></dataitem><dataitem datatype="8" dataid="gpx1"><datatitle>route.gpx</datatitle><datafmt>gpx</datafmt><datasize>9</datasize></dataitem><dataitem datatype="4" dataid="vid1"><datafmt>mp4</datafmt></dataitem></datalist></recordinfo>"#;
    let escaped = record.replace('<', "&lt;").replace('>', "&gt;");
    let content = format!(
        "<msg><appmsg><title /><des>Meet at the gate</des><type>24</type><recorditem>{escaped}</recorditem></appmsg></msg>"
    );
    said(id, 49 | 24 << 32, &content, field(9, &field(1, rec.as_bytes())))
}

/// The account, with two notes in its group: one whose files WeChat holds,
/// one of whose image it holds only the thumbnail.
fn with_notes(world: &World) -> Account {
    let mut account = account(world);
    account.messages.extend([note(9120, "aaaa1111bbbb2222"), note(9121, "cccc3333dddd4444")]);
    let rec = |dir: &str, path: &str| {
        format!("msg/attach/{}/2025-10/Rec/{dir}/{path}", wxread_media::chat_dir(GROUP))
    };
    account.files.extend([
        (rec("aaaa1111bbbb2222", "Dat/0/0"), b"<p>Meet</p>".to_vec()),
        (rec("aaaa1111bbbb2222", "Img/2"), seal(&jpeg(7, 3000), Format::V2(key()))),
        (rec("aaaa1111bbbb2222", "Img/2_t"), seal(&png(7), Format::V2(key()))),
        (rec("aaaa1111bbbb2222", "F/3/route.gpx"), b"<gpx/>gpx".to_vec()),
        (rec("cccc3333dddd4444", "Img/2_t"), seal(&png(8), Format::V2(key()))),
    ]);
    account.write();
    account
}

#[test]
fn a_notes_items_are_answered_with_their_files_ids_and_whether_wechat_holds_them() {
    let world = World::new();
    let account = with_notes(&world);
    let json =
        world.run(&["messages", "--chat", GROUP], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let note = |id: &str| {
        let m = json["messages"].as_array().unwrap().iter().find(|m| m["id"] == id).unwrap();
        assert_eq!(m["kind"], "note", "{m}");
        m["note"]["items"].clone()
    };
    let items = note("9120");
    let kinds: Vec<&str> =
        items.as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["file", "text", "image", "file", "video"]);
    assert_eq!(
        items[0]["file"],
        json!({ "file_id": "note:9120:body1", "size": 12, "name": null, "extension": "htm", "file_present": true, "thumbnail_present": null })
    );
    assert_eq!(items[1]["text"], json!({ "text": "Meet at the gate" }));
    assert_eq!(
        items[2]["image"],
        json!({ "file_id": "note:9120:img1", "size": 3004, "file_present": true, "thumbnail_present": true })
    );
    assert_eq!(
        (&items[3]["file"]["file_id"], &items[3]["file"]["file_present"]),
        (&json!("note:9120:gpx1"), &json!(true))
    );
    // Where WeChat keeps a note's video is not known: nothing names it.
    assert_eq!(
        items[4]["video"],
        json!({ "file_id": null, "size": null, "seconds": null, "file_present": null, "thumbnail_present": null })
    );
    let other = note("9121");
    assert_eq!(
        (&other[2]["image"]["file_present"], &other[2]["image"]["thumbnail_present"]),
        (&json!(false), &json!(true))
    );
    assert_eq!(other[0]["file"]["file_present"], false);
}

#[test]
fn a_notes_item_is_written_by_its_file_id_or_answered_missing() {
    let world = World::new();
    let account = with_notes(&world);
    let run = Run::new(&world, &account);
    for (id, thumbnail, bytes, content_type) in [
        ("note:9120:body1", false, b"<p>Meet</p>".to_vec(), "application/octet-stream"),
        ("note:9120:img1", false, jpeg(7, 3000), "image/jpeg"),
        ("note:9120:img1", true, png(7), "image/png"),
        ("note:9120:gpx1", false, b"<gpx/>gpx".to_vec(), "application/octet-stream"),
    ] {
        let path = out(&world, &format!("{}-{thumbnail}", id.replace(':', "-")));
        let extra: &[&str] = if thumbnail { &["--thumbnail"] } else { &[] };
        assert_eq!(run.get(id, &path, extra).ok(), written(&path, &bytes, content_type), "{id}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "{id}");
    }
    // Missing: its thumbnail in its place, and the message to open.
    let path = out(&world, "missing.jpg");
    let json = run.get("note:9121:img1", &path, &[]).ok();
    assert_eq!(
        (&json["missing"], &json["thumbnail"]),
        (&json!(true), &written(&path, &png(8), "image/png"))
    );
    let fix = json["fix"].as_str().unwrap();
    assert!(fix.contains("open message 9121 in WeChat") && fix.contains(GROUP), "{fix}");
    let json = run.get("note:9121:body1", &out(&world, "body"), &[]).ok();
    assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &Value::Null));
    // What is not a note's item's file is refused, saying what to pass.
    for (id, said) in [
        ("note:9120:para1", "is a text of the note sent as message 9120"),
        ("note:9120:vid1", "where WeChat for Mac keeps a note's video"),
        ("note:9120:nope", "has no note sent as message 9120 holding item nope"),
        ("note:9999:img1", "has no note sent as message 9999"),
        ("note:9113:img1", "has no note sent as message 9113"),
        ("note:91x:img1", "is not a note's item's file id"),
        ("9120", "message 9120 is a note, whose items carry its files"),
    ] {
        let error = run.get(id, &out(&world, "refused"), &[]).error();
        assert!(error.contains(said), "{id}: {error}");
    }
    let error = run.get("note:9120:gpx1", &out(&world, "t"), &["--thumbnail"]).error();
    assert!(error.contains("leave --thumbnail out"), "{error}");
    assert!(!out(&world, "refused").exists() && !out(&world, "t").exists());
}
