//! `wxread favorites` and `wxread file get fav:…`: what the account saved,
//! each as its kind, read from WeChat's own favorites database, a page at
//! a time; and a favorite's file written at `--out`, unsealed under the key
//! derived from the synthetic kvcomm files, missing ones answered as
//! missing.

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use wxread_favorites::path;
use wxread_media::image::{Format, ImageKey, seal};
use wxread_schema::{CONTACT_DB, FAVORITE_DB};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Saved};

const GROUP: &str = "123@chatroom";
const T: i64 = 1_760_000_000;
const CODE: u32 = 4242;

fn key() -> ImageKey {
    ImageKey::derive(CODE, "wxid_me")
}

fn jpeg(n: u8) -> Vec<u8> {
    let mut image = vec![0xff, 0xd8, 0xff, 0xe0];
    image.extend((0..400).map(|i| (i as u8).wrapping_mul(n)));
    image
}

const NOTE: &[u8] = b"<p>Pack: rope, water</p>";
const PDF: &[u8] = b"%PDF-1.7 topo";

/// A favorite's `<favitem>` of `fav_type` holding `body`, saved from Amy's
/// message in the group, tagged `hike`.
fn favitem(fav_type: i64, body: &str) -> String {
    format!(
        "<favitem type=\"{fav_type}\">{body}<source sourcetype=\"1\"><fromusr>{GROUP}</fromusr><tousr>wxid_me</tousr><realchatname>wxid_amy</realchatname><createtime>{T}</createtime><msgid>9101</msgid></source><taglist><tag>hike</tag></taglist></favitem>"
    )
}

fn saved(id: i64, fav_type: i64, at: i64, body: &str) -> Saved {
    Saved {
        id,
        fav_type,
        saved: T + at,
        content: favitem(fav_type, body),
        from: GROUP,
        sender: "wxid_amy",
    }
}

fn item(datatype: i64, dataid: &str, body: &str) -> String {
    format!("<dataitem datatype=\"{datatype}\" dataid=\"{dataid}\">{body}</dataitem>")
}

fn list(items: &[String]) -> String {
    format!("<datalist count=\"{}\">{}</datalist>", items.len(), items.concat())
}

/// An account that saved one favorite of each kind, the newest only in
/// its favorites database's WAL; WeChat holds the image's file and
/// thumbnail, the note's body and the PDF, the video's thumbnail only, and
/// nothing of the voice message.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", "Auntie"),
        Contact(GROUP, "Hiking", ""),
    ];
    account.kvcomm = vec![111, CODE];
    let image = list(&[item(
        2,
        "img1",
        "<fullmd5>f1</fullmd5><thumbfullmd5>t1</thumbfullmd5><fullsize>404</fullsize>",
    )]);
    let video = list(&[item(
        4,
        "vid1",
        "<fullmd5>f2</fullmd5><thumbfullmd5>t2</thumbfullmd5><duration>21</duration><fullsize>9000</fullsize>",
    )]);
    let voice = list(&[item(3, "voi1", "<fullmd5>f3</fullmd5><duration>4500</duration>")]);
    let file = list(&[item(
        8,
        "pdf1",
        "<fullmd5>f4</fullmd5><datafmt>pdf</datafmt><datatitle>topo.pdf</datatitle><fullsize>13</fullsize>",
    )]);
    let history = format!(
        "<title>Trip chat</title><desc>Amy: Go</desc>{}",
        list(&[
            item(
                1,
                "h1",
                "<datadesc>Go at six</datadesc><datasrcname>Amy</datasrcname><srcMsgCreateTime>1760000000</srcMsgCreateTime><dataitemsource><realchatname>wxid_amy</realchatname></dataitemsource>"
            ),
            item(2, "h2", "<fullmd5>f5</fullmd5><datasrcname>Amy</datasrcname>"),
        ])
    );
    let note = list(&[
        item(8, "n1", "<datafmt>.htm</datafmt><fullmd5>f6</fullmd5>"),
        item(1, "n2", "<datadesc>Pack: rope, water</datadesc>"),
    ]);
    account.favorites = vec![
        saved(1, 1, 1, "<desc>Bring water</desc>"),
        saved(
            2,
            5,
            2,
            "<source><link>https://e.com/r</link></source><weburlitem><pagetitle>Routes</pagetitle><pagedesc>North face</pagedesc><pagethumb_url>https://e.com/r.jpg</pagethumb_url><appmsgshareitem><srcdisplayname>Crag News</srcdisplayname></appmsgshareitem></weburlitem>",
        ),
        saved(3, 2, 3, &image),
        saved(4, 4, 4, &video),
        saved(5, 8, 5, &file),
        saved(6, 3, 6, &voice),
        saved(
            7,
            6,
            7,
            "<locitem><lat>39.9</lat><lng>116.4</lng><poiname>Summit</poiname><label>Peak Rd 1</label></locitem>",
        ),
        saved(8, 14, 8, &history),
        saved(
            9,
            7,
            9,
            &list(&[item(
                7,
                "m1",
                "<datatitle>Song</datatitle><datadesc>Singer</datadesc><stream_weburl>https://e.com/s</stream_weburl>",
            )]),
        ),
        saved(
            10,
            19,
            10,
            "<title>A trail</title><appbranditem><appid>wx1</appid><sourcedisplayname>Trails</sourcedisplayname><pagepath>p/1</pagepath></appbranditem>",
        ),
        saved(
            11,
            20,
            11,
            "<title>Day one</title><finderFeed><nickname>Climber</nickname><desc>Day one</desc></finderFeed>",
        ),
        saved(12, 99, 12, "<desc>?</desc>"),
    ];
    account.recent_favorites = vec![saved(13, 18, 13, &note)];
    let sealed = |bytes: &[u8]| seal(bytes, Format::V2(key()));
    account.files = vec![
        (path("img1", Some("f1"), "data"), sealed(&jpeg(1))),
        (path("img1", Some("t1"), "thumb"), sealed(&jpeg(2))),
        (path("vid1", Some("t2"), "thumb"), sealed(&jpeg(3))),
        (path("n1", Some("f6"), "data"), sealed(NOTE)),
        (path("pdf1", Some("f4"), "data"), PDF.to_vec()),
    ];
    account.write();
    account
}

