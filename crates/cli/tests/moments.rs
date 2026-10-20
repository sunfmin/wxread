//! `wxread moments`: the Moments WeChat has loaded, read from its own
//! `sns/sns.db`, with their media, likes and comments, a page at a time;
//! and `wxread file get` of a Moment's media.

use serde_json::{Value, json};
use wxread_schema::{CONTACT_DB, SNS_DB};

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Posted};

/// A Moment's id as WeChat makes one: the millisecond it was posted in,
/// then 23 bits of its own; and that id as WeChat writes it.
fn tid(secs: i64, own: i64) -> i64 {
    (secs * 1000) << 23 | own
}
fn id(tid: i64) -> String {
    (tid as u64).to_string()
}

const AMY_AT: i64 = 1_760_000_400;
const BOB_AT: i64 = 1_760_000_200;
const OLD_AT: i64 = 1_760_000_100;

/// Amy's Moment: text, a place, an image and a video, two likes and a
/// comment Bob made that Amy answered.
fn amy() -> String {
    format!(
        r#"<SnsDataItem><TimelineObject><id>{id}</id><username>wxid_amy</username><createTime>{AMY_AT}</createTime><contentDesc>Summit at dawn</contentDesc><location poiName="Mount Tai" poiAddress="Tai'an, Shandong" city="Tai'an" latitude="36.25" longitude="117.1" /><ContentObject><type>15</type><mediaList><media><id>101</id><type>2</type><url>https://szmmsns.qpic.cn/a/0</url><thumb>https://szmmsns.qpic.cn/a/150</thumb><size width="1080" height="1440" /></media><media><id>102</id><type>6</type><url>https://snsvideo.qq.com/v</url><thumb>https://snsvideo.qq.com/v.jpg</thumb><size width="720" height="1280" /></media></mediaList></ContentObject></TimelineObject><LocalExtraInfo><nickname>Amy</nickname><like_user_list><user_comment><username>wxid_bob</username><nickname>Bob</nickname><create_time>{like}</create_time></user_comment><user_comment><username>wxid_stranger</username><nickname>Stranger</nickname><create_time>{like}</create_time></user_comment></like_user_list><comment_user_list><user_comment><username>wxid_bob</username><nickname>Bob</nickname><content>Wow &amp; wow</content><create_time>{like}</create_time></user_comment><user_comment><username>wxid_amy</username><nickname>Amy</nickname><content>Thanks!</content><create_time>{like}</create_time><ref_username>wxid_bob</ref_username></user_comment></comment_user_list></LocalExtraInfo></SnsDataItem>"#,
        id = id(tid(AMY_AT, 1)),
        like = AMY_AT + 60,
    )
}

/// A Moment of `author` at `at` sharing a link, with a video WeChat never
/// loaded.
fn link(author: &str, at: i64, own: i64) -> String {
    format!(
        r#"<SnsDataItem><TimelineObject><id>{id}</id><username>{author}</username><createTime>{at}</createTime><contentDesc>Read this</contentDesc><ContentObject><type>3</type><title>Routes</title><description>North face</description><contentUrl>https://mp.weixin.qq.com/s/r</contentUrl><mediaList><media><id>201</id><type>6</type><url>https://snsvideo.qq.com/w</url></media><media><id>202</id><type>5</type><url>https://e.com/cover</url></media></mediaList></ContentObject></TimelineObject></SnsDataItem>"#,
        id = id(tid(at, own)),
    )
}

/// The key WeChat caches the video `media` of Moment `post` under.
fn video_key(post: i64, media: &str) -> String {
    format!("{:x}", md5::compute(format!("{}_{media}_3", id(post))))
}

const MP4: &[u8] = b"\0\0\0\x18ftypmp42 a Moment's video";
const JPG: &[u8] = b"\xff\xd8\xff\xe0 a Moment's video's thumbnail";

/// An account that loaded Amy's Moment (only in its database's WAL), a
/// Moment of Bob's and an older one of his, with Amy's video and Bob's
/// video's thumbnail cached.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy W", "Auntie"), Contact("wxid_bob", "Bob", "")];
    account.moments = vec![
        Posted(tid(OLD_AT, 3), "wxid_bob", link("wxid_bob", OLD_AT, 3)),
        Posted(tid(BOB_AT, 2), "wxid_bob", link("wxid_bob", BOB_AT, 2)),
        Posted(tid(AMY_AT, 1), "wxid_amy", amy()),
    ];
    let cached = |month: &str, key: String, ext: &str| {
        format!("cache/{month}/Sns/Video/{}/{}{ext}", &key[..2], &key[2..])
    };
    account.files = vec![
        (cached("2025-10", video_key(tid(AMY_AT, 1), "102"), ".mp4"), MP4.to_vec()),
        (cached("2025-09", video_key(tid(BOB_AT, 2), "201"), ".jpg"), JPG.to_vec()),
    ];
    account.write();
    account
}

