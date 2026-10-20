//! What `wxread group leave` decides: whether the group may be left at all,
//! what stops the run before WeChat's window is touched, the steps through
//! the window, how each screen is told by the text macOS recognizes on it,
//! which group a chat opened is (by its member count and newest messages,
//! [`identify`]), and every refusal with its fix
//! (docs/adr/0015-a-group-is-left-by-driving-wechats-window-only-with-yes.md,
//! docs/adr/0017-the-group-opened-is-told-by-its-member-count-and-newest-messages.md,
//! docs/leave-group.md). The effect crate reads the group and the host,
//! takes each screenshot and sends each key and click a [`Move`] names;
//! nothing here touches anything.

use flux_rs::attrs::*;
use wxread_doctor::Permission;

pub mod identify;

pub use identify::{COMPARED, Identity, Said, closeness, fold};

/// How long the keyboard and mouse must have been left alone before the
/// window is first touched, and how long that is waited for at most: once
/// a run, before its first group.
pub const IDLE_SECONDS: u64 = 1;
pub const IDLE_WAIT_SECONDS: u64 = 30;
/// How long the databases are read for the account to be gone from the
/// group's members after OK, and how often.
pub const LEFT_WAIT_SECONDS: u64 = 30;
pub const POLL_MILLIS: u64 = 200;
/// How many times the chat info is scrolled looking for Leave.
pub const MAX_SCROLLS: u32 = 8;
/// How many of the chats a search lists by the name are opened, looking
/// for the group: the others are groups of a name like it, or a contact.
pub const MAX_MATCHES: u32 = 4;

/// Why it stopped, and the fix, as the one line the run answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

/// The command line that does to `ids`, in order, what `mode` does (a
/// plan's is a leave's), with the `--account` it was given: what a fix
/// says to run, and what the plan says would leave them.
pub fn line(ids: &[&str], account: Option<&str>, mode: Mode) -> String {
    let ids = ids.join(" ");
    let flag = if mode == Mode::Rehearse { "--dry-run" } else { "--yes" };
    match account {
        Some(account) => format!("wxread group leave {ids} --account {account} {flag}"),
        None => format!("wxread group leave {ids} {flag}"),
    }
}

/// What a run does with the groups it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Without --yes or --dry-run: say what leaving them would do; touch
    /// nothing.
    Plan,
    /// --dry-run: open each one's chat and tell whether it is the group,
    /// every one, clicking nothing past opening it.
    Rehearse,
    /// --yes: leave each in turn, stopping at the first failure.
    Leave,
}

/// What became of one group of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Without --yes: the account is in it, and --yes would leave it.
    Planned,
    /// The account is not among its members: nothing to leave, whether it
    /// never was or a run before left it.
    NotMember,
    /// --dry-run opened its chat and told it is the group.
    Identified,
    /// --dry-run did not tell its chat is the group: its error says what
    /// was read, and what did not match.
    NotIdentified,
    /// This run left it, the databases confirming it this many seconds
    /// after OK.
    Left(u64),
    /// Leaving it failed, which ended the run.
    Stopped,
    /// A group before it stopped the run, so it was not touched.
    NotTried,
}

impl Outcome {
    /// What the answer calls it.
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Planned => "planned",
            Outcome::NotMember => "not_member",
            Outcome::Identified => "identified",
            Outcome::NotIdentified => "not_identified",
            Outcome::Left(_) => "left",
            Outcome::Stopped => "stopped",
            Outcome::NotTried => "not_tried",
        }
    }

    /// The seconds the databases took to confirm a group left.
    pub fn confirmed_in(self) -> Option<u64> {
        match self {
            Outcome::Left(seconds) => Some(seconds),
            _ => None,
        }
    }

    /// Whether the account is still to leave the group after this run:
    /// what a run again is to do.
    fn remains(self) -> bool {
        !matches!(self, Outcome::NotMember | Outcome::Left(_))
    }
}