fn kinds(json: &Value) -> Vec<String> {
    let favorites = json["favorites"].as_array().unwrap();
    favorites.iter().map(|f| f["kind"].as_str().unwrap().to_string()).collect()
}

#[test]
fn it_lists_every_favorite_newest_first_each_as_its_kind() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["favorites"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(json["account"], json!("wxid_me_1a2b"));
    assert_eq!(json["next"], json!(null));
    assert_eq!(
        kinds(&json),
        [
            "note",
            "unknown",
            "channels",
            "mini_program",
            "music",
            "chat_history",
            "location",
            "voice",
            "file",
            "video",
            "image",
            "link",
            "text",
        ]
    );
    let by_id = |id: &str| {
        json["favorites"].as_array().unwrap().iter().find(|f| f["id"] == json!(id)).unwrap().clone()
    };
    let text = by_id("1");
    assert_eq!(
        (&text["saved_at"], &text["text"], &text["tags"], &text["type"]),
        (
            &json!("2025-10-09T08:53:21Z"),
            &json!({"text": "Bring water"}),
            &json!(["hike"]),
            &json!(1)
        )
    );
    assert_eq!(
        text["source"],
        json!({
            "from": "wxid_amy",
            "from_name": "Auntie",
            "chat": GROUP,
            "chat_name": "Hiking",
            "at": "2025-10-09T08:53:20Z",
            "message_id": "9101",
            "link": null,
        })
    );
    assert_eq!(text["content"], json!(favitem(1, "<desc>Bring water</desc>")));
    assert_eq!(
        by_id("2")["link"],
        json!({"title": "Routes", "summary": "North face", "url": "https://e.com/r", "cover": "https://e.com/r.jpg", "source": "Crag News"})
    );
    assert_eq!(
        by_id("3")["image"],
        json!({"file_id": "fav:3:img1", "size": 404, "file_present": true, "thumbnail_present": true})
    );
    assert_eq!(
        by_id("4")["video"],
        json!({"file_id": "fav:4:vid1", "size": 9000, "seconds": 21, "file_present": false, "thumbnail_present": true})
    );
    assert_eq!(
        by_id("5")["file"],
        json!({"file_id": "fav:5:pdf1", "size": 13, "name": "topo.pdf", "extension": "pdf", "file_present": true, "thumbnail_present": null})
    );
    assert_eq!(
        by_id("6")["voice"],
        json!({"file_id": "fav:6:voi1", "size": null, "length_ms": 4500, "file_present": false, "thumbnail_present": null})
    );
    assert_eq!(
        by_id("7")["location"],
        json!({"latitude": 39.9, "longitude": 116.4, "name": "Summit", "address": "Peak Rd 1"})
    );
    let history = by_id("8");
    assert_eq!(
        (&history["chat_history"]["title"], &history["chat_history"]["summary"]),
        (&json!("Trip chat"), &json!("Amy: Go"))
    );
    assert_eq!(
        history["chat_history"]["items"][0],
        json!({"kind": "text", "text": {"text": "Go at six"}, "sender": "wxid_amy", "sender_name": "Amy", "at": "2025-10-09T08:53:20Z", "type": 1})
    );
    assert_eq!(history["chat_history"]["items"][1]["image"]["file_id"], json!("fav:8:h2"));
    let note = by_id("13");
    assert_eq!(note["note"]["items"][0]["file"]["file_present"], json!(true));
    assert_eq!(note["note"]["items"][1]["text"]["text"], json!("Pack: rope, water"));
    assert_eq!(by_id("12")["unknown"], json!({}));
}

