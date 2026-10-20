//! What `wxread group leave` decides: whether the group may be left at all,
//! what stops the run before WeChat's window is touched, the steps through
//! the window, how each screen is told by the text macOS recognizes on it,
//! and every refusal with its fix
//! (docs/adr/0011-a-group-is-left-by-driving-wechats-window-only-with-yes.md,
//! docs/leave-group.md). The effect crate reads the group and the host,
//! takes each screenshot and sends each key and click a [`Move`] names;
//! nothing here touches anything.

use flux_rs::attrs::*;
use wxread_doctor::Permission;

/// How long the keyboard and mouse must have been left alone before the
/// window is touched, and how long that is waited for at most.
pub const IDLE_SECONDS: u64 = 3;
pub const IDLE_WAIT_SECONDS: u64 = 60;
/// How long the databases are read, once a second, for the account to be
/// gone from the group's members after OK.
pub const LEFT_WAIT_SECONDS: u64 = 30;
/// How many times the chat info is scrolled looking for Leave.
pub const MAX_SCROLLS: u32 = 8;
/// How many of the chats a search lists by the name are opened, looking
/// for the group: the others are groups the account has left, or a contact
/// of the same name.
pub const MAX_MATCHES: u32 = 4;

/// Why it stopped, and the fix, as the one line the run answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

/// The command line that leaves `id`, with the `--account` it was given:
/// what a fix says to run, and what the plan says would leave it.
pub fn line(id: &str, account: Option<&str>) -> String {
    match account {
        Some(account) => format!("wxread group leave {id} --account {account} --yes"),
        None => format!("wxread group leave {id} --yes"),
    }
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
}

