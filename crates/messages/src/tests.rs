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
        json!({ "kind": "quote", "quote": { "text": "yes, 9am", "quoted": { "id": "7001", "sender": "wxid_ann", "name": "Ann", "excerpt": "Saturday?" } } })
    );
    let of_image = r#"<msg><appmsg><title>nice</title><refermsg><type>3</type><svrid>7002</svrid><fromusr>wxid_bob</fromusr><content>&lt;msg&gt;&lt;img/&gt;&lt;/msg&gt;</content></refermsg></appmsg></msg>"#;
    assert_eq!(
        read(app(57), of_image)["quote"]["quoted"],
        json!({ "id": "7002", "sender": "wxid_bob", "name": null, "excerpt": null })
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
        json!({ "kind": "card", "card": { "contact": "wxid_dan", "name": "Dan" } })
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
        { "sender": "wxid_ann", "name": "Ann", "at": "2026-10-19T09:30:00Z", "kind": "text", "content": "Saturday?", "type": 1, "inside": null },
        { "sender": null, "name": "Ben", "at": null, "kind": "forward", "content": "Older", "type": 17, "inside": null },
        { "sender": "wxid_cat", "name": "Cat", "at": null, "kind": "text", "content": "inside", "type": 1, "inside": 1 },
        { "sender": null, "name": "Cat", "at": null, "kind": "image", "content": "[Photo]", "type": 2, "inside": 1 },
        { "sender": null, "name": "Ben", "at": null, "kind": "link", "content": "Route topo", "type": 5, "inside": null },
        { "sender": null, "name": null, "at": null, "kind": "unknown", "content": "?", "type": 99, "inside": null },
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
        (m["sender"].clone(), m["sender_name"].clone())
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
        json!({ "kind": "nudge", "nudge": { "from": "wxid_ann", "to": "wxid_me", "text": "\"Ann\" 拍了拍 我" } })
    );
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
        json!({ "kind": "join", "join": { "contacts": ["wxid_ben", "wxid_cat"], "by": "wxid_ann", "text": "\"Ann\"邀请\"Ben、Cat\"加入了群聊" } })
    );
    let by_qr = notice(
        r#""$adder$"通过扫描"$from$"分享的二维码加入群聊"#,
        &(members("adder", &[("wxid_dan", "Dan")]) + &members("from", &[("wxid_ann", "Ann")])),
    );
    assert_eq!(read(SYSTEM, &by_qr)["join"]["contacts"], json!(["wxid_dan"]));
    assert_eq!(read(SYSTEM, &by_qr)["join"]["by"], "wxid_ann");
    let me_too = notice(
        r#""$username$"邀请你和"$others$"加入了群聊"#,
        &(members("username", &[("wxid_ann", "Ann")]) + &members("others", &[("wxid_ben", "Ben")])),
    );
    assert_eq!(read(SYSTEM, &me_too)["join"]["contacts"], json!(["wxid_me", "wxid_ben"]));
    let by_me = notice(r#"你邀请"$names$"加入了群聊"#, &members("names", &[("wxid_ben", "Ben")]));
    assert_eq!(read(SYSTEM, &by_me)["join"]["by"], "wxid_me");
}

#[test]
fn a_removal_is_a_leave_and_a_rename_has_the_new_name() {
    let removed =
        notice(r#"你将"$kickoutname$"移出了群聊"#, &members("kickoutname", &[("wxid_ben", "Ben")]));
    assert_eq!(
        read(SYSTEM, &removed),
        json!({ "kind": "leave", "leave": { "contacts": ["wxid_ben"], "by": "wxid_me", "text": "你将\"Ben\"移出了群聊" } })
    );
    let renamed = notice(
        r#""$username$"修改群名为“$remark$”"#,
        &(members("username", &[("wxid_ann", "Ann")])
            + r#"<link name="remark" type="link_plain"><plain><![CDATA[Crag Rats]]></plain></link>"#),
    );
    assert_eq!(
        read(SYSTEM, &renamed),
        json!({ "kind": "renamed", "renamed": { "name": "Crag Rats", "by": "wxid_ann", "text": "\"Ann\"修改群名为“Crag Rats”" } })
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
            "id": "1001", "at": "2025-10-09T08:53:20Z", "sender": "wxid_ann", "sender_name": "Ann",
            "kind": "text", "text": { "text": "m1" }, "type": 1, "subtype": 0, "content": "m1",
        })
    );
    let mut local = row("message_0", 1, 0, 0);
    local.server_id = 0;
    local.sender = None;
    let m = message(&chat("wxid_ann"), &names, &local, &none);
    assert_eq!(
        (&m["id"], &m["at"], &m["sender"], &m["sender_name"]),
        (&Value::Null, &Value::Null, &Value::Null, &Value::Null)
    );
}