/// The run of `groups`, in order, each its target when the account is
/// among its members, else None, as `mode` says: a plan touches none; a
/// rehearsal has `go` open each, every one, and a failure only marks its
/// group; a leave has `go` leave each in turn, and its first failure ends
/// the run, every group after it not tried. `go` answers the outcome it
/// came to (identified, or left), or why it failed. A group the account is
/// not in is passed over, so a run again of the same groups leaves only
/// the rest. Each group's failure, if any, is answered beside its outcome.
pub fn run<E>(
    groups: &[Option<Target>],
    mode: Mode,
    mut go: impl FnMut(&Target) -> Result<Outcome, E>,
) -> Vec<(Outcome, Option<E>)> {
    let mut ran = Vec::new();
    let mut stopped = false;
    for group in groups {
        let done = match group {
            None => (Outcome::NotMember, None),
            Some(_) if mode == Mode::Plan => (Outcome::Planned, None),
            Some(_) if stopped => (Outcome::NotTried, None),
            Some(target) => match go(target) {
                Ok(outcome) => (outcome, None),
                Err(why) if mode == Mode::Rehearse => (Outcome::NotIdentified, Some(why)),
                Err(why) => {
                    stopped = true;
                    (Outcome::Stopped, Some(why))
                }
            },
        };
        ran.push(done);
    }
    ran
}

/// The groups of `ids` whose `outcomes` leave them still to leave, in
/// order.
pub fn rest<'a>(ids: &[&'a str], outcomes: &[Outcome]) -> Vec<&'a str> {
    let mut rest = Vec::new();
    for (at, id) in ids.iter().enumerate() {
        if outcomes.get(at).is_some_and(|o| o.remains()) {
            rest.push(*id);
        }
    }
    rest
}

/// What to do once a run of `ids` ended in `outcomes`, asked with
/// `--account` `account`; `blocked` when blockers stop it.
pub fn next(ids: &[&str], outcomes: &[Outcome], account: Option<&str>, blocked: bool) -> String {
    let rest = rest(ids, outcomes);
    let retry = line(&rest, account, Mode::Leave);
    if outcomes.contains(&Outcome::Stopped) {
        return format!(
            "fix what error names, then run `{retry}` to leave the rest; or the same command again: the groups the account has left are passed over"
        );
    }
    if outcomes.contains(&Outcome::NotIdentified) {
        return "nothing was changed: each group not_identified has an error saying what its chat showed and what did not match; leave those in WeChat itself, or run --dry-run again once its chat shows its newest messages, and leave out of --yes any still not identified"
            .to_string();
    }
    if outcomes.contains(&Outcome::Identified) {
        return format!(
            "nothing was changed, and the chat of each one is the group: run `{retry}` to leave them; it cannot be undone"
        );
    }
    if outcomes.contains(&Outcome::Planned) {
        if blocked {
            return "nothing was changed: fix each of blockers, then run it again".to_string();
        }
        return format!(
            "nothing was changed: run it with --dry-run to check each one's chat is told apart, then `{retry}` to leave them; it cannot be undone"
        );
    }
    let mut left = Vec::new();
    for (at, id) in ids.iter().enumerate() {
        if outcomes.get(at).is_some_and(|o| o.confirmed_in().is_some()) {
            left.push(format!("`wxread group {id}`"));
        }
    }
    if left.is_empty() {
        return "nothing to leave: the account is in none of them; `wxread group <id>` answers who is"
            .to_string();
    }
    format!(
        "the account is in none of them now: {} answers its members, the account no longer among them; to rejoin one, a member must invite the account back",
        left.join(", ")
    )
}

/// The group to leave, as the databases answer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub id: String,
    /// What WeChat shows it as, and so what its window is searched for.
    pub display_name: String,
    /// Its own name; "" for none.
    pub name: String,
    /// How many members it has, the account among them: the count its
    /// chat's title shows.
    pub members: usize,
    /// Its newest messages, newest first, at most [`COMPARED`]: what its
    /// chat shows at its bottom.
    pub recent: Vec<Said>,
}

