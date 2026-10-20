use super::*;
use proptest::prelude::*;
use serde_json::Value;

const ID: &str = "58872695997@chatroom";
const NAME: &str = "wxread 退群测试";
const ME: &str = "sunfmin";
const RETRY: &str = "wxread group leave 58872695997@chatroom --yes";

/// The screen `name` under screens/, as `wxread group leave` reads it.
fn screen(name: &str) -> Screen {
    let path = format!("{}/screens/{name}.jsonl", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    let mut screen = Screen { width: 0, height: 0, lines: Vec::new(), dialog: None };
    let n = |v: &Value| u32::try_from(v.as_u64().unwrap()).unwrap();
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        if let Some(size) = v.get("window") {
            (screen.width, screen.height) = (n(&size[0]), n(&size[1]));
        } else if let Some(f) = v.get("dialog") {
            screen.dialog = Some(Frame { x: n(&f[0]), y: n(&f[1]), w: n(&f[2]), h: n(&f[3]) });
        } else {
            screen.lines.push(Line {
                text: v["text"].as_str().unwrap().to_string(),
                x: n(&v["x"]),
                y: n(&v["y"]),
                w: n(&v["w"]),
                h: n(&v["h"]),
            });
        }
    }
    screen
}

fn group(display_name: &str, name: &str, members: usize) -> Target {
    Target { id: ID.into(), display_name: display_name.into(), name: name.into(), members }
}

fn test_group() -> Target {
    group(NAME, NAME, 1)
}

/// Every click of `acts`.
fn clicks(acts: &[Act]) -> usize {
    acts.iter().filter(|a| matches!(a, Act::Click { .. })).count()
}

/// The path to leave `target` walked through `screens`, one per move: every
/// move it made, the last one first.
fn walk(target: Target, screens: &[&str]) -> Vec<Move> {
    let mut path = Path::new(target, RETRY);
    let mut moves = vec![path.first()];
    for name in screens {
        let screen = screen(name);
        match moves.last().unwrap() {
            Move::Then(..) => moves.push(path.seen(&screen)),
            done => panic!("moved on after {done:?}"),
        }
    }
    moves.reverse();
    moves
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
    assert_eq!(
        moves[1],
        Move::Then(vec![Act::Click { x: 1055, y: 749 }, Act::Wait(1000)], Look::Confirmation)
    );
    assert_eq!(
        moves[2],
        Move::Then(vec![Act::Click { x: 1166, y: 27 }, Act::Wait(1000)], Look::Info)
    );
    assert_eq!(moves[3], Move::Then(vec![Act::Key(Key::Return), Act::Wait(1200)], Look::Chat));
    let Move::Then(first, Look::Search) = &moves[4] else { panic!("{:?}", moves[4]) };
    assert_eq!(first[0], Act::Front);
    assert!(first.contains(&Act::Menu(MenuItem::Search)));
    assert!(first.contains(&Act::Paste(NAME.into())));
    assert_eq!(clicks(first), 0, "the search is pressed through the menus");
}

#[test]
fn a_chat_info_already_open_is_not_clicked_shut() {
    let moves = walk(test_group(), &["search", "info"]);
    assert_eq!(
        moves[0],
        Move::Then(vec![Act::Click { x: 1055, y: 749 }, Act::Wait(1000)], Look::Confirmation)
    );
}

#[test]
fn a_name_wrapped_onto_two_lines_is_read_whole() {
    let wrapped = "上海浦东新区周末亲子活动交流分享群（第二期）2026";
    let mut path = Path::new(group(wrapped, wrapped, 356), RETRY);
    path.step = Step::Look(Look::Confirmation);
    assert_eq!(
        path.seen(&screen("confirm-wrapped")),
        Move::Confirm(vec![Act::Click { x: 662, y: 460 }])
    );
}

#[test]
fn the_chat_list_behind_the_confirmation_is_not_read_into_it() {
    // confirm.jsonl has a timestamp of the chat list level with the question.
    let mut path = Path::new(test_group(), RETRY);
    path.step = Step::Look(Look::Confirmation);
    assert!(matches!(path.seen(&screen("confirm")), Move::Confirm(_)));
}

#[test]
fn a_search_box_without_the_name_stops_before_return() {
    let moves = walk(test_group(), &["search-empty"]);
    let (acts, line) = stopped(&moves[0]);
    assert!(acts.is_empty());
    assert!(
        line.starts_with("stopped on the search box: the search box does not hold the name"),
        "{line}"
    );
    assert!(line.contains("nothing was clicked after it"), "{line}");
    assert!(line.contains(&format!("`{RETRY}` again")), "{line}");
}

#[test]
fn an_unexpected_screen_stops_the_run_with_no_further_click() {
    for (screens, on) in [
        (&["search", "chat-other"][..], "the group's chat"),
        (&["search", "search-empty"], "the group's chat"),
        (&["search", "chat", "chat"], "its chat info"),
        (&["search", "chat", "info-renamed"], "its chat info"),
        (&["search", "chat", "info", "info"], "the confirmation"),
        (&["search", "chat", "info", "chat"], "the confirmation"),
    ] {
        let moves = walk(test_group(), screens);
        let (acts, line) = stopped(&moves[0]);
        assert!(acts.is_empty(), "{screens:?}: {acts:?}");
        assert!(line.starts_with(&format!("stopped on {on}: ")), "{screens:?}: {line}");
    }
}

