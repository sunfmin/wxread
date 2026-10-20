//! `wxread articles`: what official accounts sent, read from WeChat's own
//! store for them, each article of a push on its own, a page at a time.

use serde_json::{Value, json};
use wxread_schema::CONTACT_DB;

use crate::common::{World, snapshot};
use crate::synthetic::{Account, Article, Contact, Said};

const ROUTES: Article = Article(
    "Route of the week",
    "The north face, again",
    "https://mp.weixin.qq.com/s/route",
    "https://mmbiz.qpic.cn/route.jpg",
);
const GEAR: Article = Article(
    "Gear review",
    "Ropes & rope bags",
    "https://mp.weixin.qq.com/s?__biz=1&mid=2",
    "https://mmbiz.qpic.cn/gear.jpg",
);
const KNOTS: Article = Article(
    "Knots",
    "Eight ways to tie in",
    "https://mp.weixin.qq.com/s/knots",
    "https://mmbiz.qpic.cn/knots.jpg",
);
const TEA: Article = Article(
    "Oolong",
    "A tea for autumn",
    "https://mp.weixin.qq.com/s/tea",
    "https://mmbiz.qpic.cn/tea.jpg",
);
const PRESS: Article = Article("Hello", "From a stranger", "https://mp.weixin.qq.com/s/hi", "");

/// An account following three official accounts and sent articles by a
/// fourth it has no contact of: a push of two articles, one of one only in
/// its store's WAL, one compressed in a second store, and a text message
/// that is no article; `gh_quiet` sent nothing.
fn account(world: &World, id: &'static str) -> Account {
    let mut account = Account::new(&world.data_dir(), id);
    account.contacts = vec![
        Contact("gh_crag", "Crag News", ""),
        Contact("gh_tea", "Tea Daily", "Tea!"),
        Contact("gh_quiet", "Quiet Co", ""),
    ];
    let recent = Said::push("gh_crag", "Crag News", 1_760_000_400, &[ROUTES]);
    let mut tea = Said::push("gh_tea", "Tea Daily", 1_760_000_200, &[TEA]);
    (tea.db, tea.zstd) = ("biz_message_1", true);
    let mut text = Said::push("gh_crag", "Crag News", 1_760_000_300, &[ROUTES]);
    (text.local_type, text.content) = (1, "Thanks for following".to_string());
    account.messages = vec![
        Said::push("gh_crag", "Crag News", 1_760_000_100, &[GEAR, KNOTS]),
        Said::push("gh_stranger", "Stranger Press", 1_760_000_050, &[PRESS]),
        tea,
        text,
    ];
    account.recent_messages = vec![recent];
    account.write();
    account
}

/// The keys of every database `account` has.
fn keys(account: &Account) -> String {
    account.all_keys()
}

fn article(id: &str, name: &str, at: &str, a: &Article) -> Value {
    let Article(title, digest, url, cover) = a;
    json!({
        "official_account": {"id": id, "display_name": name},
        "sent_at": at,
        "title": title,
        "digest": digest,
        "url": url,
        "cover": cover,
    })
}

fn titles(json: &Value) -> Vec<String> {
    let articles = json["articles"].as_array().unwrap();
    articles.iter().map(|a| a["title"].as_str().unwrap().to_string()).collect()
}

#[test]
fn it_lists_every_article_newest_first_each_of_a_push_on_its_own() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let json = world.run(&["articles"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(
        json,
        json!({
            "account": "wxid_me_1a2b",
            "articles": [
                article("gh_crag", "Crag News", "2025-10-09T09:00:00Z", &ROUTES),
                article("gh_tea", "Tea!", "2025-10-09T08:56:40Z", &TEA),
                article("gh_crag", "Crag News", "2025-10-09T08:55:00Z", &GEAR),
                article("gh_crag", "Crag News", "2025-10-09T08:55:00Z", &KNOTS),
                article("gh_stranger", "Stranger Press", "2025-10-09T08:54:10Z", &PRESS),
            ],
            "next": null,
        })
    );
}

#[test]
fn official_account_picks_one_official_account_s_articles() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let env = [("WXREAD_KEYS", keys(&account))];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let json = world.run(&["articles", "--official-account", "gh_crag"], &env).ok();
    assert_eq!(titles(&json), ["Route of the week", "Gear review", "Knots"]);
}