/// The group `id` as the target of a leave, when the account `own` is
/// among its `members`; `recent` are its newest messages, newest first.
pub fn target(
    id: &str,
    display_name: &str,
    name: &str,
    members: &[String],
    own: &str,
    recent: Vec<Said>,
) -> Result<Target, Refusal> {
    if !members.iter().any(|m| m == own) {
        return Err(Refusal(format!(
            "the account is not a member of {id} ({display_name}): there is nothing to leave; `wxread group {id}` answers who is"
        )));
    }
    if normalize(display_name).is_empty() {
        return Err(Refusal(format!(
            "group {id} has no name WeChat shows it by, so its window cannot be searched for it: leave it in WeChat itself"
        )));
    }
    let mut recent = recent;
    recent.truncate(COMPARED);
    Ok(Target {
        id: id.to_string(),
        display_name: display_name.to_string(),
        name: name.to_string(),
        members: members.len(),
        recent,
    })
}

/// What the running WeChat is, as the effect crate found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeChat {
    NotRunning,
    /// Running, with the databases of these accounts open: the account
    /// logged in, by its directory's name; none before anyone logs in.
    LoggedIn(Vec<String>),
}

/// What stops a run of `account`'s groups before the window is touched:
/// each permission `missing`, and WeChat not running or logged in as
/// another account; `retry` is the line to run again.
pub fn blockers(
    missing: &[Permission],
    wechat: &WeChat,
    account: &str,
    retry: &str,
) -> Vec<Refusal> {
    let mut found: Vec<Refusal> =
        missing.iter().map(|p| Refusal(format!("{}: {}", p.problem(), p.fix()))).collect();
    match wechat {
        WeChat::NotRunning => found.push(Refusal(format!(
            "WeChat is not running: open WeChat, log in as {account}, then run `{retry}` again"
        ))),
        WeChat::LoggedIn(accounts) if accounts.is_empty() => found.push(Refusal(format!(
            "WeChat is running, logged in as no account: log in as {account}, then run `{retry}` again"
        ))),
        WeChat::LoggedIn(accounts) if !accounts.iter().any(|a| a == account) => {
            found.push(Refusal(format!(
                "WeChat is logged in as {}, not {account}: log in to WeChat as {account}, or pass --account with the account WeChat is logged in as, then run it again",
                accounts.join(", ")
            )))
        }
        WeChat::LoggedIn(_) => {}
    }
    found
}

/// What the plan says the run does, in order, for `target`: up to telling
/// its chat is the group when `rehearse`, else up to leaving it.
pub fn plan(target: &Target, rehearse: bool) -> Vec<String> {
    let shown = &target.display_name;
    let mut steps = vec![
        format!(
            "once a run, before its first group: wait until the keyboard and mouse have been left alone for {IDLE_SECONDS} s (at most {IDLE_WAIT_SECONDS} s), and bring WeChat to the front, where it stays until the last group is done"
        ),
        format!(
            "search for {shown} (Edit › Search, then Edit › Paste, the pasteboard put back) and click the chat it lists whose name reads most like it; one that is not the group is passed for the next, up to {MAX_MATCHES}"
        ),
        format!(
            "tell the chat is the group: its title shows {} members, and most of the {} newest messages the databases hold of it are in the chat, in order",
            target.members,
            target.recent.len()
        ),
    ];
    if rehearse {
        steps.push("click nothing more: the chat is left open".to_string());
    } else {
        steps.extend([
            format!(
                "open its chat info (···), click Leave (退出群聊), then OK (确定) once the confirmation quotes {shown}, leaving Clear Chat History unticked: the chat's messages stay"
            ),
            format!(
                "read the databases every {POLL_MILLIS} ms, for at most {LEFT_WAIT_SECONDS} s, until they no longer list the account among its members"
            ),
        ]);
    }
    steps.push("after the last group, give the front back to the app that had it".to_string());
    steps
}

