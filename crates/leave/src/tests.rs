use super::*;
use proptest::prelude::*;
use serde_json::Value;

const ID: &str = "58872695997@chatroom";
const NAME: &str = "wxread 退群测试";
const ME: &str = "sunfmin";
const RETRY: &str = "wxread group leave 58872695997@chatroom --yes";
/// What [`walk`] is given in place of a screen when the step's patience
/// ran out.
const WAITED: &str = "(waited)";

/// The screen `name` under screens/, as `wxread group leave` reads it.
fn screen(name: &str) -> Screen {
    let path = format!("{}/screens/{name}.jsonl", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    let mut screen = Screen { width: 0, height: 0, lines: Vec::new(), overlays: Vec::new() };
    let n = |v: &Value| u32::try_from(v.as_u64().unwrap()).unwrap();
    let line = |v: &Value| Line {
        text: v["text"].as_str().unwrap().to_string(),
        x: n(&v["x"]),
        y: n(&v["y"]),
        w: n(&v["w"]),
        h: n(&v["h"]),
    };
    for text in text.lines() {
        let v: Value = serde_json::from_str(text).unwrap();
        if let Some(size) = v.get("window") {
            (screen.width, screen.height) = (n(&size[0]), n(&size[1]));
        } else if let Some(f) = v.get("overlay") {
            screen.overlays.push(Overlay {
                frame: Frame { x: n(&f[0]), y: n(&f[1]), w: n(&f[2]), h: n(&f[3]) },
                lines: v["lines"].as_array().unwrap().iter().map(line).collect(),
            });
        } else {
            screen.lines.push(line(&v));
        }
    }
    screen
}

fn said(sender: &str, minute: Option<u32>, text: &str) -> Said {
    Said { sender: sender.into(), minute, text: text.into() }
}

fn group(display_name: &str, name: &str, members: usize, recent: Vec<Said>) -> Target {
    Target { id: ID.into(), display_name: display_name.into(), name: name.into(), members, recent }
}

/// The test group, its one message the one chat.jsonl shows.
fn test_group() -> Target {
    group(NAME, NAME, 1, vec![said("", None, "You've joined the group chat.")])
}

/// The test group as it was when the recorded-* screens were read.
fn recorded_group() -> Target {
    group(NAME, NAME, 1, vec![said(ME, None, "你修改群名为“wxread 退群测试”")])
}

/// 智能通信定位圈—广域物联②群 as chat-dash.jsonl shows it.
fn dash_group(members: usize) -> Target {
    let name = "智能通信定位圈—广域物联②群";
    let recent = vec![
        said("定位器方案-小杜", Some(10 * 60 + 58), "@宠物定位-小周 有滴。"),
        said("电池-小米", Some(10 * 60 + 58), "请问有没有需要纽扣电池的公司？支持出口"),
        said("宠物定位-小周", Some(10 * 60 + 35), "请问有没有做宠物定位器的公司？目的国美国"),
        said("射频方案-老陈", Some(10 * 60 + 34), ""),
        said("投资-老黄", Some(9 * 60 + 58), "对标铂力特、华曙高科的项目[握手][握手]"),
    ];
    Target { id: "5308573596@chatroom".into(), ..group(name, name, members, recent) }
}

/// The ads chat-ads.jsonl shows, which two groups are fed alike.
fn ads() -> Vec<Said> {
    let ad = |text: &str| said("周边游小助手", Some(18 * 60 + 20), text);
    vec![
        ad("名额有限，先到先得"),
        ad("安吉大竹海徒步一日游，周日出发"),
        ad("周末千岛湖两日游，含住宿\n早鸟价399元，报名私聊"),
    ]
}

/// Every click of `acts`.
fn clicks(acts: &[Act]) -> usize {
    acts.iter().filter(|a| matches!(a, Act::Click { .. })).count()
}

/// The path to leave `target` (to tell its chat, when `rehearse`) walked
/// through `screens`, one per move, [`WAITED`] for a step's patience run
/// out: every move it made, the last one first.
fn walk_as(target: Target, rehearse: bool, screens: &[&str]) -> Vec<Move> {
    let mut path = Path::new(target, RETRY, rehearse);
    let mut moves = vec![path.first()];
    for name in screens {
        match moves.last().unwrap() {
            Move::Then(..) | Move::Again if *name == WAITED => moves.push(path.waited()),
            Move::Then(..) | Move::Again => moves.push(path.seen(&screen(name))),
            done => panic!("moved on after {done:?}"),
        }
    }
    moves.reverse();
    moves
}

fn walk(target: Target, screens: &[&str]) -> Vec<Move> {
    walk_as(target, false, screens)
}

fn stopped(last: &Move) -> (&[Act], &str) {
    match last {
        Move::Stop(acts, Refusal(line)) => (acts, line),
        other => panic!("expected a stop, got {other:?}"),
    }
}

#[test]
fn the_recorded_path_ends_in_ok_on_the_confirmation_that_quotes_the_group() {
    let moves = walk(test_group(), &["search", "chat", "info", "confirm"]);
    assert_eq!(moves[0], Move::Confirm(vec![Act::Click { x: 662, y: 460 }]));
    assert_eq!(moves[1], Move::Then(vec![Act::Click { x: 1055, y: 749 }], Look::Confirmation));
    assert_eq!(moves[2], Move::Then(vec![Act::Click { x: 1166, y: 27 }], Look::Info));
    // The first chat listed by the name, not the search of the internet.
    assert_eq!(moves[3], Move::Then(vec![Act::Click { x: 176, y: 105 }], Look::Chat));
    let Move::Then(first, Look::Search) = &moves[4] else { panic!("{:?}", moves[4]) };
    assert_eq!(first[0], Act::Front);
    assert!(first.contains(&Act::Menu(MenuItem::Search)));
    assert!(first.contains(&Act::Paste(NAME.into())));
    assert_eq!(clicks(first), 0, "the search is pressed through the menus");
}

#[test]
fn the_path_recorded_on_the_test_group_ends_in_ok() {
    // The screens of the run that left 57609122265@chatroom on 2026-10-20.
    let screens = ["recorded-search", "recorded-chat", "recorded-info", "recorded-confirm"];
    let moves = walk(recorded_group(), &screens);
    assert_eq!(moves[0], Move::Confirm(vec![Act::Click { x: 605, y: 664 }]));
    let clicked: Vec<&Act> = moves[1..4]
        .iter()
        .map(|m| match m {
            Move::Then(acts, _) => &acts[0],
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        clicked,
        [
            &Act::Click { x: 967, y: 899 },
            &Act::Click { x: 1058, y: 27 },
            &Act::Click { x: 181, y: 106 }
        ]
    );
}

#[test]
fn a_chat_info_already_open_is_not_clicked_shut() {
    let moves = walk(test_group(), &["search", "info-chat"]);
    assert_eq!(moves[0], Move::Then(vec![Act::Click { x: 1055, y: 749 }], Look::Confirmation));
}

#[test]
fn a_name_wrapped_onto_two_lines_is_read_whole() {
    let wrapped = "上海浦东新区周末亲子活动交流分享群（第二期）2026";
    let mut path = Path::new(group(wrapped, wrapped, 356, Vec::new()), RETRY, false);
    path.step = Step::Look(Look::Confirmation);
    assert_eq!(
        path.seen(&screen("confirm-wrapped")),
        Move::Confirm(vec![Act::Click { x: 662, y: 460 }])
    );
}

#[test]
fn the_chat_list_behind_the_confirmation_is_not_read_into_it() {
    // confirm.jsonl has a timestamp of the chat list level with the question.
    let mut path = Path::new(test_group(), RETRY, false);
    path.step = Step::Look(Look::Confirmation);
    assert!(matches!(path.seen(&screen("confirm")), Move::Confirm(_)));
}

#[test]
fn a_search_listing_nothing_yet_is_looked_at_again_then_stops_with_no_click() {
    // The search box does not hold the name: no paste landed. It is only
    // what was read: the matches listed, or none, decide.
    let moves = walk(test_group(), &["search-empty", "search-empty", WAITED]);
    assert_eq!(moves[1], Move::Again);
    assert_eq!(moves[2], Move::Again);
    let (acts, line) = stopped(&moves[0]);
    assert!(acts.is_empty());
    assert!(
        line.starts_with(
            "stopped on the search's matches: the search lists no chat by a name like it (read there: QSearch)"
        ),
        "{line}"
    );
    assert!(line.contains("nothing was clicked after it"), "{line}");
    assert!(line.contains(&format!("`{RETRY}` again")), "{line}");
}

#[test]
fn a_search_listing_no_chat_by_the_name_stops_with_no_click() {
    // Only a search of the internet is listed by the name: it is not a chat.
    let moves = walk(test_group(), &["search-no-chat", WAITED]);
    let (acts, line) = stopped(&moves[0]);
    assert!(acts.is_empty());
    assert!(line.contains("the search lists no chat by a name like it"), "{line}");
    assert!(
        line.contains("Internetsearchresults | wxread退群测试 | Q微信退群了怎样恢复"),
        "{line}"
    );
}

#[test]
fn a_chat_listed_by_the_name_cut_short_is_opened_and_told_by_its_messages() {
    // #71: 46264918477@chatroom's search, as the run that stopped on it
    // read it, its full-width parentheses read as ASCII and its name cut
    // short; its title read with one parenthesis full-width.
    let name = "朱富贵会员群A02（杭州区域）";
    let recent = vec![
        said("朱富贵-小美", None, "新品上市，欢迎品尝"),
        said("朱富贵-小美", None, "本周会员日全场八折，到店出示会员码"),
    ];
    let moves =
        walk_as(group(name, name, 168, recent), true, &["search-truncated", "chat-truncated"]);
    assert_eq!(moves[0], Move::Identified);
    assert_eq!(moves[1], Move::Then(vec![Act::Click { x: 203, y: 105 }], Look::Chat));
}

#[test]
fn a_name_read_with_its_dash_as_one_and_a_cursor_after_it_is_found_and_told() {
    // #76: 5308573596@chatroom, its — read as 一 (and 广 as 厂 in the
    // search box), a text cursor after it.
    let moves = walk(dash_group(500), &["search-dash", "chat-dash"]);
    assert_eq!(moves[0], Move::Then(vec![Act::Click { x: 1058, y: 27 }], Look::Info));
    assert_eq!(moves[1], Move::Then(vec![Act::Click { x: 217, y: 107 }], Look::Chat));
}

#[test]
fn groups_fed_the_same_ads_are_told_apart_by_their_member_count() {
    let five = group("周边游🚩户外旅行交友美食5群", "", 255, ads());
    let nineteen = group("周边游🚩户外旅行交友美食19群", "", 325, ads());
    let chat = screen("chat-ads");
    let seen = identify::identity(&chat, &five);
    assert_eq!((seen.members, seen.found, seen.compared), (Some(255), 3, 3));
    assert_eq!(seen.of(&five), Ok(()));
    assert_eq!(
        identify::identity(&chat, &nineteen).of(&nineteen),
        Err("its title shows 255 members, and the group has 325".to_string())
    );
}

#[test]
fn a_chat_of_images_only_is_told_by_its_senders_and_times() {
    let recent = vec![
        said("老王", Some(9 * 60 + 12), ""),
        said("小丽", Some(9 * 60 + 6), ""),
        said("阿强", Some(9 * 60 + 5), ""),
    ];
    let images = group("表情包交流群", "", 12, recent);
    let seen = identify::identity(&screen("chat-images"), &images);
    assert_eq!((seen.found, seen.compared), (3, 3));
    assert_eq!(seen.of(&images), Ok(()));
    // Senders and times nobody in the chat has: it cannot be told, and the
    // run stops, clicking nothing.
    let others = vec![said("小张", Some(14 * 60), ""), said("小李", Some(15 * 60), "")];
    let strangers = group("表情包交流群", "", 12, others);
    assert!(identify::identity(&screen("chat-images"), &strangers).of(&strangers).is_err());
}

#[test]
fn a_chat_whose_messages_are_not_the_groups_is_not_it() {
    let mut other = dash_group(500);
    for (n, said) in other.recent.iter_mut().enumerate() {
        said.text = format!("另一个群的第{n}条消息");
        said.sender = "另一个人".into();
        said.minute = None;
    }
    let seen = identify::identity(&screen("chat-dash"), &other);
    assert_eq!(seen.found, 0);
    assert_eq!(
        seen.of(&other),
        Err("0 of the group's 2 newest messages were found in the chat".into())
    );
}

#[test]
fn a_group_whose_messages_the_mac_does_not_hold_is_not_told() {
    let none = group(NAME, NAME, 1, Vec::new());
    let seen = identify::identity(&screen("chat"), &none);
    assert!(seen.of(&none).unwrap_err().contains("hold none of the group's messages"));
}

#[test]
fn a_rehearsal_clicks_nothing_past_opening_the_chat() {
    let moves = walk_as(test_group(), true, &["search", "chat"]);
    assert_eq!(moves[0], Move::Identified);
    let acts: Vec<&Act> = moves[1..]
        .iter()
        .flat_map(|m| match m {
            Move::Then(acts, _) => acts.iter(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(acts.iter().filter(|a| matches!(a, Act::Click { .. })).count(), 1);
}

#[test]
fn a_chat_that_is_not_the_group_is_passed_for_the_next_match() {
    // The first match opens a group of the name the account has left: its
    // title holds no member count. Once the step's patience runs out the
    // search is made again and the next match opened.
    let moves = walk(test_group(), &["search", "chat-left", WAITED, "search", "chat"]);
    assert_eq!(moves[0], Move::Then(vec![Act::Click { x: 1166, y: 27 }], Look::Info));
    assert_eq!(moves[1], Move::Then(vec![Act::Click { x: 176, y: 201 }], Look::Chat));
    assert!(
        matches!(&moves[2], Move::Then(acts, Look::Search) if acts.contains(&Act::Menu(MenuItem::Search)) && clicks(acts) == 0)
    );
    assert_eq!(moves[3], Move::Again);
}

#[test]
fn no_more_than_four_chats_by_the_name_are_opened() {
    let mut screens = vec!["search"];
    for _ in 0..MAX_MATCHES {
        screens.extend(["chat-left", WAITED, "search"]);
    }
    screens.pop();
    let moves = walk(test_group(), &screens);
    let (acts, line) = stopped(&moves[0]);
    assert!(acts.is_empty());
    assert!(line.starts_with("stopped on the group's chat: the chat opened is not told to be the group: its title shows no member count (read there: wxread退群测试 | Youveleftthegroupchat"), "{line}");
}

#[test]
fn a_member_count_that_differs_is_another_group() {
    let moves = walk(
        group(NAME, NAME, 2, test_group().recent),
        &["search", "chat", WAITED, "search", "chat-other", WAITED],
    );
    assert_eq!((&moves[1], &moves[4]), (&Move::Again, &Move::Again));
    assert!(matches!(&moves[0], Move::Then(_, Look::Search)), "{:?}", moves[0]);
}

#[test]
fn an_unexpected_screen_stops_the_run_with_no_further_click() {
    for (screens, on) in [
        (&["search", "chat", "chat", WAITED][..], "its chat info"),
        (&["search", "chat", "info", "info", WAITED], "the confirmation"),
        (&["search", "chat", "info", "chat", WAITED], "the confirmation"),
    ] {
        let moves = walk(test_group(), screens);
        let (acts, line) = stopped(&moves[0]);
        assert!(acts.is_empty(), "{screens:?}: {acts:?}");
        assert!(line.starts_with(&format!("stopped on {on}: ")), "{screens:?}: {line}");
    }
}

#[test]
fn a_chat_that_changes_under_its_chat_info_stops_the_run() {
    let moves = walk(test_group(), &["search", "chat", "info-other"]);
    let (acts, line) = stopped(&moves[0]);
    assert!(acts.is_empty());
    assert!(line.starts_with("stopped on its chat info: the chat changed under its chat info (read there: wxread退群测试2(3))"), "{line}");
}

#[test]
fn a_group_name_that_reads_otherwise_does_not_stop_the_leave() {
    // The Group Name is no longer compared: the chat was told to be the
    // group by its member count and messages.
    let moves = walk(test_group(), &["search", "chat", "info-renamed"]);
    assert!(matches!(moves[0], Move::Then(_, Look::Confirmation)), "{:?}", moves[0]);
}

#[test]
fn a_confirmation_quoting_another_group_is_cancelled() {
    let mut path = Path::new(test_group(), RETRY, false);
    path.step = Step::Look(Look::Confirmation);
    let next = path.seen(&screen("confirm-other"));
    let (acts, line) = stopped(&next);
    assert_eq!(acts, [Act::Click { x: 535, y: 460 }], "Cancel, not OK");
    assert!(line.starts_with("stopped on the confirmation: the confirmation does not quote the group's name (read there: wxread退群测试2); Cancel was clicked"), "{line}");
}

#[test]
fn a_confirmation_quoting_the_name_with_its_dash_misread_is_ok() {
    let mut path = Path::new(dash_group(500), RETRY, false);
    path.step = Step::Look(Look::Confirmation);
    let mut confirm = screen("confirm");
    for line in &mut confirm.lines {
        if line.text.starts_with('"') {
            line.text = "\"智能通信定位圈一广域物联②群\"".into();
        }
    }
    assert!(matches!(path.seen(&confirm), Move::Confirm(_)));
}

#[test]
fn no_leave_scrolls_the_chat_info_at_most_eight_times() {
    let mut path = Path::new(test_group(), RETRY, false);
    path.step = Step::Look(Look::Info);
    let info = screen("info-no-leave");
    for _ in 0..MAX_SCROLLS {
        assert_eq!(path.seen(&info), Move::Then(vec![Act::Scroll { x: 1050, y: 400 }], Look::Info));
    }
    let last = path.seen(&info);
    let (acts, line) = stopped(&last);
    assert!(acts.is_empty());
    assert!(line.starts_with("stopped on its chat info: no Leave in the chat info"), "{line}");
}

#[test]
fn a_screen_after_the_path_ended_is_refused() {
    let mut path = Path::new(test_group(), RETRY, false);
    let _ = path.first();
    let _ = path.seen(&screen("search-empty"));
    let _ = path.waited();
    let last = path.seen(&screen("chat"));
    let (acts, _) = stopped(&last);
    assert!(acts.is_empty());
}

#[test]
fn every_look_waits_a_few_seconds_at_most() {
    for look in [Look::Search, Look::Chat, Look::Info, Look::Confirmation] {
        assert!(look.patience() <= 4000, "{look:?}");
    }
    assert_eq!(
        [Look::Search, Look::Chat, Look::Info, Look::Confirmation].map(Look::step),
        ["search", "identify", "leave", "leave"]
    );
}

#[test]
fn a_group_the_account_is_in_is_a_target_with_its_newest_messages() {
    let members = vec![ME.to_string()];
    let recent: Vec<Said> = (0..12).map(|n| said(ME, None, &n.to_string())).collect();
    let target = target(ID, NAME, NAME, &members, ME, recent).unwrap();
    assert_eq!(target.members, 1);
    assert_eq!(target.recent.len(), COMPARED);
    assert_eq!(target.recent[0].text, "0", "newest first");
}

#[test]
fn a_group_the_account_is_not_in_is_refused() {
    let Refusal(line) = target(ID, NAME, NAME, &[], ME, Vec::new()).unwrap_err();
    assert_eq!(
        line,
        "the account is not a member of 58872695997@chatroom (wxread 退群测试): there is nothing to leave; `wxread group 58872695997@chatroom` answers who is"
    );
}

#[test]
fn a_group_shown_by_no_name_is_refused() {
    let Refusal(line) = target(ID, " ", "", &[ME.into()], ME, Vec::new()).unwrap_err();
    assert!(line.contains("has no name WeChat shows it by"), "{line}");
}

#[test]
fn each_blocker_names_its_fix() {
    let fine = WeChat::LoggedIn(vec!["sunfmin_d844".into()]);
    assert!(blockers(&[], &fine, "sunfmin_d844", RETRY).is_empty());
    let lines = |missing: &[Permission], wechat: &WeChat| -> Vec<String> {
        blockers(missing, wechat, "sunfmin_d844", RETRY).into_iter().map(|Refusal(l)| l).collect()
    };
    let all = lines(&Permission::ALL, &WeChat::NotRunning);
    assert_eq!(all.len(), 3);
    assert!(all[0].contains("System Settings › Privacy & Security › Accessibility"), "{}", all[0]);
    assert!(all[1].contains("Screen & System Audio Recording"), "{}", all[1]);
    assert_eq!(
        all[2],
        format!(
            "WeChat is not running: open WeChat, log in as sunfmin_d844, then run `{RETRY}` again"
        )
    );
    let other = lines(&[], &WeChat::LoggedIn(vec!["wxid_other_1a2b".into()]));
    assert_eq!(other.len(), 1);
    assert!(other[0].starts_with("WeChat is logged in as wxid_other_1a2b, not sunfmin_d844: log in to WeChat as sunfmin_d844, or pass --account"), "{}", other[0]);
    let nobody = lines(&[], &WeChat::LoggedIn(vec![]));
    assert!(nobody[0].starts_with("WeChat is running, logged in as no account"), "{}", nobody[0]);
}

#[test]
fn the_plan_says_how_the_chat_is_told_and_that_the_messages_stay() {
    let said = plan(&test_group(), false).join("\n");
    for needle in [
        "left alone for 1 s",
        "once a run",
        "its title shows 1 members, and most of the 1 newest messages",
        "Clear Chat History unticked",
        "give the front back",
    ] {
        assert!(said.contains(needle), "{needle} in {said}");
    }
    let rehearsed = plan(&test_group(), true).join("\n");
    assert!(rehearsed.contains("click nothing more"), "{rehearsed}");
    assert!(!rehearsed.contains("Leave"), "{rehearsed}");
}

#[test]
fn the_leave_is_told_by_membership_alone() {
    assert!(!left(&[ME.into(), "wxid_a".into()], ME));
    assert!(left(&["wxid_a".into()], ME));
    assert!(left(&[], ME));
    let Refusal(line) = not_confirmed(&test_group(), 30);
    assert!(line.contains("30 s later the databases still list the account"), "{line}");
}

#[test]
fn the_notice_the_account_wrote_after_ok_tells_it_left() {
    // 5308573596@chatroom on 2026-10-20: OK at 10:27:00, the notice at
    // 10:27:01, the membership 11 s after OK.
    let ok = 1_792_405_620;
    let notice = Newest { system: true, server_id: 0, sender: Some(ME.into()), at: ok + 1 };
    assert!(left_by_notice(&notice, ME, ok));
    for not in [
        Newest { at: ok - 60, ..notice.clone() },
        Newest { sender: Some("wxid_owner".into()), ..notice.clone() },
        Newest { server_id: 4814281552280097615, ..notice.clone() },
        Newest { system: false, ..notice.clone() },
        Newest { sender: None, ..notice.clone() },
    ] {
        assert!(!left_by_notice(&not, ME, ok), "{not:?}");
    }
}

#[test]
fn the_retry_line_keeps_the_account_asked_for() {
    assert_eq!(line(&[ID], None, Mode::Leave), RETRY);
    assert_eq!(
        line(&[ID], Some("a_1"), Mode::Leave),
        "wxread group leave 58872695997@chatroom --account a_1 --yes"
    );
    assert_eq!(
        line(&[ID], None, Mode::Rehearse),
        "wxread group leave 58872695997@chatroom --dry-run"
    );
}

#[test]
fn the_retry_line_names_every_group_in_order() {
    assert_eq!(
        line(&["1@chatroom", "2@chatroom"], None, Mode::Plan),
        "wxread group leave 1@chatroom 2@chatroom --yes"
    );
}

/// A group of the account's, `n`th of a run.
fn nth(n: usize) -> Target {
    Target {
        id: format!("{n}@chatroom"),
        display_name: format!("G{n}"),
        name: String::new(),
        members: 3,
        recent: Vec::new(),
    }
}

/// The outcomes of `ran`, without their failures.
fn outcomes<E>(ran: &[(Outcome, Option<E>)]) -> Vec<Outcome> {
    ran.iter().map(|(o, _)| *o).collect()
}

#[test]
fn a_plan_touches_nothing_and_each_group_is_planned() {
    let groups = [Some(nth(1)), None, Some(nth(3))];
    let mut tried = 0;
    let ran = run(&groups, Mode::Plan, |_: &Target| -> Result<Outcome, ()> {
        tried += 1;
        Ok(Outcome::Left(1))
    });
    assert_eq!(tried, 0);
    assert_eq!(outcomes(&ran), [Outcome::Planned, Outcome::NotMember, Outcome::Planned]);
}

#[test]
fn a_rehearsal_tries_every_group_past_one_not_identified() {
    let groups = [Some(nth(1)), Some(nth(2)), None, Some(nth(4))];
    let mut tried = Vec::new();
    let ran = run(&groups, Mode::Rehearse, |t: &Target| {
        tried.push(t.id.clone());
        if t.id == "2@chatroom" {
            Err("its title shows 4 members")
        } else {
            Ok(Outcome::Identified)
        }
    });
    assert_eq!(tried, ["1@chatroom", "2@chatroom", "4@chatroom"]);
    assert_eq!(
        outcomes(&ran),
        [Outcome::Identified, Outcome::NotIdentified, Outcome::NotMember, Outcome::Identified]
    );
    assert_eq!(ran[1].1, Some("its title shows 4 members"));
}

#[test]
fn a_failure_on_the_second_of_three_stops_the_run_there() {
    let groups = [Some(nth(1)), Some(nth(2)), Some(nth(3))];
    let mut tried = Vec::new();
    let ran = run(&groups, Mode::Leave, |t: &Target| {
        tried.push(t.id.clone());
        if t.id == "2@chatroom" { Err("its title did not read G2") } else { Ok(Outcome::Left(4)) }
    });
    assert_eq!(tried, ["1@chatroom", "2@chatroom"]);
    let done = outcomes(&ran);
    assert_eq!(done, [Outcome::Left(4), Outcome::Stopped, Outcome::NotTried]);
    assert_eq!(ran[1].1, Some("its title did not read G2"));
    assert_eq!(
        rest(&["1@chatroom", "2@chatroom", "3@chatroom"], &done),
        ["2@chatroom", "3@chatroom"]
    );
    // Run again, the databases no longer list the account in the first:
    // only the rest are tried.
    let again = [None, Some(nth(2)), Some(nth(3))];
    let mut tried = Vec::new();
    let ran = run(&again, Mode::Leave, |t: &Target| -> Result<Outcome, &str> {
        tried.push(t.id.clone());
        Ok(Outcome::Left(2))
    });
    assert_eq!(tried, ["2@chatroom", "3@chatroom"]);
    let done = outcomes(&ran);
    assert_eq!(done, [Outcome::NotMember, Outcome::Left(2), Outcome::Left(2)]);
    assert!(rest(&["1@chatroom", "2@chatroom", "3@chatroom"], &done).is_empty());
}

#[test]
fn each_outcome_has_its_name() {
    let names: Vec<&str> = [
        Outcome::Planned,
        Outcome::NotMember,
        Outcome::Identified,
        Outcome::NotIdentified,
        Outcome::Left(1),
        Outcome::Stopped,
        Outcome::NotTried,
    ]
    .iter()
    .map(|o| o.name())
    .collect();
    assert_eq!(
        names,
        ["planned", "not_member", "identified", "not_identified", "left", "stopped", "not_tried"]
    );
    assert_eq!((Outcome::Left(7).confirmed_in(), Outcome::Planned.confirmed_in()), (Some(7), None));
}

#[test]
fn next_says_what_to_run_now() {
    let ids = ["1@chatroom", "2@chatroom"];
    let planned = next(&ids, &[Outcome::Planned, Outcome::NotMember], None, false);
    assert!(planned.contains("--dry-run"), "{planned}");
    assert!(planned.contains("`wxread group leave 1@chatroom --yes`"), "{planned}");
    let blocked = next(&ids, &[Outcome::Planned, Outcome::Planned], None, true);
    assert!(blocked.starts_with("nothing was changed: fix each of blockers"), "{blocked}");
    let identified = next(&ids, &[Outcome::Identified, Outcome::Identified], None, false);
    assert!(
        identified.contains("run `wxread group leave 1@chatroom 2@chatroom --yes`"),
        "{identified}"
    );
    let unsure = next(&ids, &[Outcome::Identified, Outcome::NotIdentified], None, false);
    assert!(unsure.contains("not_identified"), "{unsure}");
    let none = next(&ids, &[Outcome::NotMember, Outcome::NotMember], None, false);
    assert!(none.starts_with("nothing to leave: the account is in none of them"), "{none}");
    let done = next(&ids, &[Outcome::Left(1), Outcome::NotMember], Some("a_1"), false);
    assert!(done.contains("`wxread group 1@chatroom`"), "{done}");
    let stopped = next(&ids, &[Outcome::Left(1), Outcome::Stopped], Some("a_1"), false);
    assert!(stopped.contains("`wxread group leave 2@chatroom --account a_1 --yes`"), "{stopped}");
    assert!(stopped.contains("the same command again"), "{stopped}");
}

#[test]
fn text_is_compared_without_spaces_quotes_or_full_width_parentheses() {
    assert_eq!(normalize(" “wxread 退群测试（园）” "), "wxread退群测试(园)");
    assert_eq!(normalize("Group Name"), "GroupName");
}

#[test]
fn full_width_punctuation_is_compared_as_its_ascii() {
    assert_eq!(normalize("【群】：一，二！？；Ａ１"), "[群]:一,二!?;A1");
    assert_eq!(normalize("＂引＂＇号＇"), "引号");
}

#[test]
fn dashes_circled_numbers_and_cursors_fold_together() {
    // #76: — read as 一; a text cursor after the name.
    assert_eq!(fold("智能通信定位圈—广域物联②群"), fold("智能通信定位圈一广域物联2群|"));
    assert_eq!(fold("搞笑段子互助小组-好段子要分享"), fold("搞笑段子互助小组—好段子要分享"));
    assert_eq!(fold("捞粉专属｜海底捞"), "捞粉专属海底捞");
    assert_eq!(fold("⑱"), "18");
}

#[test]
fn a_name_read_close_to_it_or_cut_short_is_listed_by_it() {
    let name = fold("朱富贵会员群A02（杭州区域）");
    for read in ["朱富贵会员群A02(杭州区.", "朱富贵会员群A02(杭州区...", "朱富贵会员群A02（杭州区…"]
    {
        assert!(closeness(&fold(read), &name).is_some(), "{read}");
    }
    assert_eq!(closeness(&fold("朱富贵会员群A02(杭州区域)"), &name), Some(0));
    // 广 read as 厂: one character of fourteen.
    let dash = fold("智能通信定位圈—广域物联②群");
    assert_eq!(closeness(&fold("智能通信定位圈一厂域物联②群"), &dash), Some(1));
    for read in
        [".", "…", "朱富贵会员群A02(杭州区域)二.", "孙凤君、秦俊斌", "Internetsearchresults"]
    {
        assert!(closeness(&fold(read), &name).is_none(), "{read}");
    }
}

#[test]
fn two_messages_of_the_same_text_are_found_one_above_the_other() {
    // 18294283211@chatroom posts its notice again and again: the older one
    // is not found in the lines of the newer.
    let notice = "周边游线路安排\n地铁集合友好\n10.25莫干山红叶季";
    let lines: Vec<String> = [
        "周边游线路安排",
        "地铁集合友好",
        "10.25莫干山红叶季",
        "16:34",
        "小V",
        "周边游线路安排",
        "地铁集合友好",
        "10.25莫干山红叶季",
    ]
    .iter()
    .map(|l| fold(l))
    .collect();
    let recent = vec![said("小V", None, notice), said("小V", None, notice), said("小V", None, "")];
    assert_eq!(identify::in_order(&recent, &lines), (2, 2));
}

#[test]
fn emoji_fold_away_so_a_name_read_with_them_misread_is_close() {
    // 🇻🇳🇨🇳 read as ★圓 in the search's list on 2026-10-20.
    let name = fold("🇻🇳🇨🇳中越防失联交流群");
    assert_eq!(name, "中越防失联交流群");
    assert!(closeness(&fold("★圓中越防失联交流群"), &name).is_some());
    assert_eq!(fold("周边游🚩户外0️⃣元"), "周边游户外0元");
}

#[test]
fn a_line_cut_short_or_misread_is_found_in_its_messages_text() {
    let card = said(
        "霍曼-小芬",
        None,
        "开着依维柯，带着全家去撒欢｜22.99万，威尔特芬C型房车，6座4卧\n苏州霍曼房车旗舰店",
    );
    // Cut short at the bubble's edge, its ellipsis read as `.`.
    assert!(identify::shows(&card, &fold("22.99万,威尔特芬C型房车,6.")));
    // An emoji read as `17` before a notice's line, 广 read as 厂.
    let notice = said("小V", None, "🍁10.25莫干山红叶季68元起\n广域物联方案");
    assert!(identify::shows(&notice, &fold("1710.25莫干山红叶季68元起")));
    assert!(identify::shows(&notice, &fold("厂域物联方案")));
    // A line of another message is not found in it.
    assert!(!identify::shows(&notice, &fold("请问有没有做宠物定位器的公司")));
    // Short lines must be found exactly.
    assert!(!identify::shows(&notice, &fold("干活")));
    assert!(identify::shows(&notice, &fold("莫干")));
}

proptest! {
    #[test]
    fn normalizing_twice_is_normalizing_once(text in "\\PC{0,40}") {
        let once = normalize(&text);
        prop_assert_eq!(normalize(&once), once.clone());
        prop_assert!(!once.chars().any(char::is_whitespace));
    }

    #[test]
    fn folding_twice_is_folding_once(text in "\\PC{0,40}") {
        let once = fold(&text);
        prop_assert_eq!(fold(&once), once);
    }

    #[test]
    fn a_name_is_closest_to_itself(name in "[a-z退群测试0-9]{1,20}") {
        prop_assert_eq!(closeness(&name, &name), Some(0));
    }

    #[test]
    fn a_frame_holds_exactly_the_points_inside_it(
        x in 0u32..2000, y in 0u32..2000, w in 0u32..500, h in 0u32..500,
        px in 0u32..3000, py in 0u32..3000,
    ) {
        let inside = px >= x && py >= y && px <= x + w && py <= y + h;
        prop_assert_eq!(Frame { x, y, w, h }.contains((px, py)), inside);
    }

    #[test]
    fn no_screen_makes_the_path_click_ok_unless_it_quotes_the_group(
        quoted in "[a-z退群测试 ]{0,12}",
    ) {
        let mut confirm = screen("confirm");
        for line in &mut confirm.lines {
            if line.text.starts_with('"') {
                line.text = quoted.clone();
            }
        }
        let mut path = Path::new(test_group(), RETRY, false);
        path.step = Step::Look(Look::Confirmation);
        let confirmed = matches!(path.seen(&confirm), Move::Confirm(_));
        prop_assert_eq!(confirmed, fold(&quoted) == fold(NAME));
    }
}