#[test]
fn what_was_read_is_in_the_refusal() {
    let moves = walk(test_group(), &["search", "chat-other"]);
    let (_, line) = stopped(&moves[0]);
    assert!(
        line.contains(
            "the opened chat is not the group asked for (read there: wxread退群测试2(3))"
        ),
        "{line}"
    );
    let moves = walk(test_group(), &["search", "chat", "info-renamed"]);
    let (_, line) = stopped(&moves[0]);
    assert!(
        line.contains("Group Name is not the group's own name (read there: 楼盘推广)"),
        "{line}"
    );
}

#[test]
fn a_member_count_that_differs_is_another_group() {
    let moves = walk(group(NAME, NAME, 2), &["search", "chat"]);
    let (_, line) = stopped(&moves[0]);
    assert!(
        line.starts_with("stopped on the group's chat: the opened chat is not the group"),
        "{line}"
    );
}

#[test]
fn a_group_with_no_name_of_its_own_is_not_asked_its_group_name() {
    let moves = walk(group(NAME, "", 1), &["search", "chat", "info-renamed"]);
    assert!(matches!(moves[0], Move::Then(_, Look::Confirmation)), "{:?}", moves[0]);
}

#[test]
fn a_confirmation_quoting_another_group_is_cancelled() {
    let mut path = Path::new(test_group(), RETRY);
    path.step = Step::Look(Look::Confirmation);
    let next = path.seen(&screen("confirm-other"));
    let (acts, line) = stopped(&next);
    assert_eq!(acts, [Act::Click { x: 535, y: 460 }, Act::Wait(800)], "Cancel, not OK");
    assert!(line.starts_with("stopped on the confirmation: the confirmation does not quote the group's name (read there: wxread退群测试2); Cancel was clicked"), "{line}");
}

#[test]
fn no_leave_scrolls_the_chat_info_at_most_eight_times() {
    let mut path = Path::new(test_group(), RETRY);
    path.step = Step::Look(Look::Info);
    let info = screen("info-no-leave");
    for _ in 0..MAX_SCROLLS {
        assert_eq!(
            path.seen(&info),
            Move::Then(vec![Act::Scroll { x: 1050, y: 400 }, Act::Wait(500)], Look::Info)
        );
    }
    let last = path.seen(&info);
    let (acts, line) = stopped(&last);
    assert!(acts.is_empty());
    assert!(line.starts_with("stopped on its chat info: no Leave in the chat info"), "{line}");
}

#[test]
fn a_screen_after_the_path_ended_is_refused() {
    let mut path = Path::new(test_group(), RETRY);
    let _ = path.first();
    let _ = path.seen(&screen("search-empty"));
    let last = path.seen(&screen("chat"));
    let (acts, _) = stopped(&last);
    assert!(acts.is_empty());
}

#[test]
fn a_group_the_account_is_in_alone_by_its_name_is_a_target() {
    let members = vec![ME.to_string()];
    let target = target(ID, NAME, NAME, &members, ME, &[ID.to_string()]).unwrap();
    assert_eq!(target, test_group());
}

#[test]
fn a_group_the_account_is_not_in_is_refused() {
    let Refusal(line) = target(ID, NAME, NAME, &[], ME, &[ID.into()]).unwrap_err();
    assert_eq!(
        line,
        "the account is not a member of 58872695997@chatroom (wxread 退群测试): there is nothing to leave; `wxread group 58872695997@chatroom` answers who is"
    );
}

#[test]
fn a_display_name_another_group_shares_is_refused_naming_both() {
    let members = vec![ME.to_string(), "wxid_qin".into()];
    let same = ["19010950345@chatroom".to_string(), "17812931124@chatroom".into()];
    let Refusal(line) =
        target("19010950345@chatroom", "孙凤君、秦俊斌 @qinjunbin", "", &members, ME, &same)
            .unwrap_err();
    assert!(line.starts_with("more than one of the account's groups is shown as 孙凤君、秦俊斌 @qinjunbin (17812931124@chatroom, 19010950345@chatroom): nothing in WeChat's window tells them apart"), "{line}");
    assert!(line.contains("Remark"), "{line}");
}

#[test]
fn a_group_shown_by_no_name_is_refused() {
    let Refusal(line) = target(ID, " ", "", &[ME.into()], ME, &[]).unwrap_err();
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
fn the_plan_says_what_it_checks_and_that_the_messages_stay() {
    let said = plan(&test_group()).join("\n");
    for needle in [
        "left alone for 3 s",
        "wxread 退群测试 (1)",
        "Group Name reads wxread 退群测试",
        "Clear Chat History unticked",
        "give the front back",
    ] {
        assert!(said.contains(needle), "{needle} in {said}");
    }
    assert!(!plan(&group(NAME, "", 1)).join("\n").contains("Group Name"));
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
fn the_retry_line_keeps_the_account_asked_for() {
    assert_eq!(line(ID, None), RETRY);
    assert_eq!(
        line(ID, Some("a_1")),
        "wxread group leave 58872695997@chatroom --account a_1 --yes"
    );
}

#[test]
fn text_is_compared_without_spaces_quotes_or_full_width_parentheses() {
    assert_eq!(normalize(" “wxread 退群测试（园）” "), "wxread退群测试(园)");
    assert_eq!(normalize("Group Name"), "GroupName");
}

proptest! {
    #[test]
    fn normalizing_twice_is_normalizing_once(text in "\\PC{0,40}") {
        let once = normalize(&text);
        prop_assert_eq!(normalize(&once), once.clone());
        prop_assert!(!once.chars().any(char::is_whitespace));
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
        let mut path = Path::new(test_group(), RETRY);
        path.step = Step::Look(Look::Confirmation);
        let confirmed = matches!(path.seen(&confirm), Move::Confirm(_));
        prop_assert_eq!(confirmed, normalize(&quoted) == normalize(NAME));
    }
}