/// Whether the account `own` has left: it is not among `members`.
pub fn left(members: &[String], own: &str) -> bool {
    !members.iter().any(|m| m == own)
}

/// What is answered when the account is still a member `seconds` after OK.
pub fn not_confirmed(target: &Target, seconds: u64) -> Refusal {
    Refusal(format!(
        "OK was clicked on the confirmation, but {seconds} s later the databases still list the account among the members of {} ({}): the leave is not confirmed; run `wxread group {}` to see whether it has left since, and look at WeChat's window",
        target.id, target.display_name, target.id
    ))
}

/// Text as macOS recognizes it, made comparable: no whitespace, no
/// quotes, full-width punctuation, letters and digits made ASCII, `【】`
/// made `[]`. It reads `wxread 退群测试` as `wxread退群测试`, `（园）` as
/// `(园)` and `：` as `:`.
pub fn normalize(text: &str) -> String {
    let mut made = String::new();
    for c in text.chars() {
        match narrow(c) {
            '"' | '“' | '”' | '\'' | '‘' | '’' | '「' | '」' => {}
            c if c.is_whitespace() => {}
            c => made.push(c),
        }
    }
    made
}

/// The ASCII form of a full-width `c` (`（` is `(`, `Ａ` is `A`), or `c`.
fn narrow(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(c).wrapping_sub(0xFEE0)).unwrap_or(c),
        '【' => '[',
        '】' => ']',
        c => c,
    }
}

/// One line of text recognized in a screenshot of WeChat's window, and
/// where: in the window's points, from its top left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Line {
    /// Its middle, where a click on it lands.
    pub fn center(&self) -> (u32, u32) {
        (self.x.saturating_add(self.w / 2), self.y.saturating_add(self.h / 2))
    }
}

/// A rectangle in the window's points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Frame {
    pub fn contains(&self, (x, y): (u32, u32)) -> bool {
        x >= self.x && y >= self.y && x - self.x <= self.w && y - self.y <= self.h
    }
}

/// What one look at WeChat's window showed: its size, the text in it, and
/// each window of WeChat's own over it (the matches a search lists, a
/// confirmation), which a screenshot of the main window does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub lines: Vec<Line>,
    pub overlays: Vec<Overlay>,
}

/// A window of WeChat's over its main one, and the text read in it, both
/// in the main window's points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub frame: Frame,
    pub lines: Vec<Line>,
}

/// Which screen a step expects: what a refusal says it stopped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Search,
    Chat,
    Info,
    Confirmation,
}

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Search => "the search's matches",
            Look::Chat => "the group's chat",
            Look::Info => "its chat info",
            Look::Confirmation => "the confirmation",
        }
    }

    /// The step of a leave its time is counted in, as the answer names it.
    pub fn step(self) -> &'static str {
        match self {
            Look::Search => "search",
            Look::Chat => "identify",
            Look::Info | Look::Confirmation => "leave",
        }
    }

    /// How long, in milliseconds, the window is looked at again for this
    /// screen to come before [`Path::waited`] is asked what then.
    pub fn patience(self) -> u64 {
        match self {
            Look::Search | Look::Chat => 4000,
            Look::Info | Look::Confirmation => 3000,
        }
    }
}

/// A key the run presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
}

/// A menu item the run presses through Accessibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Search,
    Paste,
}

impl MenuItem {
    /// Its menu and its title, with WeChat in English and in Chinese.
    pub fn titles(self) -> [(&'static str, &'static str); 2] {
        match self {
            MenuItem::Search => [("Edit", "Search"), ("编辑", "搜索")],
            MenuItem::Paste => [("Edit", "Paste"), ("编辑", "粘贴")],
        }
    }
}

