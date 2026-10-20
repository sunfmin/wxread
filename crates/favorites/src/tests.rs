use proptest::prelude::*;

use super::*;

const ME: &str = "wxid_me";
const GROUP: &str = "123@chatroom";

/// A favorite of `fav_type` saved at `at`, its `<favitem>` holding `body`
/// beside a source of `wxid_amy` said in the group.
fn row(local_id: i64, fav_type: i64, at: i64, body: &str) -> Row {
    Row {
        local_id,
        fav_type,
        update_time: at,
        content: format!(
            "<favitem type=\"{fav_type}\">{body}<source sourcetype=\"1\" sourceid=\"9\"><fromusr>{GROUP}</fromusr><tousr>{ME}</tousr><realchatname>wxid_amy</realchatname><createtime>1760000000</createtime><msgid>4242</msgid></source><taglist><tag>hike</tag><tag> </tag><tag>gear</tag></taglist></favitem>"
        ),
        from: Some(GROUP.into()),
        sender: Some("wxid_amy".into()),
    }
}

fn names() -> BTreeMap<String, String> {
    [("wxid_amy", "Amy"), (GROUP, "Hiking"), ("wxid_bob", "Bob")]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// What a favorite is answered as, WeChat holding the files at `held`.
fn read(row: &Row, held: &[String]) -> Value {
    let names = names();
    let held: BTreeSet<String> = held.iter().cloned().collect();
    favorite(row, &Seen { own: ME, names: &names, held: &held })
}

/// An item of `datatype` with `dataid`, holding `body`.
fn item(datatype: i64, dataid: &str, body: &str) -> String {
    format!("<dataitem datatype=\"{datatype}\" dataid=\"{dataid}\">{body}</dataitem>")
}

fn list(items: &[String]) -> String {
    format!("<datalist count=\"{}\">{}</datalist>", items.len(), items.concat())
}

#[test]
fn a_favorite_is_its_kind_with_its_source_tags_type_and_content() {
    let r = row(7, 1, 1_760_000_100, "<desc>Bring water</desc>");
    let got = read(&r, &[]);
    assert_eq!(
        got,
        json!({
            "id": "7",
            "saved_at": "2025-10-09T08:55:00Z",
            "kind": "text",
            "text": {"text": "Bring water"},
            "source": {
                "from": "wxid_amy",
                "from_name": "Amy",
                "chat": GROUP,
                "chat_name": "Hiking",
                "at": "2025-10-09T08:53:20Z",
                "message_id": "4242",
                "link": null,
            },
            "tags": ["hike", "gear"],
            "type": 1,
            "content": r.content,
        })
    );
}

#[test]
fn what_was_said_directly_is_from_its_sayer_in_the_chat_with_the_other() {
    let mine = Row {
        content: format!(
            "<favitem type=\"1\"><desc>hi</desc><source><fromusr>{ME}</fromusr><tousr>wxid_bob</tousr></source></favitem>"
        ),
        ..row(1, 1, 1, "")
    };
    let s = &read(&mine, &[])["source"];
    assert_eq!(
        (&s["from"], &s["chat"], &s["chat_name"]),
        (&json!(ME), &json!("wxid_bob"), &json!("Bob"))
    );
    let theirs = Row {
        content: format!(
            "<favitem type=\"1\"><desc>hi</desc><source><fromusr>wxid_bob</fromusr><tousr>{ME}</tousr></source></favitem>"
        ),
        ..row(1, 1, 1, "")
    };
    let s = &read(&theirs, &[])["source"];
    assert_eq!((&s["from"], &s["chat"]), (&json!("wxid_bob"), &json!("wxid_bob")));
    let linked = Row {
        content: "<favitem type=\"5\"><source><link>https://e.com/a</link><fromusr>gh_crag</fromusr></source></favitem>".into(),
        from: None,
        sender: None,
        ..row(1, 5, 1, "")
    };
    let s = &read(&linked, &[])["source"];
    assert_eq!(
        (&s["from"], &s["link"], &s["at"]),
        (&json!("gh_crag"), &json!("https://e.com/a"), &json!(null))
    );
}

#[test]
fn each_kind_is_read_as_it_is() {
    let cases: Vec<(i64, String, &str, Value)> = vec![
        (
            5,
            "<weburlitem><appmsgshareitem><srcdisplayname>Crag News</srcdisplayname></appmsgshareitem><pagethumb_url>https://e.com/c.jpg</pagethumb_url><pagetitle>Routes</pagetitle><pagedesc>North face</pagedesc></weburlitem>".replace("<weburlitem>", "<source><link>https://e.com/r</link></source><weburlitem>"),
            "link",
            json!({"title": "Routes", "summary": "North face", "url": "https://e.com/r", "cover": "https://e.com/c.jpg", "source": "Crag News"}),
        ),
        (
            6,
            "<locitem><lng>116.4</lng><poiname>Summit</poiname><label>Peak Rd 1</label><lat>39.9</lat></locitem>".into(),
            "location",
            json!({"latitude": 39.9, "longitude": 116.4, "name": "Summit", "address": "Peak Rd 1"}),
        ),
        (
            21,
            format!("<desc>Song</desc>{}", list(&[item(29, "m1", "<datatitle>Song</datatitle><datadesc>Singer</datadesc><stream_weburl>https://e.com/s</stream_weburl><stream_dataurl>https://e.com/s.mp3</stream_dataurl>")])),
            "music",
            json!({"title": "Song", "summary": "Singer", "url": "https://e.com/s", "audio_url": "https://e.com/s.mp3"}),
        ),
        (
            19,
            "<appbranditem><appid>wx1</appid><sourcedisplayname>Trails</sourcedisplayname><pagepath>p/1</pagepath></appbranditem><title>A trail</title>".into(),
            "mini_program",
            json!({"title": "A trail", "app": "Trails", "app_id": "wx1", "path": "p/1"}),
        ),
        (
            20,
            "<finderFeed><nickname>Climber</nickname><desc>Day one</desc></finderFeed><title>Day one</title>".into(),
            "channels",
            json!({"title": "Day one", "author": "Climber", "summary": "Day one"}),
        ),
        (99, "<desc>?</desc>".into(), "unknown", json!({})),
    ];
    for (t, body, kind, fields) in cases {
        let got = read(&row(1, t, 1, &body), &[]);
        assert_eq!((got["kind"].as_str(), &got[kind]), (Some(kind), &fields), "{t}");
    }
}

#[test]
fn a_media_favorite_names_its_file_and_says_whether_wechat_holds_it() {
    let image = list(&[item(
        2,
        "d1",
        "<fullmd5>f1</fullmd5><thumbfullmd5>t1</thumbfullmd5><fullsize>300</fullsize>",
    )]);
    let r = row(3, 2, 1, &image);
    assert_eq!(
        read(&r, &[path("d1", Some("t1"), "thumb")])["image"],
        json!({"file_id": "fav:3:d1", "size": 300, "file_present": false, "thumbnail_present": true})
    );
    assert_eq!(read(&r, &[path("d1", Some("f1"), "data")])["image"]["file_present"], json!(true));
    let video = list(&[item(
        15,
        "v1",
        "<fullmd5>f</fullmd5><duration>21</duration><fullsize>9</fullsize>",
    )]);
    assert_eq!(
        read(&row(4, 16, 1, &video), &[])["video"],
        json!({"file_id": "fav:4:v1", "size": 9, "seconds": 21, "file_present": false, "thumbnail_present": false})
    );
    let voice = list(&[item(3, "s1", "<duration>4500</duration>")]);
    assert_eq!(
        read(&row(5, 3, 1, &voice), &[])["voice"],
        json!({"file_id": "fav:5:s1", "size": null, "length_ms": 4500, "file_present": false, "thumbnail_present": null})
    );
    let file = list(&[item(
        8,
        "p1",
        "<datafmt>pdf</datafmt><datatitle>topo.pdf</datatitle><fullsize>13</fullsize>",
    )]);
    assert_eq!(
        read(&row(6, 8, 1, &file), &[])["file"],
        json!({"file_id": "fav:6:p1", "size": 13, "name": "topo.pdf", "extension": "pdf", "file_present": false, "thumbnail_present": null})
    );
}

#[test]
fn a_chat_history_is_each_message_as_its_kind_a_nested_one_with_its_own() {
    let nested = format!(
        "<recordxml><recordinfo>{}</recordinfo></recordxml><datatitle>Older</datatitle>",
        list(&[item(1, "n1", "<datadesc>inner</datadesc>")])
    );
    let body = format!(
        "<title>Trip</title><desc>Amy: Go</desc>{}",
        list(&[
            item(
                1,
                "a",
                "<datadesc>Go</datadesc><datasrcname>Amy</datasrcname><srcMsgCreateTime>1760000000</srcMsgCreateTime><dataitemsource><realchatname>wxid_amy</realchatname></dataitemsource>"
            ),
            item(2, "b", "<fullmd5>m</fullmd5><datasrcname>Bob</datasrcname>"),
            item(17, "c", &nested),
            item(42, "d", ""),
        ])
    );
    let got = &read(&row(8, 14, 1, &body), &[])["chat_history"];
    assert_eq!((&got["title"], &got["summary"]), (&json!("Trip"), &json!("Amy: Go")));
    let items = got["items"].as_array().unwrap();
    assert_eq!(
        items[0],
        json!({"kind": "text", "text": {"text": "Go"}, "sender": "wxid_amy", "sender_name": "Amy", "at": "2025-10-09T08:53:20Z", "type": 1})
    );
    assert_eq!(items[1]["image"]["file_id"], json!("fav:8:b"));
    assert_eq!(items[2]["chat_history"]["title"], json!("Older"));
    assert_eq!(items[2]["chat_history"]["items"][0]["text"]["text"], json!("inner"));
    assert_eq!((&items[3]["kind"], &items[3]["unknown"]), (&json!("unknown"), &json!({})));
}

#[test]
fn a_note_is_its_paragraphs_images_and_places() {
    let body = list(&[
        item(8, "h", "<datafmt>.htm</datafmt><fullmd5>hm</fullmd5>"),
        item(1, "t", "<datadesc>Pack list</datadesc>"),
        item(6, "l", "<locitem><lat>1.5</lat><lng>2.5</lng></locitem>"),
        item(2, "i", "<fullmd5>im</fullmd5>"),
    ]);
    let got = &read(&row(9, 18, 1, &body), &[path("h", Some("hm"), "data")])["note"]["items"];
    let kinds: Vec<&str> =
        got.as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["file", "text", "location", "image"]);
    assert_eq!(
        (&got[0]["file"]["extension"], &got[0]["file"]["file_present"]),
        (&json!("htm"), &json!(true))
    );
    assert_eq!(got[2]["location"]["latitude"], json!(1.5));
}