#[test]
fn it_pages_with_next_and_refuses_a_cursor_it_did_not_write() {
    let world = World::new();
    let keys = account(&world).all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let first = world.run(&["favorites", "--limit", "5"], &env).ok();
    assert_eq!(kinds(&first), ["note", "unknown", "channels", "mini_program", "music"]);
    let next = first["next"].as_str().unwrap();
    let second = world.run(&["favorites", "--limit", "5", "--cursor", next], &env).ok();
    assert_eq!(kinds(&second), ["chat_history", "location", "voice", "file", "video"]);
    let next = second["next"].as_str().unwrap();
    let third = world.run(&["favorites", "--limit", "5", "--cursor", next], &env).ok();
    assert_eq!(
        (kinds(&third), &third["next"]),
        (vec!["image".to_string(), "link".into(), "text".into()], &json!(null))
    );
    let error = world.run(&["favorites", "--cursor", "nope"], &env).error();
    assert!(error.contains("leave --cursor out"), "{error}");
    let error = world.run(&["favorites", "--limit", "0"], &env).error();
    assert!(error.contains("--limit 0"), "{error}");
}

#[test]
fn an_account_without_a_favorites_database_has_none() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy", "")];
    account.write();
    let json = world.run(&["favorites"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(json, json!({"account": "wxid_me_1a2b", "favorites": [], "next": null}));
}

#[test]
fn keys_without_the_favorites_databases_name_it_and_how_to_get_it() {
    let world = World::new();
    let account = account(&world);
    let keys = account.keys(&[CONTACT_DB]);
    let error = world.run(&["favorites"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(
        error.contains(FAVORITE_DB)
            && error.contains("Favorites (收藏)")
            && error.contains("sudo wxread keys"),
        "{error}"
    );
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn a_favorites_file_is_written_unsealed_as_the_file_it_is() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let before = snapshot(&world.data_dir());
    let out = world.root.join("out");
    let out_text = out.to_str().unwrap();
    for (id, bytes, content_type) in [
        ("fav:3:img1", jpeg(1), "image/jpeg"),
        ("fav:13:n1", NOTE.to_vec(), "application/octet-stream"),
        ("fav:5:pdf1", PDF.to_vec(), "application/pdf"),
    ] {
        let json = world.run(&["file", "get", id, "--out", out_text], &env).ok();
        assert_eq!(
            json,
            json!({"path": out_text, "size": bytes.len(), "sha256": sha(&bytes), "content_type": content_type}),
            "{id}"
        );
        assert_eq!(std::fs::read(&out).unwrap(), bytes, "{id}");
    }
    let json =
        world.run(&["file", "get", "fav:3:img1", "--thumbnail", "--out", out_text], &env).ok();
    assert_eq!(json["sha256"], json!(sha(&jpeg(2))));
    assert_eq!(snapshot(&world.data_dir()), before, "nothing is written beside WeChat's files");
}

#[test]
fn a_favorites_file_wechat_does_not_hold_is_missing_with_its_thumbnail_and_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let out = world.root.join("v");
    let out_text = out.to_str().unwrap();
    let json = world.run(&["file", "get", "fav:4:vid1", "--out", out_text], &env).ok();
    assert_eq!(json["missing"], json!(true));
    assert_eq!(json["thumbnail"]["sha256"], json!(sha(&jpeg(3))));
    let fix = json["fix"].as_str().unwrap();
    assert!(
        fix.contains("open favorite 4 in WeChat's Favorites")
            && fix.contains("--out holds its thumbnail"),
        "{fix}"
    );
    let silent = world.root.join("s");
    let json =
        world.run(&["file", "get", "fav:6:voi1", "--out", silent.to_str().unwrap()], &env).ok();
    assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &json!(null)));
    assert!(!silent.exists(), "nothing is written for a file WeChat does not hold");
    let json = world
        .run(&["file", "get", "fav:8:h2", "--thumbnail", "--out", silent.to_str().unwrap()], &env)
        .ok();
    assert_eq!((&json["missing"], &json["thumbnail"]), (&json!(true), &json!(null)));
}

#[test]
fn a_favorites_file_id_that_names_no_file_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let out = world.root.join("x").display().to_string();
    for (id, says) in [
        ("fav:1", "fav:<favorite>:<item>"),
        ("fav:99:img1", "has no favorite 99"),
        ("fav:3:nope", "holding item nope"),
        ("fav:8:h1", "is a text of favorite 8"),
    ] {
        let error = world.run(&["file", "get", id, "--out", &out], &env).error();
        assert!(error.contains(says), "{id}: {error}");
    }
    let error =
        world.run(&["file", "get", "fav:5:pdf1", "--thumbnail", "--out", &out], &env).error();
    assert!(error.contains("leave --thumbnail out"), "{error}");
    let error = world.run(&["file", "get", "x1", "--out", &out], &env).error();
    assert!(error.contains("wxread favorites"), "a message id's fix names favorites' too: {error}");
}