/// The group `id` as the target of a leave, when the account `own` is
/// among its `members` and no other group it is in is shown as it is:
/// `same_name` holds the id of every group the account is in that is shown
/// as `display_name`. A group it has left reads apart in the window: its
/// chat's title holds no member count.
pub fn target(
    id: &str,
    display_name: &str,
    name: &str,
    members: &[String],
    own: &str,
    same_name: &[String],
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
    let mut all: Vec<&str> = vec![id];
    for other in same_name {
        if other != id {
            all.push(other);
        }
    }
    if all.len() > 1 {
        all.sort_unstable();
        return Err(Refusal(format!(
            "more than one of the account's groups is shown as {display_name} ({}): nothing in WeChat's window tells them apart, so nothing was touched; give this one a remark in WeChat (its chat info › Remark) that no other group has, or leave it in WeChat itself",
            all.join(", ")
        )));
    }
    Ok(Target {
        id: id.to_string(),
        display_name: display_name.to_string(),
        name: name.to_string(),
        members: members.len(),
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

/// What stops a leave of `account`'s group before the window is touched:
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

/// What the plan says the run does, in order, for `target`.
pub fn plan(target: &Target) -> Vec<String> {
    let shown = &target.display_name;
    let mut steps = vec![
        format!(
            "wait until the keyboard and mouse have been left alone for {IDLE_SECONDS} s (at most {IDLE_WAIT_SECONDS} s)"
        ),
        "bring WeChat to the front".to_string(),
        format!(
            "search for {shown} (Edit › Search, then Edit › Paste, the pasteboard put back) and, once the search box holds it, click the chat it lists by that name; a group of that name the account has left is passed for the next, up to {MAX_MATCHES}"
        ),
        format!("check the chat's title reads {shown} ({})", target.members),
    ];
    if target.name.is_empty() {
        steps.push("open its chat info (···)".to_string());
    } else {
        steps.push(format!(
            "open its chat info (···) and check its Group Name reads {}",
            target.name
        ));
    }
    steps.extend([
        format!(
            "click Leave (退出群聊), then OK (确定) once the confirmation quotes {shown}, leaving Clear Chat History unticked: the chat's messages stay"
        ),
        format!(
            "read the databases once a second, for at most {LEFT_WAIT_SECONDS} s, until they no longer list the account among its members"
        ),
        "give the front back to the app that had it".to_string(),
    ]);
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
/// quotes, full-width parentheses made ASCII. It reads `wxread 退群测试` as
/// `wxread退群测试` and `（园）` as `(园)`.
pub fn normalize(text: &str) -> String {
    let mut made = String::new();
    for c in text.chars() {
        match c {
            '（' => made.push('('),
            '）' => made.push(')'),
            '"' | '“' | '”' | '\'' | '‘' | '’' | '「' | '」' => {}
            c if c.is_whitespace() => {}
            c => made.push(c),
        }
    }
    made
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
            Look::Search => "the search box",
            Look::Chat => "the group's chat",
            Look::Info => "its chat info",
            Look::Confirmation => "the confirmation",
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
    /// Put the text where the focus is by Edit › Paste, the pasteboard
    /// put back after.
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
    /// Milliseconds for WeChat to draw what changed.
    Wait(u64),
}

/// What to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Move {
    /// Do these, then take a screenshot and pass it to [`Path::seen`]: it
    /// should be this screen.
    Then(Vec<Act>, Look),
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
/// The chat list is left of this; the chat's title right of [`TITLE_FROM`].
const LIST_TO: u32 = 330;
const TITLE_FROM: u32 = 360;
/// The chat info is the panel this wide at the window's right.
const PANEL: u32 = 300;
/// The `···` that opens the chat info, from the window's right and top.
const MORE_FROM_RIGHT: u32 = 34;
const MORE_FROM_TOP: u32 = 27;

/// The path through WeChat's window to leave one group: each [`Move`] it
/// answers is checked against the screen that came of the one before.
#[derive(Debug, Clone)]
pub struct Path {
    target: Target,
    retry: String,
    step: Step,
    scrolls: Count,
    /// The matches of the search opened, and found not the group.
    opened: Count,
}

impl Path {
    /// The path to leave `target`; `retry` is the line a refusal says to
    /// run again.
    pub fn new(target: Target, retry: &str) -> Path {
        Path {
            target,
            retry: retry.to_string(),
            step: Step::Start,
            scrolls: Count::none(),
            opened: Count::none(),
        }
    }

    /// The title its chat shows: its display name and member count.
    pub fn title(&self) -> String {
        format!("{}({})", normalize(&self.target.display_name), self.target.members)
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
        Move::Then(
            vec![
                Act::Front,
                Act::Key(Key::Escape),
                Act::Menu(MenuItem::Search),
                Act::Wait(300),
                Act::Paste(self.target.display_name.clone()),
                Act::Wait(800),
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

    fn searched(&mut self, screen: &Screen) -> Move {
        let name = normalize(&self.target.display_name);
        let field = texts(screen, |_, l| l.y < TOP && l.x < LIST_TO);
        // The magnifier before the name and the clear button after it are
        // read as text too (`、`, `1`): the box holds the name, not is it.
        if !field.iter().any(|f| f.contains(&name)) {
            return self.stop(
                Look::Search,
                "the search box does not hold the name it was given",
                &field,
            );
        }
        // The matches are listed in a window of their own, under headings
        // that change from one search to the next; a search of the internet
        // is listed too. Each chat listed by the name is opened in turn, its
        // title telling whether it is the group.
        let mut matches = Vec::new();
        let mut read = Vec::new();
        for overlay in &screen.overlays {
            matches.extend(chats_named(&overlay.lines, &name));
            for line in &overlay.lines {
                read.push(normalize(&line.text));
            }
        }
        // Opening a match can move it up the list, so the matches are taken
        // in turn, round the list.
        let turn = (self.opened.count() as usize).checked_rem(matches.len()).unwrap_or(0);
        let Some(found) = matches.get(turn) else {
            return self.stop(Look::Search, "the search lists no chat by the name", &read);
        };
        let (x, y) = found.center();
        self.step = Step::Look(Look::Chat);
        Move::Then(vec![Act::Click { x, y }, Act::Wait(1200)], Look::Chat)
    }

    fn opened(&mut self, screen: &Screen) -> Move {
        let title = titles(screen);
        if !title.contains(&self.title()) {
            // A group of the same name the account has left: its title is
            // the name alone, with no member count. The next match, if any;
            // any other title stops the run.
            let left = title.contains(&normalize(&self.target.display_name));
            if let Some(more) = self.opened.more(MAX_MATCHES - 1).filter(|_| left) {
                self.opened = more;
                return self.search();
            }
            return self.stop(Look::Chat, "the opened chat is not the group asked for", &title);
        }
        if group_name_label(screen).is_some() {
            // The ··· toggles the chat info: it is open already.
            self.step = Step::Look(Look::Info);
            return self.info(screen);
        }
        self.step = Step::Look(Look::Info);
        Move::Then(
            vec![
                Act::Click { x: screen.width.saturating_sub(MORE_FROM_RIGHT), y: MORE_FROM_TOP },
                Act::Wait(1000),
            ],
            Look::Info,
        )
    }

    fn info(&mut self, screen: &Screen) -> Move {
        let Some(label) = group_name_label(screen) else {
            return self.stop(Look::Info, "the chat info did not open", &panel(screen));
        };
        let title = titles(screen);
        if !title.contains(&self.title()) {
            return self.stop(Look::Info, "the chat changed under its chat info", &title);
        }
        let own = normalize(&self.target.name);
        if !own.is_empty() {
            let value = below(screen, label).map(|l| normalize(&l.text));
            if value.as_deref() != Some(own.as_str()) {
                let read: Vec<String> = value.into_iter().collect();
                return self.stop(
                    Look::Info,
                    "its chat info's Group Name is not the group's own name",
                    &read,
                );
            }
        }
        if let Some(leave) = in_panel(screen, &LEAVE) {
            let (x, y) = leave.center();
            self.step = Step::Look(Look::Confirmation);
            return Move::Then(vec![Act::Click { x, y }, Act::Wait(1000)], Look::Confirmation);
        }
        match self.scrolls.more(MAX_SCROLLS) {
            Some(more) => {
                self.scrolls = more;
                Move::Then(
                    vec![
                        Act::Scroll {
                            x: screen.width.saturating_sub(PANEL / 2),
                            y: screen.height / 2,
                        },
                        Act::Wait(500),
                    ],
                    Look::Info,
                )
            }
            None => self.stop(Look::Info, "no Leave in the chat info", &panel(screen)),
        }
    }

    fn confirmation(&mut self, screen: &Screen) -> Move {
        let Some(dialog) = screen.overlays.first() else {
            return self.stop(Look::Confirmation, "no confirmation came up", &[]);
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
            return self.stop(Look::Confirmation, "no confirmation came up", &read);
        };
        let cancel = first(&inside, ask.y, |l| is(&l.text, &CANCEL));
        let ok = first(&inside, ask.y, |l| is(&l.text, &OK));
        let (Some(cancel), Some(ok)) = (cancel, ok) else {
            return self.stop(
                Look::Confirmation,
                "the confirmation's buttons were not read",
                &read,
            );
        };
        // The quoted name, on as many lines as it wraps to, from below the
        // question down to the Clear Chat History box.
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
            named.push_str(&normalize(&line.text));
        }
        if named != normalize(&self.target.display_name) {
            let (x, y) = cancel.center();
            let refusal = self.refusal(
                Look::Confirmation,
                "the confirmation does not quote the group's name",
                &[named],
                "Cancel was clicked",
            );
            return Move::Stop(vec![Act::Click { x, y }, Act::Wait(800)], refusal);
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

/// The chats a search's matches `lines` list by `name`, top first: each
/// reads exactly the name, indented past its avatar from the list's left
/// edge.
fn chats_named<'a>(lines: &'a [Line], name: &str) -> Vec<&'a Line> {
    let mut edge = u32::MAX;
    for line in lines {
        edge = edge.min(line.x);
    }
    let mut found: Vec<&Line> = Vec::new();
    for line in lines {
        if line.x >= edge.saturating_add(MATCH_INDENT) && normalize(&line.text) == name {
            found.push(line);
        }
    }
    found.sort_by_key(|l| (l.y, l.x));
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

/// The text of the chat's title row.
fn titles(screen: &Screen) -> Vec<String> {
    texts(screen, |_, l| l.y < TOP && l.x > TITLE_FROM)
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

/// The line of the panel right below `label`: its value.
fn below<'a>(screen: &'a Screen, label: &Line) -> Option<&'a Line> {
    let mut value: Option<&Line> = None;
    for line in &screen.lines {
        let higher = value.is_none_or(|v| line.y < v.y);
        if at_right(screen, line) && line.y > label.y && higher {
            value = Some(line);
        }
    }
    value
}

/// The panel's text, for a refusal to say what it read.
fn panel(screen: &Screen) -> Vec<String> {
    texts(screen, at_right)
}

#[cfg(test)]
mod tests;