#[test]
fn a_content_that_does_not_read_leaves_its_fields_out_never_the_favorite() {
    let r = Row { content: String::new(), ..row(2, 2, 1, "") };
    let got = read(&r, &[]);
    assert_eq!(
        (&got["kind"], &got["image"], &got["tags"]),
        (&json!("image"), &json!({}), &json!([]))
    );
    assert_eq!(
        (&got["source"]["from"], &got["source"]["chat"]),
        (&json!("wxid_amy"), &json!(GROUP))
    );
    let odd = row(3, 1, 1, "<desc>Tom & Jerry</desc>");
    assert_eq!(read(&odd, &[])["text"]["text"], json!("Tom & Jerry"));
}

#[test]
fn a_files_path_is_the_md5_of_its_item_its_md5_and_which() {
    let h = format!("{:x}", md5::compute("d1f1_data"));
    assert_eq!(path("d1", Some("f1"), "data"), format!("business/favorite/data/{}/{h}", &h[..2]));
    let t = format!("{:x}", md5::compute("d1_thumb"));
    assert_eq!(path("d1", None, "thumb"), format!("business/favorite/thumb/{}/{t}", &t[..2]));
}

#[test]
fn every_file_a_favorite_names_is_listed_to_be_looked_for() {
    let body = list(&[
        item(1, "t", "<datadesc>x</datadesc>"),
        item(2, "i", "<fullmd5>im</fullmd5><thumbfullmd5>it</thumbfullmd5>"),
        "<dataitem datatype=\"2\"><fullmd5>none</fullmd5></dataitem>".to_string(),
    ]);
    assert_eq!(
        paths(&row(1, 18, 1, &body)),
        [
            path("t", None, "data"),
            path("t", None, "thumb"),
            path("i", Some("im"), "data"),
            path("i", Some("it"), "thumb"),
        ]
    );
    assert!(paths(&Row { content: String::new(), ..row(1, 2, 1, "") }).is_empty());
}