/// One thing the effect crate does to WeChat's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Make WeChat the active app with its main window in front, and
    /// check it is.
    Front,
    Key(Key),
    Menu(MenuItem),
    /// Put the text where the focus is by Edit › Paste; the pasteboard is
    /// put back before the next act, or when the path ends.
    Paste(String),
    /// A click at a point of the window; WeChat is made the front first.
    Click {
        x: u32,
        y: u32,
    },
    /// The scroll wheel turned down, over a point of the window.
    Scroll {
        x: u32,
        y: u32,
    },
}

/// What to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Move {
    /// Do these, then take a screenshot and pass it to [`Path::seen`]: it
    /// should come to be this screen.
    Then(Vec<Act>, Look),
    /// The screen is not yet what the step waits for: look again, and
    /// once the look's [`Look::patience`] has passed, ask [`Path::waited`].
    Again,
    /// A rehearsal's end: the chat opened is the group.
    Identified,
    /// These click the confirmation's OK; the databases tell what came of it.
    Confirm(Vec<Act>),
    /// Do these (a confirmation's Cancel, or nothing), then answer the refusal.
    Stop(Vec<Act>, Refusal),
}

/// How many times something was done that is done a bounded number of
/// times (scrolling the chat info, opening a match): never past the bound
/// it was counted against, nor past 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[refined_by(n: int)]
#[invariant(n <= 8)]
pub struct Count {
    #[field(u32[n])]
    n: u32,
}

impl Count {
    #[spec(fn() -> Count[0])]
    pub fn none() -> Count {
        Count { n: 0 }
    }

    /// One more, when there may be one more within `bound`.
    #[spec(fn(Count[@n], bound: u32{bound <= 8}) -> Option<Count{m: m == n + 1 && m <= bound}>)]
    pub fn more(self, bound: u32) -> Option<Count> {
        if self.n < bound { Some(Count { n: self.n + 1 }) } else { None }
    }

    #[spec(fn(&Count[@n]) -> u32[n])]
    pub fn count(&self) -> u32 {
        self.n
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Start,
    Look(Look),
    Done,
}

/// The labels that tell each part of the window, in English and in Chinese.
const GROUP_NAME: [&str; 2] = ["GroupName", "群聊名称"];
const LEAVE: [&str; 2] = ["Leave", "退出群聊"];
const ASKS: [&str; 2] = ["leavethegroupchat", "退出群聊"];
const CANCEL: [&str; 2] = ["Cancel", "取消"];
const OK: [&str; 2] = ["OK", "确定"];
const CLEAR_HISTORY: [&str; 2] = ["ClearChatHistory", "聊天记录"];
/// How far right of the list's left edge a chat the search lists starts,
/// past its avatar: the headings and the searches of the internet start
/// nearer.
const MATCH_INDENT: u32 = 35;
/// The title row and the search box are above this, in points.
const TOP: u32 = 40;
/// The chat list is left of this; the chat's title and messages right of
/// [`TITLE_FROM`].
const LIST_TO: u32 = 330;
const TITLE_FROM: u32 = 360;
/// The chat info is the panel this wide at the window's right.
const PANEL: u32 = 300;
/// The `···` that opens the chat info, from the window's right and top.
const MORE_FROM_RIGHT: u32 = 34;
const MORE_FROM_TOP: u32 = 27;

/// What the step waiting for its screen saw last that was not it: what a
/// stop says if the screen never comes.
#[derive(Debug, Clone)]
struct Miss {
    look: Look,
    why: String,
    read: Vec<String>,
}

/// The path through WeChat's window to leave one group, or to tell its
/// chat is the group: each [`Move`] it answers is checked against the
/// screen that came of the one before.
#[derive(Debug, Clone)]
pub struct Path {
    target: Target,
    retry: String,
    rehearse: bool,
    step: Step,
    scrolls: Count,
    /// The matches of the search opened, and found not the group.
    opened: Count,
    missed: Option<Miss>,
    /// What the last chat opened showed of which group it is.
    identity: Option<Identity>,
}

impl Path {
    /// The path to leave `target`, or only to tell its chat when
    /// `rehearse`; `retry` is the line a refusal says to run again.
    pub fn new(target: Target, retry: &str, rehearse: bool) -> Path {
        Path {
            target,
            retry: retry.to_string(),
            rehearse,
            step: Step::Start,
            scrolls: Count::none(),
            opened: Count::none(),
            missed: None,
            identity: None,
        }
    }