#[test]
fn a_group_message_drops_its_sender_prefix() {
    let names = Names::default();
    let mut r = row("message_0", 1, 1, 0);
    r.content = b"wxid_ann:\nhello:\nthere".to_vec();
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"], &m["content"]), (&json!("wxid_ann"), &json!("hello:\nthere")));
    r.sender = None;
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"], &m["content"]), (&json!("wxid_ann"), &json!("hello:\nthere")));
    // A direct chat's content is its own, whatever it starts with.
    r.sender = Some("wxid_ann".into());
    let m = message(&chat("wxid_ann"), &names, &r, &none);
    assert_eq!(m["content"], "wxid_ann:\nhello:\nthere");
    // The account's own message in a group has no prefix.
    r.sender = Some("wxid_me".into());
    r.content = b"hi all".to_vec();
    let m = message(&chat("1@chatroom"), &names, &r, &none);
    assert_eq!((&m["sender"], &m["content"]), (&json!("wxid_me"), &json!("hi all")));
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
    for text in ["", "x", "1.2.message_0", "a.2.message_0.4", "1.2..4", "1.2.message_0.4.5"] {
        let why = Cursor::parse(text).unwrap_err();
        assert!(why.contains("leave --cursor out"), "{text}: {why}");
    }
    let c = Cursor::parse("10.-3.message_0.4").unwrap();
    assert_eq!(c.text(), "10.-3.message_0.4");
    assert_eq!(c.below_in("message_0"), (10, -3, 4));
    assert_eq!(c.below_in("biz_message_0"), (10, -3, i64::MAX));
    assert_eq!(c.below_in("message_1"), (10, -3, i64::MIN));
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
        None,
        Limit::default_limit(),
        none,
    );
    assert_eq!(ids(&all), [1, 2, 3, 1, 9]);
    assert_eq!(all.messages[0]["at"], rfc3339(200).unwrap());
    assert_eq!(all.next, None);
    let range = Range { from: Some(100), to: Some(200) };
    let some =
        page(&chat("wxid_ann"), &Names::default(), rows, range, None, Limit::default_limit(), none);
    assert_eq!(ids(&some), [2, 3, 1]);
}

proptest! {
    #[test]
    fn paging_with_next_returns_each_message_once_in_order(
        rows in proptest::collection::vec((0usize..3, 0i64..3, 0i64..3), 0..40),
        limit in 1usize..7,
    ) {
        // Many messages in one second, some of one order in it, across
        // databases: each is still on one page, in its place.
        let rows: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, &(db, at, seq))| row(&format!("message_{db}"), i as i64, at, seq))
            .collect();
        let read = |after: Option<&Cursor>, limit: usize| {
            page(&chat("wxid_ann"), &Names::default(), rows.clone(), Range::default(), after, Limit::new(limit).unwrap(), none)
        };
        let whole = read(None, wxread_answer::MAX_LIMIT);
        prop_assert_eq!(whole.messages.len(), rows.len());
        let mut paged = Vec::new();
        let mut after = None;
        loop {
            let p = read(after.as_ref(), limit);
            prop_assert!(p.messages.len() <= limit);
            paged.extend(p.messages);
            match p.next {
                Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
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