#[test]
fn a_file_is_found_by_its_item_wherever_it_is() {
    let body = list(&[
        item(1, "t", "<datadesc>x</datadesc>"),
        item(
            17,
            "c",
            &format!(
                "<recordxml><recordinfo>{}</recordinfo></recordxml>",
                list(&[item(4, "v", "<fullmd5>vm</fullmd5><thumbfullmd5>vt</thumbfullmd5>")])
            ),
        ),
    ]);
    let r = row(4, 14, 1, &body);
    assert_eq!(
        stored(&r, "v"),
        Ok(Some(Stored {
            kind: Kind::Video,
            file: path("v", Some("vm"), "data"),
            thumbnail: Some(path("v", Some("vt"), "thumb")),
        }))
    );
    assert_eq!(stored(&r, "t"), Err(Kind::Text));
    assert_eq!(stored(&r, "nope"), Ok(None));
    let file = row(5, 8, 1, &list(&[item(8, "p", "")]));
    assert_eq!(stored(&file, "p").unwrap().unwrap().thumbnail, None);
}

#[test]
fn a_file_id_is_its_favorite_and_its_item() {
    let id = FileId::parse("fav:12:ab_9-Z").unwrap();
    assert_eq!(
        (id.favorite, id.item.as_str(), id.text()),
        (12, "ab_9-Z", "fav:12:ab_9-Z".to_string())
    );
    for not in
        ["12:ab", "fav:", "fav:12", "fav:12:", "fav:x:ab", "fav:-1:ab", "fav:1:a/b", "fav:1:a:b"]
    {
        assert_eq!(FileId::parse(not), None, "{not}");
    }
    assert!(is_file_id("fav:x") && !is_file_id("9015"));
}