    /// What the last chat opened showed of which group it is, if one was.
    pub fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    /// The first move: WeChat in front, its search holding the name.
    pub fn first(&mut self) -> Move {
        if self.step != Step::Start {
            return self.stop(Look::Search, "the path was started twice", &[]);
        }
        self.search()
    }

    /// WeChat in front, its search holding the name.
    fn search(&mut self) -> Move {
        self.step = Step::Look(Look::Search);
        self.missed = None;
        Move::Then(
            vec![
                Act::Front,
                Act::Key(Key::Escape),
                Act::Menu(MenuItem::Search),
                Act::Paste(self.target.display_name.clone()),
            ],
            Look::Search,
        )
    }

    /// The move after `screen`, the screenshot taken after the last move.
    pub fn seen(&mut self, screen: &Screen) -> Move {
        match self.step {
            Step::Look(Look::Search) => self.searched(screen),
            Step::Look(Look::Chat) => self.opened(screen),
            Step::Look(Look::Info) => self.info(screen),
            Step::Look(Look::Confirmation) => self.confirmation(screen),
            Step::Start | Step::Done => {
                self.stop(Look::Search, "a screen came where none was asked for", &[])
            }
        }
    }

    /// The move once the screen waited for has not come within the look's
    /// patience: the next match of the search when the chat opened was not
    /// the group, else a stop saying what was seen last.
    pub fn waited(&mut self) -> Move {
        let Step::Look(look) = self.step else {
            return self.stop(Look::Search, "a wait came where none was asked for", &[]);
        };
        if look == Look::Chat
            && let Some(more) = self.opened.more(MAX_MATCHES - 1)
        {
            self.opened = more;
            return self.search();
        }
        let miss = self.missed.take().unwrap_or(Miss {
            look,
            why: "it did not come".to_string(),
            read: Vec::new(),
        });
        self.stop(miss.look, &miss.why, &miss.read)
    }

    /// Not yet the screen `look` waits for, for `why`, having read `read`.
    fn miss(&mut self, look: Look, why: &str, read: Vec<String>) -> Move {
        self.missed = Some(Miss { look, why: why.to_string(), read });
        Move::Again
    }

    fn searched(&mut self, screen: &Screen) -> Move {
        let name = fold(&self.target.display_name);
        // The matches are listed in a window of their own, under headings
        // that change from one search to the next; a search of the internet
        // is listed too. The chats listed by a name like it are opened in
        // turn, closest first, until one is told to be the group.
        let mut matches = Vec::new();
        let mut read = texts(screen, |_, l| l.y < TOP && l.x < LIST_TO);
        for overlay in &screen.overlays {
            matches.extend(chats_named(&overlay.lines, &name));
            for line in &overlay.lines {
                read.push(normalize(&line.text));
            }
        }
        matches.sort_by_key(|(apart, l)| (*apart, l.y, l.x));
        // Opening a match can move it up the list, so the matches are taken
        // in turn, round the list.
        let turn = (self.opened.count() as usize).checked_rem(matches.len()).unwrap_or(0);
        let Some((_, found)) = matches.get(turn) else {
            return self.miss(Look::Search, "the search lists no chat by a name like it", read);
        };
        let (x, y) = found.center();
        self.step = Step::Look(Look::Chat);
        self.missed = None;
        Move::Then(vec![Act::Click { x, y }], Look::Chat)
    }

