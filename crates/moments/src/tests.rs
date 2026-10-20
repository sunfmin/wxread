use super::*;
use proptest::prelude::*;

/// A Moment's id as WeChat makes one: the millisecond it was posted in,
/// then 23 bits of its own.
fn id_at(secs: i64, own: i64) -> i64 {
    (secs * 1000) << 23 | own
}

fn row(tid: i64, author: &str, content: &str) -> Row {
    Row { tid, author: author.into(), content: content.into() }
}

/// A Moment of `author` at `secs`, with its text, media, likes and
/// comments as WeChat writes them.
fn full(secs: i64) -> String {
    format!(
        r#"<SnsDataItem><TimelineObject><id>{id}</id><username>wxid_amy</username><createTime>{secs}</createTime><contentDesc>Summit &amp; back</contentDesc><location poiName="Mount Tai" poiAddress="Tai'an" city="Tai'an" latitude="36.25" longitude="117.1" /><ContentObject><type>1</type><title></title><description></description><contentUrl></contentUrl><mediaList><media><id>111</id><type>2</type><url md5="x" token="t">https://szmmsns.qpic.cn/a/0</url><thumb>https://szmmsns.qpic.cn/a/150</thumb><size width="1080" height="1440" totalSize="9" /></media><media><id>222</id><type>6</type><url>https://snsvideo.qq.com/v</url><thumb>https://snsvideo.qq.com/v.jpg</thumb><size width="720" height="1280" /></media><media><id>333</id><type>5</type><url>https://e.com/x</url></media></mediaList></ContentObject></TimelineObject><LocalExtraInfo><nickname>Amy Own</nickname><like_user_list><user_comment><username>wxid_bob</username><nickname>Bobby</nickname><create_time>{like}</create_time></user_comment><user_comment><username>wxid_cat</username><nickname>Cat</nickname><create_time>{like}</create_time></user_comment></like_user_list><comment_user_list><user_comment><username>wxid_bob</username><nickname>Bobby</nickname><content>Wow</content><create_time>{like}</create_time><comment_id>1</comment_id></user_comment><user_comment><username>wxid_amy</username><nickname>Amy Own</nickname><content>Thanks!</content><create_time>{like}</create_time><ref_username>wxid_bob</ref_username></user_comment><user_comment><username>wxid_cat</username><content>gone</content><b_deleted>1</b_deleted></user_comment></comment_user_list></LocalExtraInfo></SnsDataItem>"#,
        id = id_at(secs, 5) as u64,
        like = secs + 60,
    )
}

fn names() -> BTreeMap<String, String> {
    BTreeMap::from([("wxid_bob".into(), "Bob (climbing)".into()), ("wxid_amy".into(), "".into())])
}

fn person(id: &str, name: &str) -> Person {
    Person { id: id.into(), name: name.into() }
}

#[test]
fn a_moment_is_read_with_its_text_media_likes_and_comments() {
    let secs = 1_760_000_000;
    let tid = id_at(secs, 5);
    let m = moment(&row(tid, "wxid_amy", &full(secs)), &names());
    let id = (tid as u64).to_string();
    assert_eq!(m.id, id);
    assert_eq!(m.author, person("wxid_amy", "Amy Own"));
    assert_eq!(m.at.as_deref(), Some("2025-10-09T08:53:20Z"));
    assert_eq!(m.text, "Summit & back");
    assert_eq!(
        m.location,
        Some(Location {
            name: "Mount Tai".into(),
            address: "Tai'an".into(),
            city: "Tai'an".into(),
            latitude: Some("36.25".into()),
            longitude: Some("117.1".into()),
        })
    );
    assert_eq!(m.link, None);
    assert_eq!(m.kind, Some(1));
    let kinds: Vec<(&str, &str)> = m.media.iter().map(|x| (x.id.as_str(), x.kind)).collect();
    let ids = [format!("{id}:0"), format!("{id}:1"), format!("{id}:2")];
    assert_eq!(kinds, [(&*ids[0], "image"), (&*ids[1], "video"), (&*ids[2], "other")]);
    assert_eq!(
        (m.media[0].url.as_str(), m.media[0].thumbnail_url.as_str()),
        ("https://szmmsns.qpic.cn/a/0", "https://szmmsns.qpic.cn/a/150")
    );
    assert_eq!((m.media[0].width, m.media[0].height), (Some(1080), Some(1440)));
    let video = moment_video_key(&id, "222");
    assert_eq!(m.media[1].file, Some(Media::MomentVideo { key: Some(video) }));
    assert_eq!(m.media[2].file, None);
    let at = Some("2025-10-09T08:54:20Z".to_string());
    let like = |by| Said { by, at: at.clone(), text: String::new(), reply_to: None };
    assert_eq!(
        m.likes,
        [like(person("wxid_bob", "Bob (climbing)")), like(person("wxid_cat", "Cat"))]
    );
    assert_eq!(
        m.comments,
        [
            Said { text: "Wow".into(), ..like(person("wxid_bob", "Bob (climbing)")) },
            Said {
                text: "Thanks!".into(),
                reply_to: Some(person("wxid_bob", "Bob (climbing)")),
                ..like(person("wxid_amy", "Amy Own"))
            },
        ],
        "a deleted comment is not listed"
    );
    assert_eq!(m.content, full(secs));
}