#[test]
fn every_kind_has_its_name_and_types_name_their_kinds() {
    let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "text",
            "link",
            "image",
            "video",
            "voice",
            "file",
            "location",
            "music",
            "chat_history",
            "note",
            "mini_program",
            "channels",
            "unknown"
        ]
    );
    for (t, kind) in TYPES {
        assert_eq!(Kind::of_type(t), kind);
    }
    assert_eq!(Kind::of_type(0), Kind::Unknown);
}

#[test]
fn a_cursor_wxread_did_not_write_is_refused_with_its_fix() {
    for text in ["", "x", "1", "1:", "x:1", "1:y"] {
        assert!(Cursor::parse(text).unwrap_err().contains("leave --cursor out"), "{text}");
    }
    assert_eq!(Cursor::parse("-4:2").unwrap().text(), "-4:2");
}

#[test]
fn what_wechat_does_not_hold_says_how_to_make_it_fetch_it() {
    let id = FileId { favorite: 3, item: "d".into() };
    let fix = fetch_fix(&id, Kind::Image, false, true);
    assert!(
        fix.contains("open favorite 3 in WeChat's Favorites")
            && fix.contains("--out holds its thumbnail"),
        "{fix}"
    );
    assert!(fetch_fix(&id, Kind::Video, true, false).contains("this video's thumbnail"));
}

proptest! {
    #[test]
    fn paging_lists_every_favorite_once_newest_first(
        saved in proptest::collection::vec(0i64..4, 0..15),
        limit in 1usize..6,
    ) {
        let rows: Vec<Row> = saved.iter().enumerate().map(|(i, &at)| row(i as i64, 1, at, "<desc>x</desc>")).collect();
        let ids = |p: &Page| -> Vec<(i64, i64)> {
            p.rows.iter().map(|r| (r.update_time, r.local_id)).collect()
        };
        let whole = ids(&page(rows.clone(), None, Limit::new(1000).unwrap()));
        let mut sorted = whole.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        prop_assert_eq!(&whole, &sorted);
        prop_assert_eq!(whole.len(), rows.len());
        let mut paged = Vec::new();
        let mut after = None;
        loop {
            let p = page(rows.clone(), after.as_ref(), Limit::new(limit).unwrap());
            prop_assert!(p.rows.len() <= limit);
            paged.extend(ids(&p));
            match p.next {
                Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                None => break,
            }
        }
        prop_assert_eq!(paged, whole);
    }
}