    fn opened(&mut self, screen: &Screen) -> Move {
        let identity = identify::identity(screen, &self.target);
        let verdict = identity.of(&self.target);
        let read = [identity.title.clone(), identity.read.clone()].concat();
        self.identity = Some(identity);
        if let Err(why) = verdict {
            return self.miss(
                Look::Chat,
                &format!("the chat opened is not told to be the group: {why}"),
                read,
            );
        }
        if self.rehearse {
            self.step = Step::Done;
            return Move::Identified;
        }
        self.step = Step::Look(Look::Info);
        self.missed = None;
        if group_name_label(screen).is_some() {
            // The ··· toggles the chat info: it is open already.
            return self.info(screen);
        }
        Move::Then(
            vec![Act::Click { x: screen.width.saturating_sub(MORE_FROM_RIGHT), y: MORE_FROM_TOP }],
            Look::Info,
        )
    }

    fn info(&mut self, screen: &Screen) -> Move {
        if group_name_label(screen).is_none() {
            return self.miss(Look::Info, "the chat info did not open", panel(screen));
        }
        // The chat told to be the group must still be the one open: its
        // title's member count is read again.
        let shown = identify::identity(screen, &self.target);
        match shown.members {
            None => return self.miss(Look::Info, "its title shows no member count", shown.title),
            Some(n) if n != self.target.members => {
                return self.stop(Look::Info, "the chat changed under its chat info", &shown.title);
            }
            Some(_) => {}
        }
        if let Some(leave) = in_panel(screen, &LEAVE) {
            let (x, y) = leave.center();
            self.step = Step::Look(Look::Confirmation);
            self.missed = None;
            return Move::Then(vec![Act::Click { x, y }], Look::Confirmation);
        }
        match self.scrolls.more(MAX_SCROLLS) {
            Some(more) => {
                self.scrolls = more;
                Move::Then(
                    vec![Act::Scroll {
                        x: screen.width.saturating_sub(PANEL / 2),
                        y: screen.height / 2,
                    }],
                    Look::Info,
                )
            }
            None => self.stop(Look::Info, "no Leave in the chat info", &panel(screen)),
        }
    }

    fn confirmation(&mut self, screen: &Screen) -> Move {
        let Some(dialog) = screen.overlays.first() else {
            return self.miss(Look::Confirmation, "no confirmation came up", Vec::new());
        };
        // Only the text inside the confirmation's own window: the chat list
        // behind it is level with it.
        let mut inside: Vec<&Line> = dialog.lines.iter().collect();
        for line in &screen.lines {
            if dialog.frame.contains(line.center()) {
                inside.push(line);
            }
        }
        let mut read = Vec::new();
        for line in &inside {
            read.push(normalize(&line.text));
        }
        let Some(ask) = first(&inside, 0, |l| holds(&l.text, &ASKS)) else {
            return self.miss(Look::Confirmation, "no confirmation came up", read);
        };
        let cancel = first(&inside, ask.y, |l| is(&l.text, &CANCEL));
        let ok = first(&inside, ask.y, |l| is(&l.text, &OK));
        let (Some(cancel), Some(ok)) = (cancel, ok) else {
            return self.miss(Look::Confirmation, "the confirmation's buttons were not read", read);
        };
        // The quoted name, on as many lines as it wraps to, from below the
        // question down to the Clear Chat History box, folded as a name is.
        let mid = ask.y.saturating_add(ask.h / 2);
        let bottom = first(&inside, ask.y, |l| holds(&l.text, &CLEAR_HISTORY)).unwrap_or(cancel);
        let end = bottom.y.saturating_sub(2);
        let mut quoted: Vec<&Line> = Vec::new();
        for line in &inside {
            if line.y > mid && line.y < end {
                quoted.push(line);
            }
        }
        quoted.sort_by_key(|l| (l.y, l.x));
        let mut named = String::new();
        for line in &quoted {
            named.push_str(&line.text);
        }
        if fold(&named) != fold(&self.target.display_name) {
            let (x, y) = cancel.center();
            let refusal = self.refusal(
                Look::Confirmation,
                "the confirmation does not quote the group's name",
                &[normalize(&named)],
                "Cancel was clicked",
            );
            return Move::Stop(vec![Act::Click { x, y }], refusal);
        }
        self.step = Step::Done;
        let (x, y) = ok.center();
        Move::Confirm(vec![Act::Click { x, y }])
    }

