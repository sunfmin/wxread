//! `wxread messages`: a chat's messages read from the account's encrypted
//! message databases and their WAL, each as its kind, a page at a time,
//! with nothing decrypted written.

use serde_json::{Value, json};
use wxread_schema::CONTACT_DB;

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Contact, Group, Said, field};

const GROUP: &str = "123@chatroom";
const T: i64 = 1_760_000_000;

/// An app message's `local_type` of subtype `sub`.
fn app(sub: i64) -> i64 {
    49 | sub << 32
}

/// A message of the group, of `local_type`, Bob's unless it is a notice,
/// its content prefixed with its sender as WeChat keeps a group's.
fn group(n: i64, local_type: i64, content: &str) -> Said {
    let notice = local_type == 10000 || local_type == 10002;
    let sender = if notice { None } else { Some("wxid_bob") };
    let mut said = Said::text(GROUP, 9000 + n, T + n, "wxid_bob", content);
    said.local_type = local_type;
    said.sender = sender;
    if !notice {
        said.content = format!("wxid_bob:\n{content}");
    }
    said
}

/// Every kind, said in the group in turn, with the fields each is read as.
fn every_kind() -> Vec<(Said, &'static str, Value)> {
    let template = |template: &str, links: &str| {
        format!(
            r#"<sysmsg type="sysmsgtemplate"><sysmsgtemplate><content_template type="tmpl_type_profile"><template><![CDATA[{template}]]></template><link_list>{links}</link_list></content_template></sysmsgtemplate></sysmsg>"#
        )
    };
    let member = |name: &str, id: &str, nick: &str| {
        format!(
            r#"<link name="{name}" type="link_profile"><memberlist><member><username>{id}</username><nickname>{nick}</nickname></member></memberlist></link>"#
        )
    };
    let record = "<recordinfo><datalist><dataitem datatype=\"1\"><datadesc>Saturday?</datadesc><sourcename>Amy</sourcename><srcMsgCreateTime>1760000000</srcMsgCreateTime><dataitemsource><realchatname>wxid_amy</realchatname></dataitemsource></dataitem><dataitem datatype=\"17\"><datatitle>Older</datatitle><sourcename>Bob</sourcename><recordxml><recordinfo><datalist><dataitem datatype=\"2\"><datadesc>[Photo]</datadesc><sourcename>Cat</sourcename></dataitem></datalist></recordinfo></recordxml></dataitem></datalist></recordinfo>";
    let mut image = group(15, 3, r#"<msg><img md5="feed01" length="2048"/></msg>"#);
    image.packed = field(3, &field(4, b"a1b2c3"));
    let mut video = group(16, 43, r#"<msg><videomsg playlength="7" md5="beef02"/></msg>"#);
    video.packed = field(4, &field(8, b"v9"));
    let mut file = group(
        17,
        app(6),
        "<msg><appmsg><title>topo.pdf</title><type>6</type><appattach><totallen>4096</totallen><fileext>pdf</fileext></appattach></appmsg></msg>",
    );
    file.packed = field(7, &field(2, b"topo(1).pdf"));
    let mut zstd = group(
        22,
        app(5),
        "<msg><appmsg><title>Packed</title><type>5</type><url>https://e.com/z</url></appmsg></msg>",
    );
    zstd.zstd = true;
    vec![
        (group(1, 1, "hello:\nall"), "text", json!({ "text": "hello:\nall" })),
        (
            group(
                3,
                app(5),
                "<msg><appmsg><title>Route topo</title><des>North face</des><type>5</type><url>https://e.com/topo</url></appmsg></msg>",
            ),
            "link",
            json!({ "to": "web_page", "title": "Route topo", "summary": "North face", "url": "https://e.com/topo", "source": null }),
        ),
        (
            group(
                4,
                app(57),
                "<msg><appmsg><title>yes, 9am</title><type>57</type><refermsg><type>1</type><svrid>9001</svrid><chatusr>wxid_bob</chatusr><displayname>Bobby</displayname><content>Saturday?</content></refermsg></appmsg></msg>",
            ),
            "quote",
            json!({ "text": "yes, 9am", "quoted": { "id": "9001", "sender": "wxid_bob", "name": "Bobby", "excerpt": "Saturday?" } }),
        ),
        (
            group(
                5,
                48,
                r#"<msg><location x="22.5" y="114.1" label="1 Shennan Rd" poiname="The Gym"/></msg>"#,
            ),
            "location",
            json!({ "latitude": 22.5, "longitude": 114.1, "name": "The Gym", "address": "1 Shennan Rd" }),
        ),
        (
            group(6, 42, r#"<msg username="wxid_dan" nickname="Dan"/>"#),
            "card",
            json!({ "contact": "wxid_dan", "name": "Dan" }),
        ),
        (
            group(
                7,
                app(19),
                &format!(
                    "<msg><appmsg><title>Chat history</title><type>19</type><recorditem><![CDATA[{record}]]></recorditem></appmsg></msg>"
                ),
            ),
            "forward",
            json!({ "title": "Chat history", "messages": [
                { "sender": "wxid_amy", "name": "Amy", "at": "2025-10-09T08:53:20Z", "kind": "text", "content": "Saturday?", "type": 1, "inside": null },
                { "sender": null, "name": "Bob", "at": null, "kind": "forward", "content": "Older", "type": 17, "inside": null },
                { "sender": null, "name": "Cat", "at": null, "kind": "image", "content": "[Photo]", "type": 2, "inside": 1 },
            ] }),
        ),
        (
            group(
                8,
                10002,
                r#"<sysmsg type="revokemsg"><revokemsg><content><![CDATA["Bobby" 撤回了一条消息]]></content></revokemsg></sysmsg>"#,
            ),
            "recall",
            json!({ "text": "\"Bobby\" 撤回了一条消息" }),
        ),
        (
            group(
                9,
                app(62),
                "<msg><appmsg><title>\"Bobby\" 拍了拍 我</title><type>62</type><patinfo><fromusername>wxid_bob</fromusername><pattedusername>wxid_me</pattedusername></patinfo></appmsg></msg>",
            ),
            "nudge",
            json!({ "from": "wxid_bob", "to": "wxid_me", "text": "\"Bobby\" 拍了拍 我" }),
        ),
        (
            group(
                10,
                10000,
                &template(
                    r#""$username$"邀请"$names$"加入了群聊"#,
                    &(member("username", "wxid_bob", "Bobby")
                        + &member("names", "wxid_dan", "Dan")),
                ),
            ),
            "join",
            json!({ "contacts": ["wxid_dan"], "by": "wxid_bob", "text": "\"Bobby\"邀请\"Dan\"加入了群聊" }),
        ),
        (
            group(
                11,
                10000,
                &template(
                    r#"你将"$kickoutname$"移出了群聊"#,
                    &member("kickoutname", "wxid_dan", "Dan"),
                ),
            ),
            "leave",
            json!({ "contacts": ["wxid_dan"], "by": "wxid_me", "text": "你将\"Dan\"移出了群聊" }),
        ),
        (
            group(
                12,
                10000,
                &template(
                    r#""$username$"修改群名为“$remark$”"#,
                    &(member("username", "wxid_bob", "Bobby")
                        + r#"<link name="remark" type="link_plain"><plain>Crag Rats</plain></link>"#),
                ),
            ),
            "renamed",
            json!({ "name": "Crag Rats", "by": "wxid_bob", "text": "\"Bobby\"修改群名为“Crag Rats”" }),
        ),
        (
            group(
                13,
                app(2000),
                "<msg><appmsg><type>2000</type><wcpayinfo><paysubtype>1</paysubtype><feedesc>￥12.50</feedesc><pay_memo>lunch</pay_memo></wcpayinfo></appmsg></msg>",
            ),
            "transfer",
            json!({ "amount_fen": 1250, "state": "sent", "memo": "lunch" }),
        ),
        (
            group(
                14,
                app(2001),
                "<msg><appmsg><type>2001</type><wcpayinfo><sendertitle>恭喜发财</sendertitle></wcpayinfo></appmsg></msg>",
            ),
            "red_packet",
            json!({ "amount_fen": null, "state": null, "memo": "恭喜发财" }),
        ),
        (image, "image", json!({ "name": "a1b2c3", "md5": "feed01" })),
        (video, "video", json!({ "name": "v9", "seconds": 7, "md5": "beef02" })),
        (
            file,
            "file",
            json!({ "name": "topo.pdf", "stored_as": "topo(1).pdf", "size": 4096, "extension": "pdf" }),
        ),
        (
            group(18, 47, r#"<msg><emoji md5="ABC" cdnurl="https://cdn.example/s"/></msg>"#),
            "sticker",
            json!({ "md5": "abc", "url": "https://cdn.example/s" }),
        ),
        (
            group(19, 34, r#"<msg><voicemsg voicelength="3200"/></msg>"#),
            "voice",
            json!({ "length_ms": 3200 }),
        ),
        (
            group(20, 10000, "Bobby is now an admin"),
            "system",
            json!({ "text": "Bobby is now an admin" }),
        ),
        (group(21, app(5), "<msg><appmsg><title>broken"), "unknown", json!({})),
        (
            zstd,
            "link",
            json!({ "to": "web_page", "title": "Packed", "summary": null, "url": "https://e.com/z", "source": null }),
        ),
    ]
}

/// An account with a group whose messages are of every kind, and a
/// direct chat with Amy whose messages are many in one second, across two
/// message databases and the WAL of one.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_me", "Me", ""),
        Contact("wxid_amy", "Amy", "Amy W"),
        Contact("wxid_bob", "Bob", ""),
        Contact(GROUP, "Hiking", ""),
    ];
    account.groups = vec![Group {
        id: GROUP,
        owner: "wxid_bob",
        members: vec![("wxid_bob", "Bobby"), ("wxid_dan", "Dan")],
        notice: None,
    }];
    account.messages = every_kind().into_iter().map(|(said, _, _)| said).collect();
    // Dan is in no contact and named by no row: only the prefix says it is
    // Dan's.
    let mut dans = group(2, 1, "hi");
    dans.sender = None;
    dans.content = "wxid_dan:\nhi".into();
    account.messages.push(dans);
    for (i, (db, seq)) in
        [("message_0", 3), ("message_1", 1), ("message_0", 5), ("message_1", 4), ("message_0", 2)]
            .into_iter()
            .enumerate()
    {
        let mut said =
            Said::text("wxid_amy", 100 + i as i64, T, "wxid_amy", &format!("at once {i}"));
        said.db = db;
        said.sort_seq = T * 1000 + seq;
        account.messages.push(said);
    }
    let mut mine = Said::text("wxid_amy", 99, T - 60, "wxid_me", "earlier");
    mine.db = "message_1";
    account.messages.push(mine);
    let mut last = Said::text("wxid_amy", 110, T, "wxid_amy", "only in the WAL");
    last.sort_seq = T * 1000 + 9;
    account.recent_messages = vec![last];
    account.write();
    account
}

fn ids(json: &Value) -> Vec<String> {
    json["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn each_kind_is_read_as_its_kind_with_its_fields() {
    let world = World::new();
    let account = account(&world);
    let json =
        world.run(&["messages", "--chat", GROUP], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    assert_eq!(
        (&json["account"], &json["chat"], &json["next"]),
        (&json!("wxid_me_1a2b"), &json!(GROUP), &Value::Null)
    );
    let messages = json["messages"].as_array().unwrap();
    let expected = every_kind();
    assert_eq!(messages.len(), expected.len() + 1, "{json}");
    for (said, kind, fields) in expected {
        let id = said.server_id.to_string();
        let m = messages
            .iter()
            .find(|m| m["id"] == id.as_str())
            .unwrap_or_else(|| panic!("{id} in {json}"));
        assert_eq!((&m["kind"], &m[kind]), (&json!(kind), &fields), "{m}");
        assert_eq!(m["at"], wxread_answer::rfc3339(said.create_time).unwrap());
        assert_eq!(
            (m["type"].as_i64(), m["subtype"].as_i64()),
            (Some(said.local_type & 0xffff_ffff), Some(said.local_type >> 32))
        );
        let prefix = format!("{}:\n", said.sender.unwrap_or("-"));
        assert_eq!(
            m["content"],
            said.content.strip_prefix(&prefix).unwrap_or(&said.content),
            "{m}"
        );
    }
    // Newest first; a notice has no sender, the account's own removal is
    // the account's, and each sender is shown as WeChat shows them.
    assert_eq!(messages[0]["id"], "9022");
    let by_id = |id: &str| messages.iter().find(|m| m["id"] == id).unwrap();
    assert_eq!(
        (&by_id("9001")["sender"], &by_id("9001")["sender_name"]),
        (&json!("wxid_bob"), &json!("Bobby"))
    );
    assert_eq!(
        (&by_id("9002")["sender"], &by_id("9002")["sender_name"]),
        (&json!("wxid_dan"), &json!("Dan"))
    );
    assert_eq!(by_id("9002")["content"], "hi");
    assert_eq!(
        (&by_id("9012")["sender"], &by_id("9012")["sender_name"]),
        (&Value::Null, &Value::Null)
    );
}

#[test]
fn a_message_whose_xml_does_not_read_is_unknown_with_its_content_and_type() {
    let world = World::new();
    let account = account(&world);
    let json =
        world.run(&["messages", "--chat", GROUP], &[("WXREAD_KEYS", &account.all_keys())]).ok();
    let m =
        json["messages"].as_array().unwrap().iter().find(|m| m["id"] == "9021").unwrap().clone();
    assert_eq!(
        m,
        json!({
            "id": "9021", "at": "2025-10-09T08:53:41Z", "sender": "wxid_bob", "sender_name": "Bobby",
            "kind": "unknown", "unknown": {}, "type": 49, "subtype": 5, "content": "<msg><appmsg><title>broken",
        })
    );
}

#[test]
fn messages_of_one_second_are_in_order_and_paging_returns_each_once() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    let whole = world.run(&["messages", "--chat", "wxid_amy"], &env).ok();
    assert_eq!(ids(&whole), ["110", "102", "103", "100", "104", "101", "99"]);
    let mine = &whole["messages"][6];
    assert_eq!(
        (&mine["sender"], &mine["sender_name"], &mine["content"]),
        (&json!("wxid_me"), &json!("Me"), &json!("earlier"))
    );
    assert_eq!(whole["messages"][0]["sender_name"], "Amy W");
    for limit in ["1", "2", "3"] {
        let mut paged = Vec::new();
        let mut args = vec!["messages", "--chat", "wxid_amy", "--limit", limit];
        let mut cursor;
        loop {
            let json = world.run(&args, &env).ok();
            let page = ids(&json);
            assert!(page.len() <= limit.parse().unwrap(), "{json}");
            paged.extend(page);
            match json["next"].as_str() {
                Some(next) => {
                    cursor = next.to_string();
                    args = vec![
                        "messages", "--chat", "wxid_amy", "--limit", limit, "--cursor", &cursor,
                    ];
                }
                None => break,
            }
        }
        assert_eq!(paged, ids(&whole), "--limit {limit}");
    }
}

#[test]
fn from_and_to_bound_the_times_read() {
    let world = World::new();
    let account = account(&world);
    let env = [("WXREAD_KEYS", account.all_keys())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let run = |args: &[&str]| ids(&world.run(args, &env).ok());
    assert_eq!(run(&["messages", "--chat", "wxid_amy", "--to", "2025-10-09T08:53:20Z"]), ["99"]);
    assert_eq!(
        run(&["messages", "--chat", "wxid_amy", "--from", "2025-10-09T16:53:20+08:00"]).len(),
        6
    );
    assert_eq!(
        run(&[
            "messages",
            "--chat",
            GROUP,
            "--from",
            "2025-10-09T08:53:21Z",
            "--to",
            "2025-10-09T08:53:23Z"
        ]),
        ["9002", "9001"]
    );
    assert!(run(&["messages", "--chat", GROUP, "--from", "2025-10-10"]).is_empty());
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
    world.run(&["messages", "--chat", GROUP], &env).ok();
    world.run(&["messages", "--chat", "wxid_amy", "--limit", "1"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}

#[test]
fn a_chat_no_database_holds_points_at_chats() {
    let world = World::new();
    let account = account(&world);
    let error = world
        .run(&["messages", "--chat", "wxid_nobody"], &[("WXREAD_KEYS", &account.all_keys())])
        .error();
    assert!(
        error.contains("no messages of chat wxid_nobody") && error.contains("`wxread chats`"),
        "{error}"
    );
    let error = world.run(&["messages"], &[("WXREAD_KEYS", &account.all_keys())]).error();
    assert!(error.contains("pass --chat <id>") && error.contains("`wxread chats`"), "{error}");
}

#[test]
fn a_message_database_the_keys_lack_is_named_with_the_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.keys(&[CONTACT_DB, "message/message_0.db"]);
    let error = world.run(&["messages", "--chat", "wxid_amy"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(
        error.contains("no key of message/message_1.db") && error.contains("sudo wxread keys"),
        "{error}"
    );
    let error = world.run(&["messages", "--chat", "wxid_amy"], &[]).error();
    assert!(
        error.contains("message/message_<n>.db") && error.contains("sudo wxread keys"),
        "{error}"
    );
}

#[test]
fn a_limit_cursor_or_time_that_is_not_one_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let refused = |extra: &[&str]| {
        let mut args = vec!["messages", "--chat", "wxid_amy"];
        args.extend_from_slice(extra);
        world.run(&args, &[("WXREAD_KEYS", &keys)]).error()
    };
    for limit in ["0", "1001", "x"] {
        assert!(refused(&["--limit", limit]).contains("number of messages from 1 to 1000"));
    }
    assert!(refused(&["--cursor", "nope"]).contains("leave --cursor out"));
    assert!(refused(&["--from", "yesterday"]).contains("--from yesterday is not a time"));
    assert!(
        refused(&["--from", "2025-10-10", "--to", "2025-10-09"]).contains("is not before --to")
    );
}
