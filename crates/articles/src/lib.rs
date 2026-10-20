//! What `wxread articles` answers of the messages official accounts sent:
//! each article of a push on its own, with its official account, when it
//! was sent, its title, digest, url and cover, newest first, a page at a
//! time. The effect crate reads the messages out of the official accounts'
//! stores; nothing here reads or writes.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use roxmltree::Node;
use wxread_answer::{CURSOR_FLAG, Limit, rfc3339};
use wxread_messages::kinds::split;
use wxread_xml::{child, named, text, xml};

/// The option that picks one official account's articles.
pub const OFFICIAL_ACCOUNT_FLAG: &str = "--official-account";

/// WeChat's type of an app message, and the appmsg type of a link, which
/// is what an official account's push of articles is.
pub const APP: i64 = 49;
pub const LINK: i64 = 5;

/// One message an official account sent, as its table in a
/// `biz_message_<n>.db` holds it, its content decompressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The official account's id (`gh_…`), whose table it is in.
    pub official_account: String,
    pub local_id: i64,
    /// Its type in the low 32 bits, its subtype in the high.
    pub local_type: i64,
    /// When it was sent, in seconds since the epoch.
    pub create_time: Option<i64>,
    /// `message_content` as stored: its XML, compressed when `compressed`
    /// (`WCDB_CT_message_content`) says.
    pub content: Vec<u8>,
    pub compressed: Option<i64>,
}

/// One article, as `wxread articles` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// The official account's id (`gh_…`).
    pub official_account: String,
    /// The official account as WeChat shows it: the name the account's
    /// contact of it has, else the name its push gives; "" for neither.
    pub name: String,
    /// When its push was sent (RFC 3339, UTC); None for no time.
    pub sent_at: Option<String>,
    pub title: String,
    /// What the push says it is about; "" for nothing.
    pub digest: String,
    pub url: String,
    /// Its cover image's url; "" for none.
    pub cover: String,
}

/// Where an article is in the list: newest push first, by official
/// account and newest message among pushes sent alike, and in a push in
/// its order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    sent: Reverse<i64>,
    official_account: String,
    local_id: Reverse<i64>,
    index: usize,
}

/// Where a page ended: the last article it listed. Its text is the `next`
/// an answer gives and `--cursor` takes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(Place);

impl Cursor {
    /// The cursor `text` writes, or what fixes it.
    pub fn parse(text: &str) -> Result<Cursor, String> {
        let place = (|| {
            let (sent, rest) = text.split_once(':')?;
            let (local_id, rest) = rest.split_once(':')?;
            let (index, official_account) = rest.split_once(':')?;
            Some(Place {
                sent: Reverse(sent.parse().ok()?),
                official_account: Some(official_account).filter(|id| !id.is_empty())?.to_string(),
                local_id: Reverse(local_id.parse().ok()?),
                index: index.parse().ok()?,
            })
        })();
        place.map(Cursor).ok_or_else(|| {
            format!(
                "{CURSOR_FLAG} {text} is not a `next` that wxread articles answered: pass the `next` of the answer before, or leave {CURSOR_FLAG} out to start at the newest article"
            )
        })
    }

    /// What `next` says of it.
    pub fn text(&self) -> String {
        let Place { sent, official_account, local_id, index } = &self.0;
        format!("{}:{}:{index}:{official_account}", sent.0, local_id.0)
    }
}

/// One page of the articles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub articles: Vec<Article>,
    /// Where the next page starts, when there are articles past this one.
    pub next: Option<Cursor>,
}

/// The page of the articles in `messages` that starts after `after` (at
/// the newest, for none) and holds at most `limit`; `names` the name of
/// each official account the account has a contact of, by its id. A
/// message that is no push of articles (a text, an image, a link that
/// does not read as WeChat writes one) has none.
pub fn page(
    messages: &[Message],
    names: &BTreeMap<String, String>,
    after: Option<&Cursor>,
    limit: Limit,
) -> Page {
    let mut placed: Vec<(Place, Article)> = Vec::new();
    for m in messages {
        let named = names.get(&m.official_account).map(String::as_str);
        for (index, article) in articles(m, named).into_iter().enumerate() {
            let place = Place {
                sent: Reverse(m.create_time.unwrap_or(0)),
                official_account: m.official_account.clone(),
                local_id: Reverse(m.local_id),
                index,
            };
            if after.is_none_or(|c| place > c.0) {
                placed.push((place, article));
            }
        }
    }
    placed.sort_by(|a, b| a.0.cmp(&b.0));
    let more = placed.len() > limit.get();
    placed.truncate(limit.get());
    let next = if more { placed.last().map(|(place, _)| Cursor(place.clone())) } else { None };
    Page { articles: placed.into_iter().map(|(_, article)| article).collect(), next }
}