#[test]
fn a_link_is_read_and_a_moment_with_no_location_has_none() {
    let link = r#"<SnsDataItem><TimelineObject><id>9</id><username>u</username><createTime>1</createTime><contentDesc></contentDesc><location poiName="" latitude="0" longitude="0.0" /><ContentObject><type>3</type><title>Routes</title><description>North face</description><contentUrl>https://mp.weixin.qq.com/s/r?a=1&amp;b=2</contentUrl></ContentObject></TimelineObject></SnsDataItem>"#;
    let m = moment(&row(9, "u", link), &BTreeMap::new());
    assert_eq!(
        m.link,
        Some(Link {
            title: "Routes".into(),
            description: "North face".into(),
            url: "https://mp.weixin.qq.com/s/r?a=1&b=2".into(),
        })
    );
    assert_eq!((m.location, m.text.as_str(), m.media.len()), (None, "", 0));
    assert_eq!((m.likes.len(), m.comments.len()), (0, 0), "no LocalExtraInfo, no likes");
}

#[test]
fn a_moment_whose_xml_does_not_read_is_still_listed() {
    for content in ["", "<SnsDataItem>", "not xml", "<other/>"] {
        let m = moment(&row(-3, "wxid_amy", content), &names());
        assert_eq!(m.id, (-3i64 as u64).to_string());
        assert_eq!(
            (m.author.id.as_str(), m.at.clone(), m.content.as_str()),
            ("wxid_amy", None, content)
        );
    }
}

#[test]
fn a_video_with_no_id_in_its_moment_is_cached_under_the_rows_id() {
    let xml = r#"<TimelineObject><username>u</username><ContentObject><mediaList><media><id>7</id><type>6</type></media></mediaList></ContentObject></TimelineObject>"#;
    let m = moment(&row(-2, "u", xml), &BTreeMap::new());
    let key = moment_video_key(&(-2i64 as u64).to_string(), "7");
    assert_eq!(m.media[0].file, Some(Media::MomentVideo { key: Some(key) }));
}

#[test]
fn moments_are_newest_first_by_their_id_even_past_i64() {
    // An id past i64::MAX is negative as SQLite holds it, and newer.
    let rows = [row(5, "a", ""), row(-1, "b", ""), row(id_at(1_760_000_000, 0), "c", "")];
    let p = page(&rows, &BTreeMap::new(), None, Limit::new(2).unwrap());
    let authors: Vec<&str> = p.moments.iter().map(|m| m.author.id.as_str()).collect();
    assert_eq!(authors, ["b", "c"]);
    let next = Cursor::parse(&p.next.unwrap().text()).unwrap();
    let rest = page(&rows, &BTreeMap::new(), Some(&next), Limit::new(2).unwrap());
    assert_eq!((rest.moments[0].author.id.as_str(), rest.next), ("a", None));
}

#[test]
fn a_cursor_or_media_id_wxread_did_not_write_is_refused() {
    for text in ["", "x", "-1", "1:2", "18446744073709551616"] {
        assert!(Cursor::parse(text).unwrap_err().contains("leave --cursor out"), "{text}");
    }
    assert_eq!(
        MediaId::parse("18446744073709551615:2"),
        Some(MediaId { moment: u64::MAX, place: 2 })
    );
    assert_eq!(MediaId::parse("18446744073709551615:2").unwrap().tid(), -1);
    for text in ["9015", "1:", ":1", "1:x", "a:1", "1:2:3"] {
        assert_eq!(MediaId::parse(text), None, "{text}");
    }
}

proptest! {
    #[test]
    fn paging_lists_every_moment_once_in_order(
        tids in proptest::collection::btree_set(any::<i64>(), 0..12),
        limit in 1usize..6,
    ) {
        let rows: Vec<Row> = tids.iter().map(|&t| row(t, "a", "")).collect();
        let names = BTreeMap::new();
        let whole = page(&rows, &names, None, Limit::new(1000).unwrap());
        let mut paged = Vec::new();
        let mut after = None;
        loop {
            let p = page(&rows, &names, after.as_ref(), Limit::new(limit).unwrap());
            prop_assert!(p.moments.len() <= limit);
            paged.extend(p.moments);
            match p.next {
                Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                None => break,
            }
        }
        prop_assert_eq!(&paged, &whole.moments);
        let ids: Vec<u64> = paged.iter().map(|m| m.id.parse().unwrap()).collect();
        prop_assert!(ids.windows(2).all(|w| w[0] > w[1]));
    }
}