#[test]
fn an_official_account_with_none_answers_an_empty_list() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    for id in ["gh_quiet", "gh_nobody"] {
        let json = world
            .run(&["articles", "--official-account", id], &[("WXREAD_KEYS", &keys(&account))])
            .ok();
        assert_eq!(json, json!({"account": "wxid_me_1a2b", "articles": [], "next": null}), "{id}");
    }
}

#[test]
fn an_account_with_no_store_of_official_accounts_answers_an_empty_list() {
    let world = World::new();
    let mut account = Account::new(&world.data_dir(), "wxid_me_1a2b");
    account.contacts = vec![Contact("gh_crag", "Crag News", "")];
    account.write();
    let json = world.run(&["articles"], &[("WXREAD_KEYS", &keys(&account))]).ok();
    assert_eq!(json, json!({"account": "wxid_me_1a2b", "articles": [], "next": null}));
}

#[test]
fn paging_with_next_returns_each_article_once() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = keys(&account);
    let env = [("WXREAD_KEYS", keys.as_str())];
    let whole = world.run(&["articles"], &env).ok()["articles"].clone();
    for limit in ["1", "2", "3"] {
        let mut paged = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut args = vec!["articles", "--limit", limit];
            if let Some(c) = &cursor {
                args.extend(["--cursor", c.as_str()]);
            }
            let json = world.run(&args, &env).ok();
            let page = json["articles"].as_array().unwrap().clone();
            assert!(page.len() <= limit.parse().unwrap(), "{json}");
            paged.extend(page);
            match json["next"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        assert_eq!(Value::Array(paged), whole, "--limit {limit}");
    }
}

#[test]
fn no_decrypted_byte_is_written() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = keys(&account);
    let tmp = world.root.join("tmp");
    std::fs::create_dir(&tmp).unwrap();
    let before = snapshot(&account.dir());
    let env = [("WXREAD_KEYS", keys.as_str()), ("TMPDIR", tmp.to_str().unwrap())];
    world.run(&["articles"], &env).ok();
    world.run(&["articles", "--limit", "1"], &env).ok();
    assert_eq!(snapshot(&account.dir()), before, "the account's directory is as it was");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "nothing is in the temp directory");
}

#[test]
fn a_store_the_keys_lack_is_named_with_the_fix() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = account.keys(&[CONTACT_DB, "message/biz_message_0.db"]);
    let error = world.run(&["articles"], &[("WXREAD_KEYS", &keys)]).error();
    assert!(error.contains("no key of message/biz_message_1.db"), "{error}");
    assert!(error.contains("sudo wxread keys"), "{error}");
    let error = world.run(&["articles"], &[]).error();
    assert!(error.contains("message/biz_message_<n>.db") && error.contains(CONTACT_DB), "{error}");
}

#[test]
fn a_limit_or_cursor_that_is_not_one_is_refused_with_its_fix() {
    let world = World::new();
    let account = account(&world, "wxid_me_1a2b");
    let keys = keys(&account);
    for limit in ["0", "1001", "x"] {
        let error = world.run(&["articles", "--limit", limit], &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains("number of articles from 1 to 1000"), "{limit}: {error}");
    }
    for cursor in ["nope", "1:2:3", "x:1:0:gh_a"] {
        let error = world.run(&["articles", "--cursor", cursor], &[("WXREAD_KEYS", &keys)]).error();
        assert!(error.contains("leave --cursor out"), "{cursor}: {error}");
    }
}