    /// Stopping on `look` for `why`, having read `read`: nothing more is done.
    fn stop(&mut self, look: Look, why: &str, read: &[String]) -> Move {
        Move::Stop(Vec::new(), self.refusal(look, why, read, "nothing was clicked after it"))
    }

    /// The refusal of stopping on `look` for `why`, having read `read`, and
    /// then done what `then` says.
    fn refusal(&mut self, look: Look, why: &str, read: &[String], then: &str) -> Refusal {
        self.step = Step::Done;
        let read = if read.is_empty() {
            String::new()
        } else {
            format!(" (read there: {})", read.join(" | "))
        };
        Refusal(format!(
            "stopped on {}: {why}{read}; {then}. WeChat's window did not read as docs/leave-group.md records: run `{}` again with the Mac left alone, or leave the group in WeChat itself",
            look.name(),
            self.retry
        ))
    }
}

/// The first of `lines` below `y` that `wanted` wants.
fn first<'a>(lines: &[&'a Line], y: u32, wanted: fn(&Line) -> bool) -> Option<&'a Line> {
    lines.iter().copied().find(|line| line.y > y && wanted(line))
}

/// The chats a search's matches `lines` list by a name like `name`, made
/// comparable ([`fold`]), each with how far it reads from it
/// ([`closeness`]): a line indented past its avatar from the list's left
/// edge.
fn chats_named<'a>(lines: &'a [Line], name: &str) -> Vec<(usize, &'a Line)> {
    let mut edge = u32::MAX;
    for line in lines {
        edge = edge.min(line.x);
    }
    let mut found = Vec::new();
    for line in lines {
        if line.x < edge.saturating_add(MATCH_INDENT) {
            continue;
        }
        if let Some(apart) = closeness(&fold(&line.text), name) {
            found.push((apart, line));
        }
    }
    found
}

/// Whether `text`, made comparable, is one of `labels`.
fn is(text: &str, labels: &[&str]) -> bool {
    let text = normalize(text);
    labels.iter().any(|l| text == *l)
}

/// Whether `text`, made comparable, holds one of `labels`.
fn holds(text: &str, labels: &[&str]) -> bool {
    let text = normalize(text);
    labels.iter().any(|l| text.contains(l))
}

/// The text of `screen`'s lines that `keep` keeps, made comparable.
fn texts(screen: &Screen, keep: fn(&Screen, &Line) -> bool) -> Vec<String> {
    let mut texts = Vec::new();
    for line in &screen.lines {
        if keep(screen, line) {
            texts.push(normalize(&line.text));
        }
    }
    texts
}

/// Whether `line` is in the chat info's panel, at the window's right.
fn at_right(screen: &Screen, line: &Line) -> bool {
    line.x > screen.width.saturating_sub(PANEL)
}

/// The first line of the panel that is one of `labels`.
fn in_panel<'a>(screen: &'a Screen, labels: &[&str]) -> Option<&'a Line> {
    screen.lines.iter().find(|line| at_right(screen, line) && is(&line.text, labels))
}

/// The chat info's Group Name label: the chat info is open when it is there.
fn group_name_label(screen: &Screen) -> Option<&Line> {
    in_panel(screen, &GROUP_NAME)
}

/// Whether the chat info is open, taking the window's right.
fn panel_open(screen: &Screen) -> bool {
    group_name_label(screen).is_some()
}

/// The panel's text, for a refusal to say what it read.
fn panel(screen: &Screen) -> Vec<String> {
    texts(screen, at_right)
}

#[cfg(test)]
mod tests;
