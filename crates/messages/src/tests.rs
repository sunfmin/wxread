//! Every kind read from its content alone, and the paging, apart from any
//! database.

use proptest::prelude::*;
use serde_json::{Value, json};

use super::*;
use crate::kinds::{APP, CARD, LOCATION, SYSTEM, SYSTEM_XML, TEXT, parse, split};

/// `local_type` of an app message of subtype `sub`.
fn app(sub: i64) -> i64 {
    APP | sub << 32
}

/// What `content` of `local_type` is, as `{"kind", "<kind>": its fields}`.
fn read(local_type: i64, content: &str) -> Value {
    let p = parse("wxid_me", local_type, content, &[]);
    json!({ "kind": p.kind.name(), p.kind.name(): p.fields })
}

#[test]
fn the_subtype_is_in_the_high_bits() {
    assert_eq!(split(app(57)), (49, 57));
    assert_eq!(split(LOCATION), (48, 0));
}

#[test]
fn text_is_its_text() {
    assert_eq!(
        read(TEXT, "<msg> & hi"),
        json!({ "kind": "text", "text": { "text": "<msg> & hi" } })
    );
}

#[test]
fn a_media_message_is_its_kind_whatever_its_xml() {
    for (t, kind) in [
        (3, "image"),
        (34, "voice"),
        (43, "video"),
        (47, "sticker"),
        (app(6), "file"),
        (app(8), "sticker"),
    ] {
        assert_eq!(read(t, "<msg><img/></msg>")["kind"], kind);
        assert_eq!(read(t, "not xml")["kind"], kind);
    }
}

#[test]
fn what_does_not_read_is_unknown_not_an_error() {
    for (t, content) in [
        (app(5), "not xml"),
        (app(5), "<msg><appmsg><title>t</title>"),
        (50, "<msg><voipmsg/></msg>"),
        (app(53), "<msg><appmsg><title>接龙</title></appmsg></msg>"),
        (CARD, "<msg username=\"\"/>"),
        (LOCATION, "<msg><location x=\"a\" y=\"1\"/></msg>"),
        (SYSTEM, ""),
        (SYSTEM_XML, "<sysmsg type=\"mmchatroomtopmsg\"><x/></sysmsg>"),
        (SYSTEM_XML, "<sysmsg"),
    ] {
        assert_eq!(read(t, content), json!({ "kind": "unknown", "unknown": {} }), "{t}: {content}");
    }
}

#[test]
fn a_declaration_inside_and_a_bare_ampersand_still_read() {
    let declared = "<msg><?xml version=\"1.0\"?>\n<appmsg><title>t</title><type>5</type><url>https://e.com/a</url></appmsg></msg>";
    assert_eq!(read(app(5), declared)["link"]["url"], "https://e.com/a");
    let bare = "<msg><appmsg><title>A & B &#1x; &#xz; &#20;</title><url>https://e.com/?a=1&b=2&amp;c=3&#38;d=&#x26;</url></appmsg></msg>";
    let link = read(app(5), bare);
    assert_eq!(link["link"]["title"], "A & B &#1x; &#xz; &#20;");
    assert_eq!(link["link"]["url"], "https://e.com/?a=1&b=2&c=3&d=&");
}

#[test]
fn a_link_says_what_it_links_to() {
    let link = r#"<?xml version="1.0"?>
<msg><appmsg appid=""><title>Route topo</title><des><![CDATA[The north face]]></des><type>5</type><url>https://example.com/topo</url><sourcedisplayname>Crag News</sourcedisplayname></appmsg></msg>"#;
    assert_eq!(
        read(app(5), link),
        json!({ "kind": "link", "link": { "to": "web_page", "title": "Route topo", "summary": "The north face", "url": "https://example.com/topo", "source": "Crag News" } })
    );
    // The subtype from <type> when local_type has none.
    assert_eq!(read(APP, link)["link"]["to"], "web_page");
    for (sub, to) in [(3, "music"), (4, "video"), (33, "mini_program"), (36, "mini_program")] {
        let xml = format!("<msg><appmsg><title>x</title><type>{sub}</type></appmsg></msg>");
        assert_eq!(read(app(sub), &xml)["link"]["to"], to);
    }
    let program = "<msg><appmsg><title>Book a wall</title><type>33</type><weappinfo><appname>Climb</appname></weappinfo></appmsg></msg>";
    assert_eq!(read(app(33), program)["link"]["source"], "Climb");
    let feed = r#"<msg><appmsg><title></title><type>51</type><finderFeed><nickname>Crag TV</nickname><desc>Bouldering</desc></finderFeed></appmsg></msg>"#;
    assert_eq!(
        read(app(51), feed)["link"],
        json!({ "to": "channels_feed", "title": "Bouldering", "summary": "Crag TV", "url": null, "source": null })
    );
    let live = r#"<msg><appmsg><title></title><type>63</type><finderLive><nickname>Crag TV</nickname><desc></desc></finderLive></appmsg></msg>"#;
    assert_eq!(
        read(app(63), live)["link"],
        json!({ "to": "channels_live", "title": "Crag TV", "summary": "Crag TV", "url": null, "source": null })
    );
}

/// What WeChat writes as a channels feed's appmsg `<title>` and `<des>` for
/// clients too old to show one.
const PLACEHOLDER: &str = "当前微信版本不支持展示该内容，请升级至最新版本。";

#[test]
fn a_channels_feed_or_live_is_its_description_whatever_its_appmsg_title() {
    let feed = format!(
        "<msg><appmsg><title>{PLACEHOLDER}</title><des>{PLACEHOLDER}</des><type>51</type><url>https://support.weixin.qq.com/update/</url><finderFeed><nickname>Crag TV</nickname><desc>Bouldering at dawn</desc></finderFeed></appmsg></msg>"
    );
    assert_eq!(
        read(app(51), &feed)["link"],
        json!({ "to": "channels_feed", "title": "Bouldering at dawn", "summary": "Crag TV", "url": "https://support.weixin.qq.com/update/", "source": null })
    );
    let live = format!(
        "<msg><appmsg><title>{PLACEHOLDER}</title><des>{PLACEHOLDER}</des><type>63</type><finderLive><nickname>Crag TV</nickname><desc>Live from the wall</desc></finderLive></appmsg></msg>"
    );
    assert_eq!(
        read(app(63), &live)["link"],
        json!({ "to": "channels_live", "title": "Live from the wall", "summary": "Crag TV", "url": null, "source": null })
    );
    // A feed with no description is its author's nickname.
    let bare = format!(
        "<msg><appmsg><title>{PLACEHOLDER}</title><type>51</type><finderFeed><nickname>Crag TV</nickname><desc></desc></finderFeed></appmsg></msg>"
    );
    assert_eq!(read(app(51), &bare)["link"]["title"], "Crag TV");
}

#[test]
fn a_quote_has_the_reply_and_the_quoted_messages_id_sender_and_excerpt() {
    let quote = r#"<msg><appmsg><title>yes, 9am</title><type>57</type><refermsg><type>1</type><svrid>7001</svrid><fromusr>1@chatroom</fromusr><chatusr>wxid_ann</chatusr><displayname>Ann</displayname><content>Saturday?</content></refermsg></appmsg></msg>"#;
    assert_eq!(
        read(app(57), quote),
        json!({ "kind": "quote", "quote": { "text": "yes, 9am", "quoted": { "id": "7001", "sender": { "id": "wxid_ann", "display_name": "Ann" }, "excerpt": "Saturday?" } } })
    );
    let of_image = r#"<msg><appmsg><title>nice</title><refermsg><type>3</type><svrid>7002</svrid><fromusr>wxid_bob</fromusr><content>&lt;msg&gt;&lt;img/&gt;&lt;/msg&gt;</content></refermsg></appmsg></msg>"#;
    assert_eq!(
        read(app(57), of_image)["quote"]["quoted"],
        json!({ "id": "7002", "sender": { "id": "wxid_bob", "display_name": "" }, "excerpt": null })
    );
    let of_nothing = r#"<msg><appmsg><title>hm</title></appmsg></msg>"#;
    assert_eq!(read(app(57), of_nothing)["quote"]["quoted"]["id"], Value::Null);
}

