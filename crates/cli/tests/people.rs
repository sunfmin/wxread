//! Every person, group, official account and chat an answer names is
//! `{id, display_name}`, the display name as `wxread contacts` answers it:
//! a remarked contact is shown by the remark in every command.

use serde_json::{Value, json};

use crate::common::World;
use crate::synthetic::{Account, Article, Contact, Group, Posted, Said, Saved, Session};

const GROUP: &str = "123@chatroom";
const AT: i64 = 1_760_000_000;

/// An account whose contact Amy it calls "Auntie" and whose official
/// account Crag News it calls "Crag", Amy going by "Amy G" in its group:
/// Amy said something in the group and to the account, shared her own
/// card, posted a Moment, and said what the account saved; Crag News
/// pushed an article.
fn account(world: &World) -> Account {
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![
        Contact("wxid_amy", "Amy", "Auntie"),
        Contact("gh_crag", "Crag News", "Crag"),
        Contact(GROUP, "Hiking", ""),
        Contact("wxid_me", "Me", ""),
    ];
    account.groups = vec![Group {
        id: GROUP,
        owner: "wxid_amy",
        members: vec![("wxid_amy", "Amy G"), ("wxid_me", "")],
        notice: None,
    }];
    account.sessions = vec![Session("wxid_amy", AT + 3, 0), Session(GROUP, AT + 2, 0)];
    let card = Said {
        local_type: 42,
        ..Said::text(GROUP, 3, AT + 2, "wxid_amy", "<msg username=\"wxid_amy\" nickname=\"Amy\"/>")
    };
    account.messages = vec![
        Said::text("wxid_amy", 1, AT + 3, "wxid_amy", "crag at nine"),
        Said::text(GROUP, 2, AT + 1, "wxid_amy", "crag at ten"),
        card,
        Said::push(
            "gh_crag",
            "Crag News",
            AT,
            &[Article("Crag", "d", "https://e.com/a", "https://e.com/a.jpg")],
        ),
    ];
    account.moments = vec![Posted(
        (AT * 1000) << 23 | 1,
        "wxid_amy",
        format!(
            "<SnsDataItem><TimelineObject><id>{}</id><username>wxid_amy</username><createTime>{AT}</createTime><contentDesc>Summit</contentDesc></TimelineObject><LocalExtraInfo><nickname>Amy</nickname><like_user_list><user_comment><username>wxid_amy</username><nickname>Amy</nickname><create_time>{AT}</create_time></user_comment></like_user_list></LocalExtraInfo></SnsDataItem>",
            ((AT * 1000) << 23 | 1) as u64
        ),
    )];
    account.favorites = vec![Saved {
        id: 1,
        fav_type: 1,
        saved: AT,
        content: format!(
            "<favitem type=\"1\"><desc>hi</desc><source sourcetype=\"1\"><fromusr>wxid_amy</fromusr><tousr>wxid_me</tousr><createtime>{AT}</createtime></source></favitem>"
        ),
        from: "wxid_amy",
        sender: "",
    }];
    account.write();
    account
}

/// Every `{id, display_name}` in `answer`, wherever it sits.
fn people(answer: &Value, found: &mut Vec<(String, String)>) {
    match answer {
        Value::Object(fields) => {
            if let (Some(id), Some(shown)) = (
                fields.get("id").and_then(Value::as_str),
                fields.get("display_name").and_then(Value::as_str),
            ) {
                found.push((id.to_string(), shown.to_string()));
            }
            fields.values().for_each(|v| people(v, found));
        }
        Value::Array(all) => all.iter().for_each(|v| people(v, found)),
        _ => {}
    }
}

#[test]
fn a_remarked_contact_is_shown_by_the_remark_in_every_answer() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let env = [("WXREAD_KEYS", keys.as_str())];
    for args in [
        &["chats"][..],
        &["contacts"],
        &["contact", "wxid_amy"],
        &["group", GROUP],
        &["messages", "--chat", "wxid_amy"],
        &["messages", "--chat", GROUP],
        &["search", "crag"],
        &["moments"],
        &["articles"],
        &["favorites"],
    ] {
        let answer = world.run(args, &env).ok();
        let mut found = Vec::new();
        people(&answer, &mut found);
        let shown: Vec<&str> = found
            .iter()
            .filter(|(id, _)| id == "wxid_amy" || id == "gh_crag")
            .map(|(_, shown)| shown.as_str())
            .collect();
        assert!(!shown.is_empty(), "{args:?} names Amy or Crag News: {answer}");
        assert!(
            shown.iter().all(|s| *s == "Auntie" || *s == "Crag"),
            "{args:?} shows each by the remark: {found:?}"
        );
        let text = answer.to_string();
        for old in ["sender_name", "from_name", "chat_name"] {
            assert!(!text.contains(old), "{args:?} answers no {old}: {answer}");
        }
    }
}

#[test]
fn a_message_names_its_chat_and_sender_as_wechat_contacts_does() {
    let world = World::new();
    let account = account(&world);
    let keys = account.all_keys();
    let json = world.run(&["messages", "--chat", GROUP], &[("WXREAD_KEYS", &keys)]).ok();
    assert_eq!(json["chat"], json!({"id": GROUP, "display_name": "Hiking"}));
    let card = json["messages"].as_array().unwrap().iter().find(|m| m["kind"] == "card").unwrap();
    assert_eq!(card["sender"], json!({"id": "wxid_amy", "display_name": "Auntie"}));
    assert_eq!(card["card"], json!({"contact": {"id": "wxid_amy", "display_name": "Auntie"}}));
}