/// The articles of `message`, in its push's order; `contact` the name of
/// its official account the account's contact of it has. None when it is
/// no push of articles.
pub fn articles(message: &Message, contact: Option<&str>) -> Vec<Article> {
    let (kind, sub) = split(message.local_type);
    if kind != APP {
        return Vec::new();
    }
    let content = wxread_messages::content_text(&message.content, message.compressed);
    xml(&content, |root| {
        let app = child(root, "appmsg")?;
        let sub = if sub != 0 { sub } else { text(app, "type")?.parse().ok()? };
        if sub != LINK {
            return None;
        }
        let reader = child(app, "mmreader");
        let pushed =
            reader.and_then(|r| text(r, "publisher/nickname").or_else(|| text(r, "category/name")));
        let name = [contact, pushed.as_deref()]
            .into_iter()
            .flatten()
            .find(|n| !n.is_empty())
            .unwrap_or("")
            .to_string();
        let sent_at = message.create_time.and_then(rfc3339);
        let article = |title, digest, url, cover| Article {
            official_account: message.official_account.clone(),
            name: name.clone(),
            sent_at: sent_at.clone(),
            title,
            digest,
            url,
            cover,
        };
        let items: Vec<Node> =
            reader.and_then(|r| child(r, "category")).map(|c| named(c, "item")).unwrap_or_default();
        let own = |name| text(app, name).unwrap_or_default();
        if items.is_empty() {
            return Some(vec![article(own("title"), own("des"), own("url"), own("thumburl"))]);
        }
        let read = items.into_iter().filter_map(|item| {
            let field = |name| text(item, name).unwrap_or_default();
            let url = text(item, "url")?;
            Some(article(field("title"), field("digest"), url, field("cover")))
        });
        Some(read.collect())
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PUSH: i64 = APP | LINK << 32;

    fn message(id: &str, local_id: i64, at: i64, content: &str) -> Message {
        Message {
            official_account: id.to_string(),
            local_id,
            local_type: PUSH,
            create_time: Some(at),
            content: content.as_bytes().to_vec(),
            compressed: None,
        }
    }

    /// A push of `n` articles, `T<i>` each titled.
    fn push(n: usize) -> String {
        let items: String = (0..n)
            .map(|i| format!("<item><title>T{i}</title><url>https://e.com/{i}</url><digest>D{i}</digest><cover>https://e.com/{i}.jpg</cover></item>"))
            .collect();
        format!(
            "<msg><appmsg><title>T0</title><type>5</type><url>https://e.com/0</url><mmreader><category count=\"{n}\"><name>Cat</name>{items}</category><publisher><username>gh_a</username><nickname>Pub</nickname></publisher></mmreader></appmsg></msg>"
        )
    }

    fn titles(articles: &[Article]) -> Vec<&str> {
        articles.iter().map(|a| a.title.as_str()).collect()
    }

    #[test]
    fn a_push_of_several_is_each_of_its_articles_in_order() {
        let got = articles(&message("gh_a", 1, 1_760_000_000, &push(3)), Some("Crag"));
        assert_eq!(titles(&got), ["T0", "T1", "T2"]);
        assert_eq!(
            got[1],
            Article {
                official_account: "gh_a".into(),
                name: "Crag".into(),
                sent_at: Some("2025-10-09T08:53:20Z".into()),
                title: "T1".into(),
                digest: "D1".into(),
                url: "https://e.com/1".into(),
                cover: "https://e.com/1.jpg".into(),
            }
        );
    }

    #[test]
    fn an_official_account_is_named_by_its_contact_else_its_push() {
        let m = message("gh_a", 1, 1, &push(1));
        assert_eq!(articles(&m, Some("Crag"))[0].name, "Crag");
        assert_eq!(articles(&m, Some(""))[0].name, "Pub");
        assert_eq!(articles(&m, None)[0].name, "Pub");
        let unnamed = push(1).replace("<nickname>Pub</nickname>", "");
        assert_eq!(articles(&message("gh_a", 1, 1, &unnamed), None)[0].name, "Cat");
    }

    #[test]
    fn a_link_with_no_mmreader_is_one_article() {
        let link = "<msg><appmsg><title>Solo</title><des>About</des><type>5</type><url>https://e.com/s</url><thumburl>https://e.com/s.jpg</thumburl></appmsg></msg>";
        let mut m = message("gh_a", 1, 1, link);
        m.local_type = APP;
        let got = articles(&m, None);
        assert_eq!(
            (got.len(), got[0].digest.as_str(), got[0].cover.as_str(), got[0].name.as_str()),
            (1, "About", "https://e.com/s.jpg", "")
        );
    }

    #[test]
    fn what_is_no_push_of_articles_has_none() {
        let mut said = message("gh_a", 1, 1, "Thanks for following");
        said.local_type = 1;
        assert_eq!(articles(&said, None), []);
        let mut file = message("gh_a", 1, 1, &push(1));
        file.local_type = APP | 6 << 32;
        assert_eq!(articles(&file, None), []);
        assert_eq!(articles(&message("gh_a", 1, 1, "<msg><appmsg>"), None), []);
        assert_eq!(articles(&message("gh_a", 1, 1, "<msg/>"), None), []);
    }

    #[test]
    fn xml_as_wechat_writes_it_is_read() {
        let odd = push(1)
            .replace("<appmsg>", "<?xml version=\"1.0\"?>\n<appmsg>")
            .replace("https://e.com/0<", "https://e.com/?a=1&b=2&amp;c=3<");
        let got = articles(&message("gh_a", 1, 1, &odd), None);
        assert_eq!(got[0].url, "https://e.com/?a=1&b=2&c=3");
    }

    #[test]
    fn articles_are_newest_first_and_each_on_one_page() {
        let messages = [
            message("gh_b", 7, 5, &push(2)),
            message("gh_a", 3, 9, &push(1)),
            message("gh_a", 4, 5, &push(2)),
            message("gh_b", 8, 5, &push(1)),
        ];
        let names = BTreeMap::new();
        let two = Limit::new(2).unwrap();
        let place = |p: &Page| -> Vec<(String, String)> {
            p.articles.iter().map(|a| (a.official_account.clone(), a.title.clone())).collect()
        };
        let first = page(&messages, &names, None, two);
        let pair = |id: &str, t: &str| (id.to_string(), t.to_string());
        assert_eq!(place(&first), [pair("gh_a", "T0"), pair("gh_a", "T0")]);
        let next = Cursor::parse(&first.next.unwrap().text()).unwrap();
        let second = page(&messages, &names, Some(&next), two);
        assert_eq!(place(&second), [pair("gh_a", "T1"), pair("gh_b", "T0")]);
        let third = page(&messages, &names, second.next.as_ref(), two);
        assert_eq!(place(&third), [pair("gh_b", "T0"), pair("gh_b", "T1")]);
        assert_eq!(third.next, None);
    }

    #[test]
    fn a_cursor_wxread_did_not_write_is_refused_with_its_fix() {
        for text in ["", "x", "1:2:3", "1:2:3:", "x:1:0:gh_a", "1:2:-1:gh_a"] {
            let why = Cursor::parse(text).unwrap_err();
            assert!(why.contains("leave --cursor out"), "{text}: {why}");
        }
        let c = Cursor::parse("-4:2:3:gh_a:b").unwrap();
        assert_eq!(c.text(), "-4:2:3:gh_a:b");
    }

    proptest! {
        #[test]
        fn paging_lists_every_article_once_in_order(
            pushes in proptest::collection::vec((0usize..3, 0i64..3, 1usize..4), 0..12),
            limit in 1usize..6,
        ) {
            let messages: Vec<Message> = pushes
                .iter()
                .enumerate()
                .map(|(i, &(account, at, n))| message(&format!("gh_{account}"), i as i64, at, &push(n)))
                .collect();
            let names = BTreeMap::new();
            let whole = page(&messages, &names, None, Limit::new(1000).unwrap());
            let mut paged = Vec::new();
            let mut after = None;
            loop {
                let p = page(&messages, &names, after.as_ref(), Limit::new(limit).unwrap());
                prop_assert!(p.articles.len() <= limit);
                paged.extend(p.articles);
                match p.next {
                    Some(next) => after = Some(Cursor::parse(&next.text()).unwrap()),
                    None => break,
                }
            }
            prop_assert_eq!(paged, whole.articles);
        }
    }
}