#[test]
fn a_location_has_its_place_and_a_card_its_contact() {
    let at = r#"<msg><location x="22.543" y="114.057" scale="15" label="1 Shennan Rd, Shenzhen" poiname="The Gym" /></msg>"#;
    assert_eq!(
        read(LOCATION, at),
        json!({ "kind": "location", "location": { "latitude": 22.543, "longitude": 114.057, "name": "The Gym", "address": "1 Shennan Rd, Shenzhen" } })
    );
    assert_eq!(
        read(CARD, r#"<msg username="wxid_dan" nickname="Dan" bigheadimgurl=""/>"#),
        json!({ "kind": "card", "card": { "contact": { "id": "wxid_dan", "display_name": "Dan" } } })
    );
}

/// A forwarded history of `items`, its <recordinfo> kept as CDATA or
/// escaped.
fn forward(items: &str, cdata: bool) -> String {
    let record = format!(
        "<recordinfo><title>Chat history</title><datalist count=\"3\">{items}</datalist></recordinfo>"
    );
    let kept = if cdata {
        format!("<![CDATA[{record}]]>")
    } else {
        record.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    };
    format!(
        "<msg><appmsg><title>Ann and Ben's chat</title><type>19</type><recorditem>{kept}</recorditem></appmsg></msg>"
    )
}

pub const ITEMS: &str = r#"<dataitem datatype="1" dataid="a"><datadesc>Saturday?</datadesc><sourcename>Ann</sourcename><srcMsgCreateTime>1792402200</srcMsgCreateTime><dataitemsource><realchatname>wxid_ann</realchatname></dataitemsource></dataitem><dataitem datatype="17" dataid="b"><datatitle>Older</datatitle><sourcename>Ben</sourcename><recordxml><recordinfo><datalist><dataitem datatype="1"><datadesc>inside</datadesc><sourcename>Cat</sourcename><dataitemsource><fromusr>wxid_cat</fromusr></dataitemsource></dataitem><dataitem datatype="2"><datadesc>[Photo]</datadesc><sourcename>Cat</sourcename></dataitem></datalist></recordinfo></recordxml></dataitem><dataitem datatype="5"><datatitle>Route topo</datatitle><sourcename>Ben</sourcename><dataitemsource><hashusername>abc</hashusername></dataitemsource></dataitem><dataitem datatype="99"><datadesc>?</datadesc></dataitem>"#;

#[test]
fn a_forward_has_each_message_in_order_nested_ones_flattened_with_where_they_sit() {
    let expected = json!({ "kind": "forward", "forward": { "title": "Ann and Ben's chat", "messages": [
        { "sender": { "id": "wxid_ann", "display_name": "Ann" }, "at": "2026-10-19T09:30:00Z", "kind": "text", "content": "Saturday?", "type": 1, "inside": null },
        { "sender": { "id": null, "display_name": "Ben" }, "at": null, "kind": "forward", "content": "Older", "type": 17, "inside": null },
        { "sender": { "id": "wxid_cat", "display_name": "Cat" }, "at": null, "kind": "text", "content": "inside", "type": 1, "inside": 1 },
        { "sender": { "id": null, "display_name": "Cat" }, "at": null, "kind": "image", "content": "[Photo]", "type": 2, "inside": 1 },
        { "sender": { "id": null, "display_name": "Ben" }, "at": null, "kind": "link", "content": "Route topo", "type": 5, "inside": null },
        { "sender": null, "at": null, "kind": "unknown", "content": "?", "type": 99, "inside": null },
    ] } });
    assert_eq!(read(app(19), &forward(ITEMS, true)), expected);
    assert_eq!(read(app(19), &forward(ITEMS, false)), expected);
}

#[test]
fn a_recall_is_its_notice_and_the_accounts_own_is_its_own() {
    let theirs = r#"<sysmsg type="revokemsg"><revokemsg><session>1@chatroom</session><msgid>1</msgid><content><![CDATA["Ann" 撤回了一条消息]]></content></revokemsg></sysmsg>"#;
    for t in [SYSTEM, SYSTEM_XML] {
        let p = parse("wxid_me", t, theirs, &[]);
        assert_eq!(
            (p.kind, Value::Object(p.fields), p.sender, p.named.as_deref()),
            (
                Kind::Recall,
                json!({ "text": "\"Ann\" 撤回了一条消息", "recalled": null }),
                None,
                Some("Ann")
            )
        );
    }
    // As WeChat for Mac 4.x keeps them: plain text, or a `<revokemsg>`
    // with its text alone, in Chinese and in English.
    for (content, sender, named) in [
        ("\"Ann\" 撤回了一条消息", None, Some("Ann")),
        ("\"Ann \"Ace\"\" 撤回了一条消息", None, Some("Ann \"Ace\"")),
        ("你撤回了一条消息", Some("wxid_me"), None),
        ("You recalled a message", Some("wxid_me"), None),
        ("You recalled a message.", Some("wxid_me"), None),
        ("\"Ann\" recalled a message.", None, Some("Ann")),
        (
            r#"<?xml version="1.0"?><sysmsg type="revokemsg"><revokemsg><content>你撤回了一条消息</content><revoketime>0</revoketime></revokemsg></sysmsg>"#,
            Some("wxid_me"),
            None,
        ),
        (
            r#"<?xml version="1.0"?><sysmsg type="revokemsg"><revokemsg><content>You recalled a message.</content><revoketime>1791344471</revoketime></revokemsg></sysmsg>"#,
            Some("wxid_me"),
            None,
        ),
        (
            r#"<?xml version="1.0"?><sysmsg type="revokemsg"><revokemsg><content>"王卫星" 撤回了一条消息</content><revoketime>0</revoketime></revokemsg></sysmsg>"#,
            None,
            Some("王卫星"),
        ),
    ] {
        let p = parse("wxid_me", SYSTEM, content, &[]);
        assert_eq!(
            (p.kind, p.sender.as_deref(), p.named.as_deref(), &p.fields["recalled"]),
            (Kind::Recall, sender, named, &Value::Null),
            "{content}"
        );
    }
}

#[test]
fn a_recall_has_the_id_of_the_message_recalled_when_wechat_keeps_it() {
    let pushed = r#"<sysmsg type="revokemsg"><revokemsg><session>1@chatroom</session><oldmsgid>12</oldmsgid><msgid>34</msgid><newmsgid>8967088367583248018</newmsgid><replacemsg><![CDATA["Ann" 撤回了一条消息]]></replacemsg><content><![CDATA["Ann" 撤回了一条消息]]></content></revokemsg></sysmsg>"#;
    let p = parse("wxid_me", SYSTEM_XML, pushed, &[]);
    assert_eq!(p.fields["recalled"], "8967088367583248018");
    // Its text is the `<replacemsg>` when it has no `<content>`.
    let replaced = pushed.replace(r#"<content><![CDATA["Ann" 撤回了一条消息]]></content>"#, "");
    let p = parse("wxid_me", SYSTEM_XML, &replaced, &[]);
    assert_eq!(
        (p.kind, &p.fields["text"], p.named.as_deref()),
        (Kind::Recall, &json!("\"Ann\" 撤回了一条消息"), Some("Ann"))
    );
    // 0 names no message.
    let zero = pushed.replace("8967088367583248018", "0");
    assert_eq!(parse("wxid_me", SYSTEM_XML, &zero, &[]).fields["recalled"], Value::Null);
}

#[test]
fn a_recall_is_its_recallers_by_the_name_its_notice_shows_them_by() {
    let mut names = Names::default();
    for (id, remark, nick) in [
        ("wxid_me", None, "Me"),
        ("wxid_ann", None, "Ann"),
        ("wxid_qin", Some("秦俊斌"), "Qin"),
        ("wxid_ben", None, "Ben"),
    ] {
        names.contacts.insert(
            id.into(),
            Contact { remark: remark.map(String::from), nick_name: Some(nick.into()) },
        );
    }
    names.in_group.insert("wxid_ann".into(), "".into());
    names.in_group.insert("wxid_qin".into(), "".into());
    names.in_group.insert("wxid_ben".into(), "Benny".into());
    names.in_group.insert("wxid_cat".into(), "Ann".into());
    names.in_group.insert("wxid_dan".into(), "".into());
    let recall = |text: &str| {
        let mut r = row("message_0", 1, 1, 0);
        r.local_type = SYSTEM;
        r.sender = None;
        r.content = format!(
            r#"<?xml version="1.0"?><sysmsg type="revokemsg"><revokemsg><content>{text}</content><revoketime>0</revoketime></revokemsg></sysmsg>"#
        )
        .into_bytes();
        r
    };
    let said = |chat_id: &str, text: &str| {
        let m = message(&chat(chat_id), &names, &recall(text), &none);
        assert_eq!(m["kind"], "recall", "{m}");
        (m["sender"]["id"].clone(), m["sender"]["display_name"].clone())
    };
    // By the remark, the name in the group or the own name of a member.
    assert_eq!(
        said("1@chatroom", "\"秦俊斌\" 撤回了一条消息"),
        (json!("wxid_qin"), json!("秦俊斌"))
    );
    assert_eq!(said("1@chatroom", "\"Qin\" 撤回了一条消息"), (json!("wxid_qin"), json!("秦俊斌")));
    assert_eq!(said("1@chatroom", "\"Benny\" 撤回了一条消息"), (json!("wxid_ben"), json!("Benny")));
    assert_eq!(said("1@chatroom", "\"Ben\" 撤回了一条消息"), (json!("wxid_ben"), json!("Benny")));
    // A name two members go by, or none does, names nobody.
    assert_eq!(said("1@chatroom", "\"Ann\" 撤回了一条消息"), (Value::Null, Value::Null));
    assert_eq!(said("1@chatroom", "\"Eve\" 撤回了一条消息"), (Value::Null, Value::Null));
    // The account's own, in a group or a direct chat.
    assert_eq!(said("1@chatroom", "你撤回了一条消息"), (json!("wxid_me"), json!("Me")));
    assert_eq!(said("wxid_ann", "You recalled a message."), (json!("wxid_me"), json!("Me")));
    // In a direct chat, anyone's but the account's is the other's.
    assert_eq!(said("wxid_ann", "\"Annie\" 撤回了一条消息"), (json!("wxid_ann"), json!("Ann")));
}

#[test]
fn a_nudge_has_who_nudged_whom() {
    let pat = r#"<msg><appmsg><title>"Ann" 拍了拍 我</title><type>62</type><patinfo><fromusername>wxid_ann</fromusername><chatusername>wxid_ann</chatusername><pattedusername>wxid_me</pattedusername></patinfo></appmsg></msg>"#;
    assert_eq!(
        read(app(62), pat),
        json!({ "kind": "nudge", "nudge": { "from": { "id": "wxid_ann", "display_name": "" }, "to": { "id": "wxid_me", "display_name": "" }, "text": "\"Ann\" 拍了拍 我" } })
    );
}

/// 接龙 as WeChat for Mac wrote them, taken from an account's records with
/// the people and their words made up and the ranges counted again: a
/// 接龙's seventh entry, and its first message, its solitaire info kept as
/// CDATA; one kept escaped, with an example line; and one written by
/// another client, its parts in another order and with no solitaire id.
const SOLITAIRE: &str = include_str!("fixtures/solitaire.xml");
const SOLITAIRE_STARTED: &str = include_str!("fixtures/solitaire_started.xml");
const SOLITAIRE_ESCAPED: &str = include_str!("fixtures/solitaire_escaped_with_example.xml");
const SOLITAIRE_WITHOUT_ID: &str = include_str!("fixtures/solitaire_without_id.xml");

/// Someone a 接龙 names, as the answer has them before the chat names them.
fn person(id: &str) -> Value {
    json!({ "id": id, "display_name": "" })
}

/// An entry of a 接龙 as the answer has it before its member is named.
fn entry(number: usize, member: &str, text: &str, at: &str) -> Value {
    json!({ "number": number, "member": person(member), "text": text, "at": at })
}

#[test]
fn a_solitaire_has_its_title_and_its_entries_in_order() {
    let shown = "#接龙\n周四10:30莫干山\n\n1. 🐝Ann\n2. Bob\n3. 🐯Cat  Dan去才去\n4. Eve 你去我就去\n5. 🐔Fay +2\n6. Gus 再带一个就去\n7. Hal 🌺 在杭州就来";
    assert_eq!(
        read(app(53), SOLITAIRE),
        json!({ "kind": "solitaire", "solitaire": {
            "id": "1792387959382_6313",
            "title": "周四10:30莫干山",
            "example": null,
            "starter": person("wxid_ann"),
            "entries": [
                entry(1, "wxid_ann", "🐝Ann", "2026-10-19T05:32:29Z"),
                entry(2, "wxid_bob", "Bob", "2026-10-19T06:00:10Z"),
                entry(3, "wxid_cat", "🐯Cat  Dan去才去", "2026-10-19T06:04:00Z"),
                entry(4, "wxid_dan", "Eve 你去我就去", "2026-10-19T06:10:08Z"),
                entry(5, "wxid_eve", "🐔Fay +2", "2026-10-19T06:10:19Z"),
                entry(6, "wxid_fay", "Gus 再带一个就去", "2026-10-19T06:23:54Z"),
                entry(7, "wxid_gus", "Hal 🌺 在杭州就来", "2026-10-19T06:32:56Z"),
            ],
            "text": shown,
        } })
    );
    // The subtype from <type> when local_type has none.
    assert_eq!(read(APP, SOLITAIRE)["kind"], "solitaire");
}

#[test]
fn each_message_of_one_solitaire_has_its_id() {
    let started = read(app(53), SOLITAIRE_STARTED);
    let later = read(app(53), SOLITAIRE);
    assert_eq!(started["solitaire"]["id"], later["solitaire"]["id"]);
    assert_eq!(
        started["solitaire"]["entries"],
        json!([entry(1, "wxid_ann", "🐝Ann", "2026-10-19T05:32:29Z")])
    );
    // A record that holds no id has none, and is read all the same.
    let without = read(app(53), SOLITAIRE_WITHOUT_ID);
    assert_eq!(
        without["solitaire"],
        json!({
            "id": null,
            "title": "明早9点多龙井见",
            "example": null,
            "starter": person("wxid_ann"),
            "entries": [
                entry(1, "wxid_ann", "🐝Ann", "2026-10-13T05:21:44Z"),
                entry(2, "wxid_bob", "Bob 屁股", "2026-10-13T05:22:53Z"),
            ],
            "text": "#接龙\n明早9点多龙井见\n\n1. 🐝Ann\n2. Bob 屁股",
        })
    );
}

#[test]
fn a_solitaire_kept_escaped_reads_with_its_example() {
    let s = &read(app(53), SOLITAIRE_ESCAPED)["solitaire"];
    assert_eq!(
        s["title"],
        "9.12 14:00-17:00 新手专场的自由对弈(Casual game)\n\nAfterglowCoffee(拱墅区42号-6);"
    );
    assert_eq!(s["example"], "例 名字+分数");
    assert_eq!(s["id"], Value::Null);
    assert_eq!(
        s["entries"],
        json!([
            entry(1, "wxid_ann", "Ann", "2026-09-10T08:00:03Z"),
            entry(2, "wxid_bob", "____ （折后 ELO 600）", "2026-09-10T08:02:22Z"),
            entry(3, "wxid_cat", "Cat-（很久没下棋了 elo没概念了）", "2026-09-10T10:27:04Z"),
            entry(4, "wxid_dan", "Dan （国象联盟1000）", "2026-09-12T00:03:14Z"),
        ])
    );
}

#[test]
fn an_entry_whose_range_is_not_in_the_title_has_no_text() {
    let s = |info: &str| {
        let xml = format!(
            "<msg><appmsg><title>#接龙\nT\n\n1. a</title><type>53</type><extinfo><solitaire_info><![CDATA[<solitaire>{info}</solitaire>]]></solitaire_info></extinfo></appmsg></msg>"
        );
        read(app(53), &xml)
    };
    let one = |r: &str| {
        s(&format!("<tt>0-5</tt><content><i><u>wxid_ann</u><t>x</t><r>{r}</r></i></content>"))
    };
    assert_eq!(one("10-1")["solitaire"]["entries"][0]["text"], "a");
    for r in ["10-2", "99-1", "a-1", "10", "18446744073709551615-2", ""] {
        assert_eq!(
            one(r)["solitaire"]["entries"],
            json!([{ "number": 1, "member": person("wxid_ann"), "text": null, "at": null }]),
            "{r}"
        );
    }
    // A title that is not WeChat's tag alone; no title at all.
    assert_eq!(one("10-1")["solitaire"]["title"], "T");
    assert_eq!(s("<content></content>")["solitaire"]["title"], Value::Null);
    assert_eq!(s("<content></content>")["solitaire"]["entries"], json!([]));
    // Solitaire info with no entries list at all is not one.
    assert_eq!(s("<tt>0-5</tt>")["kind"], "unknown");
}

#[test]
fn a_solitaires_starter_and_members_are_named_as_wechat_shows_them() {
    let mut names = Names::default();
    names.contacts.insert(
        "wxid_ann".into(),
        Contact { remark: Some("Annie".into()), nick_name: Some("Ann".into()) },
    );
    names
        .contacts
        .insert("wxid_bob".into(), Contact { remark: None, nick_name: Some("Bob".into()) });
    names.in_group.insert("wxid_bob".into(), "Bobby".into());
    let mut r = row("message_0", 1, 1_792_391_576, 0);
    r.local_type = app(53);
    r.sender = Some("wxid_bob".into());
    r.content = format!("wxid_bob:\n{SOLITAIRE_WITHOUT_ID}").into_bytes();
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    let s = &m["solitaire"];
    assert_eq!(s["starter"], json!({ "id": "wxid_ann", "display_name": "Annie" }));
    let members: Vec<Value> =
        s["entries"].as_array().unwrap().iter().map(|e| e["member"].clone()).collect();
    assert_eq!(
        members,
        vec![
            json!({ "id": "wxid_ann", "display_name": "Annie" }),
            json!({ "id": "wxid_bob", "display_name": "Bobby" }),
        ]
    );
    // One nothing names is shown by no name.
    let m = message(&chat("1@chatroom"), &Names::default(), &r, &none);
    assert_eq!(m["solitaire"]["entries"][0]["member"], person("wxid_ann"));
    assert_eq!(m["solitaire"]["starter"], person("wxid_ann"));
}

/// A template notice `template`, its `links`.
pub fn notice(template: &str, links: &str) -> String {
    format!(
        r#"<sysmsg type="sysmsgtemplate"><sysmsgtemplate><content_template type="tmpl_type_profile"><plain></plain><template><![CDATA[{template}]]></template><link_list>{links}</link_list></content_template></sysmsgtemplate></sysmsg>"#
    )
}

/// A link named `name` listing `who`, each `(id, name)`.
pub fn members(name: &str, who: &[(&str, &str)]) -> String {
    let m: String = who
        .iter()
        .map(|(u, n)| format!("<member><username><![CDATA[{u}]]></username><nickname><![CDATA[{n}]]></nickname></member>"))
        .collect();
    format!(
        r#"<link name="{name}" type="link_profile"><memberlist>{m}</memberlist><separator><![CDATA[、]]></separator></link>"#
    )
}

#[test]
fn a_join_has_who_joined_and_by_whom() {
    let invited = notice(
        r#""$username$"邀请"$names$"加入了群聊"#,
        &(members("username", &[("wxid_ann", "Ann")])
            + &members("names", &[("wxid_ben", "Ben"), ("wxid_cat", "Cat")])),
    );
    assert_eq!(
        read(SYSTEM_XML, &invited),
        json!({ "kind": "join", "join": { "contacts": [{ "id": "wxid_ben", "display_name": "Ben" }, { "id": "wxid_cat", "display_name": "Cat" }], "by": { "id": "wxid_ann", "display_name": "Ann" }, "text": "\"Ann\"邀请\"Ben、Cat\"加入了群聊" } })
    );
    let by_qr = notice(
        r#""$adder$"通过扫描"$from$"分享的二维码加入群聊"#,
        &(members("adder", &[("wxid_dan", "Dan")]) + &members("from", &[("wxid_ann", "Ann")])),
    );
    assert_eq!(read(SYSTEM, &by_qr)["join"]["contacts"][0]["id"], "wxid_dan");
    assert_eq!(read(SYSTEM, &by_qr)["join"]["by"]["id"], "wxid_ann");
    let me_too = notice(
        r#""$username$"邀请你和"$others$"加入了群聊"#,
        &(members("username", &[("wxid_ann", "Ann")]) + &members("others", &[("wxid_ben", "Ben")])),
    );
    let joined = read(SYSTEM, &me_too)["join"]["contacts"].clone();
    assert_eq!(
        joined,
        json!([{ "id": "wxid_me", "display_name": "" }, { "id": "wxid_ben", "display_name": "Ben" }])
    );
    let by_me = notice(r#"你邀请"$names$"加入了群聊"#, &members("names", &[("wxid_ben", "Ben")]));
    assert_eq!(read(SYSTEM, &by_me)["join"]["by"]["id"], "wxid_me");
}

#[test]
fn a_removal_is_a_leave_and_a_rename_has_the_new_name() {
    let removed =
        notice(r#"你将"$kickoutname$"移出了群聊"#, &members("kickoutname", &[("wxid_ben", "Ben")]));
    assert_eq!(
        read(SYSTEM, &removed),
        json!({ "kind": "leave", "leave": { "contacts": [{ "id": "wxid_ben", "display_name": "Ben" }], "by": { "id": "wxid_me", "display_name": "" }, "text": "你将\"Ben\"移出了群聊" } })
    );
    let renamed = notice(
        r#""$username$"修改群名为“$remark$”"#,
        &(members("username", &[("wxid_ann", "Ann")])
            + r#"<link name="remark" type="link_plain"><plain><![CDATA[Crag Rats]]></plain></link>"#),
    );
    assert_eq!(
        read(SYSTEM, &renamed),
        json!({ "kind": "renamed", "renamed": { "name": "Crag Rats", "by": { "id": "wxid_ann", "display_name": "Ann" }, "text": "\"Ann\"修改群名为“Crag Rats”" } })
    );
}

#[test]
fn another_notice_is_a_system_notice_with_its_text() {
    assert_eq!(
        read(SYSTEM, "Ann is now your friend"),
        json!({ "kind": "system", "system": { "text": "Ann is now your friend" } })
    );
    let pinned =
        notice(r#""$username$"置顶了一条消息"#, &members("username", &[("wxid_ann", "Ann")]));
    assert_eq!(
        read(SYSTEM, &pinned),
        json!({ "kind": "system", "system": { "text": "\"Ann\"置顶了一条消息" } })
    );
}

#[test]
fn a_transfer_has_its_amount_in_fen_state_and_memo() {
    let t = |sub: &str, fee: &str| {
        format!(
            "<msg><appmsg><title>微信转账</title><type>2000</type><wcpayinfo><paysubtype>{sub}</paysubtype><feedesc><![CDATA[{fee}]]></feedesc><pay_memo><![CDATA[lunch]]></pay_memo></wcpayinfo></appmsg></msg>"
        )
    };
    assert_eq!(
        read(app(2000), &t("1", "￥0.01")),
        json!({ "kind": "transfer", "transfer": { "amount_fen": 1, "state": "sent", "memo": "lunch" } })
    );
    assert_eq!(read(app(2000), &t("3", "￥12.5"))["transfer"]["amount_fen"], 1250);
    assert_eq!(read(app(2000), &t("3", "￥12.5"))["transfer"]["state"], "received");
    assert_eq!(read(app(2000), &t("4", "¥1,000.00"))["transfer"]["amount_fen"], Value::Null);
    assert_eq!(read(app(2000), &t("4", "¥3"))["transfer"]["state"], "returned");
    assert_eq!(
        read(app(2000), &t("8", "$3"))["transfer"],
        json!({ "amount_fen": null, "state": null, "memo": "lunch" })
    );
}

#[test]
fn a_red_packet_has_its_memo_and_a_collection_is_unknown() {
    let packet = r#"<msg><appmsg><title>微信红包</title><type>2001</type><wcpayinfo><sendertitle>恭喜发财，大吉大利</sendertitle><newaa><billno></billno></newaa></wcpayinfo></appmsg></msg>"#;
    assert_eq!(
        read(app(2001), packet),
        json!({ "kind": "red_packet", "red_packet": { "amount_fen": null, "state": null, "memo": "恭喜发财，大吉大利" } })
    );
    let collection = r#"<msg><appmsg><type>2001</type><wcpayinfo><newaa><billno>1</billno></newaa></wcpayinfo></appmsg></msg>"#;
    assert_eq!(read(app(2001), collection)["kind"], "unknown");
}

/// A call's bubble as WeChat for Mac 4.x keeps it (type 50): what it
/// shows, its room type (0 video, 1 voice) and its `<duration>`.
fn call(shown: &str, room_type: &str, duration: &str) -> String {
    format!(
        "<voipmsg type=\"VoIPBubbleMsg\"><VoIPBubbleMsg><msg><![CDATA[{shown}]]></msg><room_type>{room_type}</room_type><red_dot>false</red_dot><roomid>1393520413</roomid><roomkey>0</roomkey><inviteid>1760936582</inviteid><msg_type>100</msg_type><timestamp>1760940472498</timestamp><identity><![CDATA[6411462536521458416]]></identity><duration>{duration}</duration><inviteid64>1760936582394</inviteid64><business>1</business><caller_memberid>0</caller_memberid><callee_memberid>1</callee_memberid></VoIPBubbleMsg></voipmsg>"
    )
}

#[test]
fn a_connected_call_has_its_medium_and_its_duration() {
    assert_eq!(
        read(50, &call("通话时长 01:04:50", "1", "0")),
        json!({ "kind": "call", "call": { "medium": "voice", "state": "connected", "seconds": 3890, "text": "通话时长 01:04:50" } })
    );
    assert_eq!(
        read(50, &call("通话时长 05:30", "0", "330"))["call"],
        json!({ "medium": "video", "state": "connected", "seconds": 330, "text": "通话时长 05:30" })
    );
    // Its text shows no time it reads: the bubble's own duration says it.
    assert_eq!(
        read(50, &call("Call duration", "1", "42"))["call"],
        json!({ "medium": "voice", "state": "connected", "seconds": 42, "text": "Call duration" })
    );
    // Wrapped in <msg>, as some rows keep it.
    let wrapped = format!("<msg>{}</msg>", call("通话时长 00:07", "1", "7"));
    assert_eq!(read(50, &wrapped)["call"]["seconds"], 7);
}

#[test]
fn a_call_that_did_not_connect_says_why_with_no_duration() {
    for (shown, state) in [
        ("已取消", "cancelled"),
        ("对方已取消", "cancelled"),
        ("已拒绝", "declined"),
        ("对方已拒绝", "declined"),
        ("已在其它设备拒绝", "declined"),
        ("对方无应答", "missed"),
        ("未应答", "missed"),
        ("未接听", "missed"),
        ("忙线未接听", "missed"),
    ] {
        assert_eq!(
            read(50, &call(shown, "1", "0"))["call"],
            json!({ "medium": "voice", "state": state, "seconds": null, "text": shown }),
            "{shown}"
        );
    }
    // What says none of them is a call all the same, its text kept.
    assert_eq!(
        read(50, &call("已在其它设备接听", "2", "0"))["call"],
        json!({ "medium": null, "state": null, "seconds": null, "text": "已在其它设备接听" })
    );
}

#[test]
fn a_calls_duration_is_read_as_wechat_shows_it() {
    use crate::kinds::call_seconds;
    for (shown, seconds) in [
        ("通话时长 01:04:50", Some(3890)),
        ("通话时长 00:31", Some(31)),
        ("通话时长 1:02", Some(62)),
        ("通话时长 100:00:00", None),
        ("通话时长 00:60", None),
        ("通话时长 01:61:00", None),
        ("通话时长 ", None),
        ("通话时长 1:2:3:4", None),
        ("通话时长 a:00", None),
        ("已取消", None),
    ] {
        assert_eq!(call_seconds(shown), seconds, "{shown}");
    }
}

/// A group's notice posted into its chat, as WeChat for Mac 4.x kept a
/// real one (type 49, subtype 87): its ids, keys, digests and place
/// replaced, the rest as it was.
const GROUP_NOTICE: &str = include_str!("../fixtures/group_notice.xml");

/// The text of [`GROUP_NOTICE`], as its `<textannouncement>` holds it.
fn group_notice_text() -> String {
    let (_, after) = GROUP_NOTICE.split_once("<textannouncement><![CDATA[").unwrap();
    after.split_once("]]>").unwrap().0.to_string()
}

#[test]
fn a_group_notice_has_its_text_its_poster_and_its_time() {
    let text = group_notice_text();
    assert!(text.starts_with("🍃【枫林苑・秋日便民活动通知】🍃\n秋意渐浓"), "{text}");
    assert_eq!(
        read(app(87), GROUP_NOTICE),
        json!({ "kind": "group_notice", "group_notice": {
            "text": text,
            "by": { "id": "wxid_steward", "display_name": "" },
            "at": "2026-10-16T06:55:31Z",
        } })
    );
    // Its subtype read from its XML, as WeChat stores some.
    assert_eq!(read(APP, GROUP_NOTICE)["kind"], "group_notice");
}

#[test]
fn a_group_notice_without_its_text_or_its_poster_reads_what_it_holds() {
    // Another shape WeChat writes: no type, its source after its items,
    // and no <textannouncement>: its text is its items' text.
    let item = r#"<group_notice_item><datalist count="3"><dataitem datatype="8" htmlid="WeNoteHtmlFile"><datafmt>.htm</datafmt></dataitem><dataitem datatype="1" htmlid="-1"><datadesc>Ride at 9.</datadesc></dataitem><dataitem datatype="1" htmlid="-1"><datadesc>Bring water.</datadesc></dataitem></datalist><source><tousr>1@chatroom</tousr><fromusr>wxid_ann</fromusr></source><edittime>1792221368</edittime></group_notice_item>"#;
    let notice = format!(
        "<msg><appmsg><type>87</type><announcement><![CDATA[{item}]]></announcement></appmsg><fromusername><![CDATA[wxid_bob]]></fromusername></msg>"
    );
    assert_eq!(
        read(app(87), &notice)["group_notice"],
        json!({ "text": "Ride at 9.\nBring water.", "by": { "id": "wxid_ann", "display_name": "" }, "at": "2026-10-17T07:16:08Z" })
    );
    // An announcement that does not read: its text alone, posted by who
    // the message says sent it, at no time it says.
    let bare = "<msg><appmsg><type>87</type><announcement><![CDATA[<group_notice_item>]]></announcement><textannouncement><![CDATA[Ride at 9.]]></textannouncement></appmsg><fromusername>wxid_bob</fromusername></msg>";
    assert_eq!(
        read(app(87), bare)["group_notice"],
        json!({ "text": "Ride at 9.", "by": { "id": "wxid_bob", "display_name": "" }, "at": null })
    );
    // One that says nothing is still a notice, its text empty.
    assert_eq!(
        read(app(87), "<msg><appmsg><type>87</type></appmsg></msg>")["group_notice"],
        json!({ "text": "", "by": null, "at": null })
    );
}

#[test]
fn a_group_notices_poster_is_shown_as_wechat_shows_them() {
    let mut names = Names::default();
    names
        .contacts
        .insert("wxid_steward".into(), Contact { remark: None, nick_name: Some("Steward".into()) });
    names.in_group.insert("wxid_steward".into(), "物业管家".into());
    let mut r = row("message_0", 1, 1_792_133_732, 0);
    r.local_type = app(87);
    r.sender = Some("wxid_steward".into());
    r.content = format!("wxid_steward:\n{GROUP_NOTICE}").into_bytes();
    let m = message(&chat("12345678901@chatroom"), &names, &r, &none);
    assert_eq!(
        (&m["kind"], &m["sender"]),
        (&json!("group_notice"), &json!({ "id": "wxid_steward", "display_name": "物业管家" }))
    );
    assert_eq!(
        m["group_notice"],
        json!({
            "text": group_notice_text(),
            "by": { "id": "wxid_steward", "display_name": "物业管家" },
            "at": "2026-10-16T06:55:31Z",
        })
    );
    // One whose XML names no poster or time is its sender's, when it was
    // sent.
    r.content = b"wxid_steward:\n<msg><appmsg><type>87</type></appmsg></msg>".to_vec();
    let m = message(&chat("12345678901@chatroom"), &names, &r, &none);
    assert_eq!(
        m["group_notice"],
        json!({
            "text": "",
            "by": { "id": "wxid_steward", "display_name": "物业管家" },
            "at": "2026-10-16T06:55:32Z",
        })
    );
}

#[test]
fn fen_reads_yuan_as_wechat_shows_them() {
    assert_eq!(kinds::fen(Some("￥0.01")), Some(1));
    assert_eq!(kinds::fen(Some(" ¥ 200 ")), Some(20_000));
    assert_eq!(kinds::fen(Some("￥1.234")), None);
    assert_eq!(kinds::fen(Some("￥")), None);
    assert_eq!(kinds::fen(Some("￥99999999999999999999")), None);
    assert_eq!(kinds::fen(None), None);
}

fn chat(id: &str) -> Chat {
    Chat { id: id.into(), account: "wxid_me".into() }
}

pub fn row(db: &str, local_id: i64, create_time: i64, sort_seq: i64) -> Row {
    Row {
        db: db.into(),
        local_id,
        server_id: local_id + 1000,
        local_type: TEXT,
        sort_seq,
        create_time,
        sender: Some("wxid_ann".into()),
        content: format!("m{local_id}").into_bytes(),
        compressed: None,
        packed: Vec::new(),
    }
}

/// What a test that holds no file says of every media message.
fn none(_: &Media, _: &Row) -> Held {
    Held::default()
}

#[test]
fn a_media_message_says_whether_wechat_holds_its_file_and_thumbnail() {
    let mut image = row("message_0", 1, 1_760_000_000, 0);
    image.local_type = 3;
    image.content = b"<msg><img md5=\"m\"/></msg>".to_vec();
    let held = |m: &Media, r: &Row| {
        assert_eq!((m, r.local_id), (&Media::Image { name: None }, 1));
        Held { file: true, thumbnail: Some(false) }
    };
    let m = message(&chat("wxid_ann"), &Names::default(), &image, &held);
    assert_eq!(
        m["image"],
        json!({ "name": null, "md5": "m", "file_present": true, "thumbnail_present": false })
    );
    let mut voice = image.clone();
    voice.local_type = 34;
    let m = message(&chat("wxid_ann"), &Names::default(), &voice, &none);
    assert_eq!(
        m["voice"],
        json!({ "length_ms": null, "transcription": null, "file_present": false, "thumbnail_present": null })
    );
    let text = row("message_0", 2, 1, 0);
    assert_eq!(
        message(&chat("wxid_ann"), &Names::default(), &text, &none)["text"],
        json!({ "text": "m2" })
    );
}

#[test]
fn a_message_names_its_file_or_its_kind_when_it_carries_none() {
    let mut r = row("message_0", 1, 1, 0);
    assert_eq!(media_of(&chat("wxid_ann"), &r), Err(Kind::Text));
    r.local_type = 49 | 6 << 32;
    r.content =
        b"wxid_bob:\n<msg><appmsg><title>a.pdf</title><type>6</type></appmsg></msg>".to_vec();
    r.sender = None;
    assert_eq!(
        media_of(&chat("1@chatroom"), &r),
        Ok(Media::File { name: Some("a.pdf".into()), size: None })
    );
}

#[test]
fn a_message_carries_its_id_time_sender_kind_content_and_type() {
    let mut names = Names::default();
    names
        .contacts
        .insert("wxid_ann".into(), Contact { remark: None, nick_name: Some("Ann".into()) });
    let m = message(&chat("wxid_ann"), &names, &row("message_0", 1, 1_760_000_000, 0), &none);
    assert_eq!(
        m,
        json!({
            "id": "1001", "at": "2025-10-09T08:53:20Z", "sender": { "id": "wxid_ann", "display_name": "Ann" },
            "kind": "text", "text": { "text": "m1" }, "type": 1, "subtype": 0, "content": "m1",
        })
    );
    let mut local = row("message_0", 1, 0, 0);
    local.server_id = 0;
    local.sender = None;
    let m = message(&chat("wxid_ann"), &names, &local, &none);
    assert_eq!((&m["id"], &m["at"], &m["sender"]), (&Value::Null, &Value::Null, &Value::Null));
}

#[test]
fn a_group_message_drops_its_sender_prefix() {
    let names = Names::default();
    let mut r = row("message_0", 1, 1, 0);
    r.content = b"wxid_ann:\nhello:\nthere".to_vec();
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"]["id"], &m["content"]), (&json!("wxid_ann"), &json!("hello:\nthere")));
    r.sender = None;
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"]["id"], &m["content"]), (&json!("wxid_ann"), &json!("hello:\nthere")));
    // A direct chat's content is its own, whatever it starts with.
    r.sender = Some("wxid_ann".into());
    let m = message(&chat("wxid_ann"), &names, &r, &none);
    assert_eq!(m["content"], "wxid_ann:\nhello:\nthere");
    // The account's own message in a group has no prefix.
    r.sender = Some("wxid_me".into());
    r.content = b"hi all".to_vec();
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"]["id"], &m["content"]), (&json!("wxid_me"), &json!("hi all")));
}

#[test]
fn zstd_content_is_decompressed() {
    let mut r = row("message_0", 1, 1, 0);
    r.local_type = app(5);
    let xml =
        "<msg><appmsg><title>Topo</title><type>5</type><url>https://e.com</url></appmsg></msg>";
    r.content = ruzstd::encoding::compress_to_vec(
        xml.as_bytes(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    r.compressed = Some(wxread_schema::ZSTD);
    let m = message(&chat("wxid_ann"), &Names::default(), &r, &none);
    assert_eq!((&m["kind"], &m["content"]), (&json!("link"), &json!(xml)));
    assert_eq!(m["subtype"], 5);
    // Bytes that do not decompress are kept as they are.
    r.content = b"plain".to_vec();
    assert_eq!(message(&chat("wxid_ann"), &Names::default(), &r, &none)["content"], "plain");
}

#[test]
fn a_range_is_read_from_its_times_and_refused_with_its_fix() {
    assert_eq!(Range::parse(None, None), Ok(Range::default()));
    assert_eq!(
        Range::parse(Some("2025-10-09"), Some("2025-10-09T08:53:20Z")),
        Ok(Range { from: Some(1_759_968_000), to: Some(1_760_000_000) })
    );
    let why = Range::parse(Some("yesterday"), None).unwrap_err();
    assert!(
        why.starts_with("--from yesterday is not a time") && why.contains("2026-10-20"),
        "{why}"
    );
    let why = Range::parse(None, Some("1760000000")).unwrap_err();
    assert!(why.starts_with("--to 1760000000 is not a time"), "{why}");
    let why = Range::parse(Some("2025-10-10"), Some("2025-10-09")).unwrap_err();
    assert!(why.contains("is not before --to"), "{why}");
}

#[test]
fn a_cursor_wxread_did_not_write_is_refused_with_its_fix() {
    for text in [
        "",
        "x",
        "1.2.message_0",
        "a.2.message_0.4",
        "1.2..4",
        "1.2.message_0.4.5",
        "oldest.1.2.message_0",
        "newest.1.2.message_0.4",
    ] {
        for order in [Order::NewestFirst, Order::OldestFirst] {
            let why = Cursor::parse(text, order).unwrap_err();
            assert!(why.contains("leave --cursor out"), "{text}: {why}");
        }
    }
    let c = Cursor::parse("10.-3.message_0.4", Order::NewestFirst).unwrap();
    assert_eq!(c.text(), "10.-3.message_0.4");
    assert_eq!(c.order(), Order::NewestFirst);
    assert_eq!(c.bound_in("message_0"), (10, -3, 4));
    assert_eq!(c.bound_in("biz_message_0"), (10, -3, i64::MAX));
    assert_eq!(c.bound_in("message_1"), (10, -3, i64::MIN));
    let c = Cursor::parse("oldest.10.-3.message_0.4", Order::OldestFirst).unwrap();
    assert_eq!(c.text(), "oldest.10.-3.message_0.4");
    assert_eq!(c.order(), Order::OldestFirst);
    assert_eq!(c.bound_in("message_0"), (10, -3, 4));
}

#[test]
fn a_cursor_of_one_order_given_with_the_other_is_refused_with_its_fix() {
    let why = Cursor::parse("oldest.10.-3.message_0.4", Order::NewestFirst).unwrap_err();
    assert!(
        why.contains("a `next` of an --oldest-first read") && why.contains("pass --oldest-first"),
        "{why}"
    );
    let why = Cursor::parse("10.-3.message_0.4", Order::OldestFirst).unwrap_err();
    assert!(
        why.contains("a `next` of a newest-first read") && why.contains("leave --oldest-first out"),
        "{why}"
    );
}

#[test]
fn the_message_databases_are_picked_and_ordered() {
    let all: Vec<String> = [
        "message/message_10.db",
        "message/biz_message_0.db",
        "message/message_2.db",
        "message/message_fts.db",
        "message/media_0.db",
        "message/message_resource.db",
        "session/session.db",
        "message/message_.db",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(message_dbs(&all), ["message_2", "message_10", "biz_message_0"]);
    assert_eq!(db_path("message_2"), "message/message_2.db");
}

fn ids(page: &Page) -> Vec<i64> {
    page.messages.iter().map(|m| m["id"].as_str().unwrap().parse::<i64>().unwrap() - 1000).collect()
}

#[test]
fn messages_are_newest_first_and_by_their_order_in_one_second() {
    let rows = vec![
        row("message_0", 1, 100, 100_000),
        row("message_0", 2, 100, 100_002),
        row("message_0", 3, 100, 100_001),
        row("message_1", 1, 200, 0),
        row("message_0", 9, 50, 0),
    ];
    let all = page(
        &chat("wxid_ann"),
        &Names::default(),
        rows.clone(),
        Range::default(),
        Start::First(Order::NewestFirst),
        Limit::default_limit(),
        none,
    );
    assert_eq!(ids(&all), [1, 2, 3, 1, 9]);
    assert_eq!(all.messages[0]["at"], rfc3339(200).unwrap());
    assert_eq!(all.next, None);
    let range = Range { from: Some(100), to: Some(200) };
    let first = Start::First(Order::NewestFirst);
    let some = page(
        &chat("wxid_ann"),
        &Names::default(),
        rows,
        range,
        first,
        Limit::default_limit(),
        none,
    );
    assert_eq!(ids(&some), [2, 3, 1]);
}

#[test]
fn with_oldest_first_messages_are_oldest_first_and_by_their_order_in_one_second() {
    let rows = vec![
        row("message_0", 1, 100, 100_000),
        row("message_0", 2, 100, 100_002),
        row("message_0", 3, 100, 100_001),
        row("message_1", 1, 200, 0),
        row("message_0", 9, 50, 0),
    ];
    let first = Start::First(Order::OldestFirst);
    let read = |range: Range, limit: usize| {
        let limit = Limit::new(limit).unwrap();
        page(&chat("wxid_ann"), &Names::default(), rows.clone(), range, first, limit, none)
    };
    let all = read(Range::default(), 100);
    assert_eq!(ids(&all), [9, 1, 3, 2, 1]);
    assert_eq!(all.messages[0]["at"], rfc3339(50).unwrap());
    assert_eq!(all.next, None);
    assert_eq!(ids(&read(Range { from: Some(100), to: Some(200) }, 100)), [1, 3, 2]);
    let two = read(Range::default(), 2);
    let next = two.next.unwrap();
    assert_eq!(next.text(), "oldest.100.100000.message_0.1");
    let after = Cursor::parse(&next.text(), Order::OldestFirst).unwrap();
    let rest = page(
        &chat("wxid_ann"),
        &Names::default(),
        rows.clone(),
        Range::default(),
        Start::After(&after),
        Limit::default_limit(),
        none,
    );
    assert_eq!(ids(&rest), [3, 2, 1]);
}

proptest! {
    #[test]
    fn paging_with_next_returns_each_message_once_in_order(
        rows in proptest::collection::vec((0usize..3, 0i64..3, 0i64..3), 0..40),
        limit in 1usize..7,
        oldest_first in any::<bool>(),
    ) {
        // Many messages in one second, some of one order in it, across
        // databases: each is still on one page, in its place, either way.
        let order = if oldest_first { Order::OldestFirst } else { Order::NewestFirst };
        let rows: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, &(db, at, seq))| row(&format!("message_{db}"), i as i64, at, seq))
            .collect();
        let read = |start: Start<'_>, limit: usize| {
            page(&chat("wxid_ann"), &Names::default(), rows.clone(), Range::default(), start, Limit::new(limit).unwrap(), none)
        };
        let whole = read(Start::First(order), wxread_answer::MAX_LIMIT);
        prop_assert_eq!(whole.messages.len(), rows.len());
        let mut reversed = read(Start::First(order.reverse()), wxread_answer::MAX_LIMIT).messages;
        reversed.reverse();
        prop_assert_eq!(&reversed, &whole.messages);
        let mut paged = Vec::new();
        let mut after: Option<Cursor> = None;
        loop {
            let p = read(after.as_ref().map_or(Start::First(order), Start::After), limit);
            prop_assert!(p.messages.len() <= limit);
            paged.extend(p.messages);
            match p.next {
                Some(next) => after = Some(Cursor::parse(&next.text(), order).unwrap()),
                None => break,
            }
        }
        prop_assert_eq!(paged, whole.messages);
    }

    #[test]
    fn whatever_the_content_a_message_is_read(local_type in any::<i64>(), content in ".{0,80}", packed in proptest::collection::vec(any::<u8>(), 0..40)) {
        let p = parse("wxid_me", local_type, &content, &packed);
        prop_assert!(Kind::ALL.contains(&p.kind));
    }
}

/// A note sent into a group, as WeChat for Mac 4.1 kept it: its body, a
/// paragraph, a place and an image, the CDN's addresses and keys and the
/// sender's id taken out.
const NOTE: &str = include_str!("fixtures/note.xml");
/// Its `packed_info_data`: the note's directory, `58d452fe52a3cf23`, in
/// field 9's field 1.
const NOTE_PACKED: &str = "08F70110014A120A10353864343532666535326133636632335800";
const NOTE_ID: i64 = 1_857_813_742_130_961_658;

fn note_row() -> Row {
    let packed = (0..NOTE_PACKED.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&NOTE_PACKED[i..i + 2], 16).unwrap())
        .collect();
    Row {
        server_id: NOTE_ID,
        local_type: app(24),
        sender: Some("wxid_amy".into()),
        content: NOTE.as_bytes().to_vec(),
        packed,
        ..row("message_0", 902, 1_792_391_734, 0)
    }
}

/// The note's items as `wxread favorites` answers a note's, each media
/// one's file as `file` says it: its id, and whether WeChat holds it and
/// its thumbnail.
fn note_items(file: impl Fn(&str) -> (Value, Value, Value)) -> Value {
    let (body_id, body, _) = file("58f6f1ad9a8dc9939d7948311046641e");
    let (image_id, image, thumb) = file("084b81e4ceaa08076026a325299e26fe");
    let said = |kind: &str, t: i64, fields: Value| json!({ "kind": kind, kind: fields, "sender": null, "sender_name": null, "at": null, "type": t });
    json!([
        said(
            "file",
            8,
            json!({ "file_id": body_id, "size": 299, "name": null, "extension": "htm", "file_present": body, "thumbnail_present": null })
        ),
        said("text", 1, json!({ "text": "10 ：30 导航到这里\n杭州九点 20 左右出发" })),
        said(
            "location",
            6,
            json!({ "latitude": 30.59977149963379, "longitude": 119.8946685791016, "name": "莫干山庾村广场P4停车场", "address": "浙江省湖州市德清县大乐之野莫干山(庾村店)南" })
        ),
        said(
            "image",
            2,
            json!({ "file_id": image_id, "size": 360153, "file_present": image, "thumbnail_present": thumb })
        ),
    ])
}

#[test]
fn a_note_is_its_items_as_a_favorites_note_is() {
    let unlooked = |_: &str| (Value::Null, Value::Null, Value::Null);
    assert_eq!(
        read(app(24), NOTE),
        json!({ "kind": "note", "note": { "items": note_items(unlooked) } })
    );
    // The subtype from <type> when local_type has none, and the record
    // kept as CDATA rather than escaped.
    assert_eq!(read(APP, NOTE)["kind"], "note");
    let record = NOTE.split_once("<recorditem>").unwrap().1.split_once("</recorditem>").unwrap().0;
    let unescaped = record.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    let cdata = NOTE.replace(record, &format!("<![CDATA[{unescaped}]]>"));
    assert_eq!(read(app(24), &cdata), read(app(24), NOTE));
    // A note whose record does not read is unknown, its content kept.
    let broken =
        "<msg><appmsg><type>24</type><recorditem>&lt;recordinfo</recorditem></appmsg></msg>";
    assert_eq!(read(app(24), broken)["kind"], "unknown");
}

#[test]
fn a_notes_items_name_their_files_by_its_message_and_say_whether_wechat_holds_them() {
    let rec = Some("58d452fe52a3cf23".to_string());
    let held = |m: &Media, r: &Row| {
        assert_eq!(r.local_id, 902);
        match m {
            Media::NoteImage { rec: r, place: 3 } if *r == rec => {
                Held { file: true, thumbnail: Some(false) }
            }
            Media::NoteFile { rec: r, place: 0, name: None } if *r == rec => Held::default(),
            other => panic!("{other:?}"),
        }
    };
    let m = message(&chat("48614902608@chatroom"), &Names::default(), &note_row(), &held);
    let present = |item: &str| {
        let id = json!(format!("note:{NOTE_ID}:{item}"));
        if item.starts_with("084b") {
            (id, json!(true), json!(false))
        } else {
            (id, json!(false), Value::Null)
        }
    };
    assert_eq!(
        (&m["kind"], &m["note"]),
        (&json!("note"), &json!({ "items": note_items(present) }))
    );
    assert_eq!((&m["type"], &m["subtype"]), (&json!(49), &json!(24)));
    // A note the server never saw names no file.
    let unsent = Row { server_id: 0, ..note_row() };
    let m = message(&chat("48614902608@chatroom"), &Names::default(), &unsent, &held);
    assert_eq!(m["note"]["items"][3]["image"]["file_id"], Value::Null);
}

#[test]
fn a_notes_item_is_found_by_its_file_id() {
    let id =
        note::FileId::parse(&format!("note:{NOTE_ID}:084b81e4ceaa08076026a325299e26fe")).unwrap();
    assert_eq!((id.message, id.item.as_str()), (NOTE_ID, "084b81e4ceaa08076026a325299e26fe"));
    assert_eq!(id.text(), format!("note:{NOTE_ID}:084b81e4ceaa08076026a325299e26fe"));
    for bad in
        ["note:", "note:12", "note:x:ab", "note:12:", "note:0:ab", "note:12:a/b", "fav:1:a", "12"]
    {
        assert_eq!(note::FileId::parse(bad), None, "{bad}");
    }
    assert!(note::is_file_id("note:x") && !note::is_file_id("fav:1:a"));
    let chat = chat("48614902608@chatroom");
    let rec = Some("58d452fe52a3cf23".to_string());
    assert_eq!(
        note::item(&chat, &note_row(), "084b81e4ceaa08076026a325299e26fe"),
        Ok(Some(Media::NoteImage { rec: rec.clone(), place: 3 }))
    );
    assert_eq!(
        note::item(&chat, &note_row(), "58f6f1ad9a8dc9939d7948311046641e"),
        Ok(Some(Media::NoteFile { rec, place: 0, name: None }))
    );
    // A paragraph carries no file; an item the note does not hold is none.
    assert_eq!(
        note::item(&chat, &note_row(), "e884e2dd481545a59da8c7b342bc70af"),
        Err(wxread_favorites::Kind::Text)
    );
    assert_eq!(note::item(&chat, &note_row(), "nope"), Ok(None));
    // A message that is not a note holds no item.
    assert_eq!(note::item(&chat, &row("message_0", 1, 1, 0), "a"), Ok(None));
}
#[test]
fn a_chat_no_database_holds_is_refused_without_calling_it_an_id_chats_lists() {
    let why = no_such_chat("wxid_nobody", "wxid_me_1a2b");
    assert!(
        why.contains("no messages of chat wxid_nobody") && why.contains("`wxread chats`"),
        "{why}"
    );
    assert!(why.contains("`wxread chats --query wxid_nobody`"), "{why}");
    assert!(!why.contains("an id `wxread chats` lists"), "{why}");
}

#[test]
fn a_folder_asked_for_as_a_chat_is_refused_naming_what_reads_it() {
    for folder in wxread_chats::Folder::ALL {
        let why = no_such_chat(folder.id(), "wxid_me_1a2b");
        assert_eq!(why, folder.not_a_chat());
        assert!(why.contains(&format!("`{}`", folder.read_with())), "{why}");
    }
}

#[test]
fn every_person_a_message_names_is_shown_by_the_remark_first() {
    let mut names = Names::default();
    let ann = Contact { remark: Some("Auntie".into()), nick_name: Some("Ann".into()) };
    names.contacts.insert("wxid_ann".into(), ann);
    names.in_group.insert("wxid_ann".into(), "Ann G".into());
    let auntie = json!({ "id": "wxid_ann", "display_name": "Auntie" });
    let said = |local_type: i64, content: &str| {
        let mut r = row("message_0", 1, 1, 0);
        r.local_type = local_type;
        r.content = format!("wxid_bob:\n{content}").into_bytes();
        r.sender = Some("wxid_bob".into());
        message(&chat("1@chatroom"), &names, &r, &none)
    };
    let quote = r#"<msg><appmsg><title>yes</title><type>57</type><refermsg><type>1</type><svrid>7</svrid><chatusr>wxid_ann</chatusr><displayname>Ann</displayname><content>?</content></refermsg></appmsg></msg>"#;
    assert_eq!(said(app(57), quote)["quote"]["quoted"]["sender"], auntie);
    let card = r#"<msg username="wxid_ann" nickname="Ann"/>"#;
    assert_eq!(said(CARD, card)["card"]["contact"], auntie);
    let pat = r#"<msg><appmsg><title>t</title><type>62</type><patinfo><fromusername>wxid_ann</fromusername><pattedusername>wxid_ann</pattedusername></patinfo></appmsg></msg>"#;
    let nudge = said(app(62), pat);
    assert_eq!((&nudge["nudge"]["from"], &nudge["nudge"]["to"]), (&auntie, &auntie));
    let joined = notice(
        r#""$username$"邀请"$names$"加入了群聊"#,
        &(members("username", &[("wxid_ann", "Ann")]) + &members("names", &[("wxid_ann", "Ann")])),
    );
    let join = said(SYSTEM_XML, &joined);
    assert_eq!((&join["join"]["by"], &join["join"]["contacts"][0]), (&auntie, &auntie));
    let forward = r#"<msg><appmsg><title>h</title><type>19</type><recorditem><![CDATA[<recordinfo><datalist count="1"><dataitem datatype="1"><datadesc>hi</datadesc><sourcename>Ann</sourcename><dataitemsource><realchatname>wxid_ann</realchatname></dataitemsource></dataitem></datalist></recordinfo>]]></recorditem></appmsg></msg>"#;
    assert_eq!(said(app(19), forward)["forward"]["messages"][0]["sender"], auntie);
    // Someone the account has no contact of is shown by the name the
    // message gives them, else by "".
    let stranger = r#"<msg username="wxid_zed" nickname="Zed"/>"#;
    assert_eq!(
        said(CARD, stranger)["card"]["contact"],
        json!({ "id": "wxid_zed", "display_name": "Zed" })
    );
    assert_eq!(said(TEXT, "hi")["sender"], json!({ "id": "wxid_bob", "display_name": "" }));
}