fn ids(json: &Value) -> Vec<String> {
    let moments = json["moments"].as_array().unwrap();
    moments.iter().map(|m| m["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn it_lists_each_moment_with_its_text_media_likes_and_comments_newest_first() {
    let world = World::new();
    let account = account(&world);
    let json = world.run(&["moments"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(json["account"], "wxid_me_1a2b");
    assert_eq!(ids(&json), [id(tid(AMY_AT, 1)), id(tid(BOB_AT, 2)), id(tid(OLD_AT, 3))]);
    assert_eq!(json["next"], Value::Null);
    let amy_id = id(tid(AMY_AT, 1));
    let auntie = json!({"id": "wxid_amy", "name": "Auntie"});
    let bob = json!({"id": "wxid_bob", "name": "Bob"});
    let at = "2025-10-09T09:01:00Z";
    assert_eq!(
        json["moments"][0],
        json!({
            "id": amy_id,
            "author": auntie,
            "at": "2025-10-09T09:00:00Z",
            "text": "Summit at dawn",
            "location": {
                "name": "Mount Tai",
                "address": "Tai'an, Shandong",
                "city": "Tai'an",
                "latitude": 36.25,
                "longitude": 117.1,
            },
            "link": null,
            "media": [
                {
                    "id": format!("{amy_id}:0"),
                    "kind": "image",
                    "url": "https://szmmsns.qpic.cn/a/0",
                    "thumbnail_url": "https://szmmsns.qpic.cn/a/150",
                    "width": 1080,
                    "height": 1440,
                    "file_present": false,
                    "thumbnail_present": null,
                },
                {
                    "id": format!("{amy_id}:1"),
                    "kind": "video",
                    "url": "https://snsvideo.qq.com/v",
                    "thumbnail_url": "https://snsvideo.qq.com/v.jpg",
                    "width": 720,
                    "height": 1280,
                    "file_present": true,
                    "thumbnail_present": false,
                },
            ],
            "likes": [
                {"by": bob, "at": at},
                {"by": {"id": "wxid_stranger", "name": "Stranger"}, "at": at},
            ],
            "comments": [
                {"by": bob, "at": at, "text": "Wow & wow", "reply_to": null},
                {"by": auntie, "at": at, "text": "Thanks!", "reply_to": bob},
            ],
            "type": 15,
            "content": amy(),
        })
    );
    let shared = &json["moments"][1];
    assert_eq!(
        shared["link"],
        json!({"title": "Routes", "description": "North face", "url": "https://mp.weixin.qq.com/s/r"})
    );
    let media = &shared["media"];
    assert_eq!(
        (media[0]["file_present"].clone(), media[0]["thumbnail_present"].clone()),
        (json!(false), json!(true))
    );
    assert_eq!(
        (media[1]["kind"].clone(), media[1]["file_present"].clone()),
        (json!("other"), Value::Null)
    );
    assert_eq!((shared["likes"].clone(), shared["comments"].clone()), (json!([]), json!([])));
}

#[test]
fn contact_narrows_to_one_author() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let json = world.run(&["moments", "--contact", "wxid_bob"], &[("WXREAD_KEYS", &keys)]).ok();
    assert_eq!(ids(&json), [id(tid(BOB_AT, 2)), id(tid(OLD_AT, 3))]);
    for nobody in ["wxid_nobody", "wxid_me_1a2b"] {
        let json = world.run(&["moments", "--contact", nobody], &[("WXREAD_KEYS", &keys)]).ok();
        assert_eq!(json, json!({"account": "wxid_me_1a2b", "moments": [], "next": null}));
    }
}

#[test]
fn an_account_with_no_moments_store_answers_an_empty_list() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("wxid_amy", "Amy", "")];
    account.write();
    let json = world.run(&["moments"], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(json, json!({"account": "wxid_me_1a2b", "moments": [], "next": null}));
}

#[test]
fn paging_with_next_returns_each_moment_once() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let whole = world.run(&["moments"], &env).ok()["moments"].clone();
    for limit in ["1", "2"] {
        let mut paged = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut args = vec!["moments", "--limit", limit];
            if let Some(c) = &cursor {
                args.extend(["--cursor", c.as_str()]);
            }
            let json = world.run(&args, &env).ok();
            paged.extend(json["moments"].as_array().unwrap().clone());
            match json["next"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        assert_eq!(Value::Array(paged), whole, "--limit {limit}");
    }
}

#[test]
fn a_cached_video_is_written_and_what_wechat_does_not_hold_is_missing() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let out = world.root.join("out");
    let out_s = out.to_str().unwrap();
    let amy = id(tid(AMY_AT, 1));
    let json = world.run(&["file", "get", &format!("{amy}:1"), "--out", out_s], &env).ok();
    assert_eq!(std::fs::read(&out).unwrap(), MP4);
    assert_eq!(
        (json["content_type"].clone(), json["size"].clone()),
        (json!("video/mp4"), json!(MP4.len()))
    );

    let json =
        world.run(&["file", "get", &format!("{amy}:1"), "--thumbnail", "--out", out_s], &env).ok();
    assert_eq!(json["missing"], true);
    assert_eq!(json["thumbnail"], Value::Null);
    let fix = json["fix"].as_str().unwrap();
    assert!(fix.contains("open Auntie's Moment of 2025-10-09T09:00:00Z"), "{fix}");

    // An image's cache name is not known yet (#28): it is missing.
    let json = world.run(&["file", "get", &format!("{amy}:0"), "--out", out_s], &env).ok();
    assert_eq!((json["missing"].clone(), json["thumbnail"].clone()), (json!(true), Value::Null));

    let bob = id(tid(BOB_AT, 2));
    let json = world.run(&["file", "get", &format!("{bob}:0"), "--out", out_s], &env).ok();
    assert_eq!(json["missing"], true);
    assert_eq!(json["thumbnail"]["content_type"], "image/jpeg");
    assert_eq!(std::fs::read(&out).unwrap(), JPG);
    let fix = json["fix"].as_str().unwrap();
    assert!(fix.contains("Bob's Moment") && fix.contains("--out holds its thumbnail"), "{fix}");
}

#[test]
fn a_moments_medium_that_is_not_there_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let out = world.root.join("out");
    let out = out.to_str().unwrap();
    let bob = id(tid(BOB_AT, 2));
    let amy = id(tid(AMY_AT, 1));
    for (asked, says) in [
        ("1:0".to_string(), "holds no Moment of 1:0"),
        (format!("{bob}:2"), "its Moment has 2, from 0"),
        (format!("{bob}:1"), "keeps no file of"),
        (format!("{amy}:0"), "leave --thumbnail out"),
    ] {
        let mut args = vec!["file", "get", asked.as_str(), "--out", out];
        if asked.ends_with(":0") && asked.starts_with(&amy) {
            args.push("--thumbnail");
        }
        let error = world.run(&args, &env).error();
        assert!(error.contains(says), "{asked}: {error}");
    }
    let error = world.run(&["file", "get", "x:y", "--out", out], &env).error();
    assert!(error.contains("nor a Moment's medium's"), "{error}");
    assert!(!std::path::Path::new(out).exists(), "nothing is written");
}

#[test]
fn no_decrypted_byte_is_written() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let before = snapshot(&account.dir());
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    world.run(&["moments"], &env).ok();
    world.run(&["moments", "--limit", "1", "--contact", "wxid_bob"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}

#[test]
fn a_database_the_keys_lack_is_named_with_the_fix() {
    let world = World::new();
    let account = account(&world);
    let error = world.run(&["moments"], &[("WXREAD_KEYS", &account.keys(&[CONTACT_DB]))]).error();
    assert!(
        error.contains("no key of sns/sns.db") && error.contains("sudo wxread keys"),
        "{error}"
    );
    let error = world.run(&["moments"], &[]).error();
    assert!(error.contains(SNS_DB) && error.contains(CONTACT_DB), "{error}");
}

#[test]
fn a_limit_or_cursor_that_is_not_one_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    for limit in ["0", "1001", "x"] {
        let error = world.run(&["moments", "--limit", limit], &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains("number of Moments from 1 to 1000"), "{limit}: {error}");
    }
    for cursor in ["nope", "1.5", "1:2"] {
        let error = world.run(&["moments", "--cursor", cursor], &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains("leave --cursor out"), "{cursor}: {error}");
    }
}
