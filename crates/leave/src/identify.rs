//! Which group a chat opened in WeChat's window is: told by the member
//! count its title shows and by its newest messages as the chat shows them,
//! against what the databases hold of the group asked for; never by its
//! name read off the screen, which macOS reads wrong in ever new ways
//! (docs/adr/0017-the-group-opened-is-told-by-its-member-count-and-newest-messages.md).
//! The name is only searched for: [`closeness`] picks the chat a search
//! lists that reads most like it.

use crate::{Line, Screen, TITLE_FROM, TOP, Target, at_right, normalize, panel_open};

/// How many of the group's newest messages the databases hold are
/// compared with the chat.
pub const COMPARED: usize = 8;

/// One message of the group, as the databases hold it and as its chat
/// shows it: who sent it, at what minute of the day (the Mac's time), and
/// its text; "" for a message with none (an image, a sticker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub sender: String,
    /// Hours times sixty plus minutes, as the chat's times read; None when
    /// the databases give none.
    pub minute: Option<u32>,
    pub text: String,
}

/// What a look at an opened chat showed of which group it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The member count its title shows, when one was read.
    pub members: Option<usize>,
    /// Of the group's newest messages, how many were found in the chat, in
    /// order, newest at the bottom.
    pub found: usize,
    /// How many were compared: the newest down to the oldest one found.
    pub compared: usize,
    /// The chat's title, as read.
    pub title: Vec<String>,
    /// The text of the chat below its title, as read, top first.
    pub read: Vec<String>,
}

impl Identity {
    /// Whether the chat is `target`, or why not: its title's member count
    /// must be the group's, and most of its newest messages must be in the
    /// chat, in order (at least two, or the one it has).
    pub fn of(&self, target: &Target) -> Result<(), String> {
        match self.members {
            None => return Err("its title shows no member count".to_string()),
            Some(n) if n != target.members => {
                return Err(format!(
                    "its title shows {n} members, and the group has {}",
                    target.members
                ));
            }
            Some(_) => {}
        }
        if target.recent.is_empty() {
            return Err("the databases hold none of the group's messages to compare with the chat"
                .to_string());
        }
        let least = target.recent.len().min(2);
        if self.found < least || self.found.saturating_mul(2) <= self.compared {
            return Err(format!(
                "{} of the group's {} newest messages were found in the chat",
                self.found,
                self.compared.max(least)
            ));
        }
        Ok(())
    }
}

/// What `screen`, a look at a chat opened, shows of which group it is,
/// compared with `target`'s newest messages.
pub fn identity(screen: &Screen, target: &Target) -> Identity {
    let mut title: Vec<&Line> = Vec::new();
    let mut chat: Vec<&Line> = Vec::new();
    let open = panel_open(screen);
    for line in &screen.lines {
        if line.x <= TITLE_FROM || (open && at_right(screen, line)) {
            continue;
        }
        if line.y < TOP { title.push(line) } else { chat.push(line) }
    }
    title.sort_by_key(|l| l.x);
    chat.sort_by_key(|l| (l.y, l.x));
    let title: Vec<String> = title.iter().map(|l| normalize(&l.text)).collect();
    let folded: Vec<String> = chat.iter().map(|l| fold(&l.text)).collect();
    let (found, compared) = in_order(&target.recent, &folded);
    Identity {
        members: count(&fold(&title.concat())),
        found,
        compared,
        title,
        read: chat.iter().map(|l| normalize(&l.text)).collect(),
    }
}

/// How many of `recent`, newest first, are found in `lines`, made
/// comparable, top first: each older one above the one found before it.
/// And how many were compared: those down to the oldest found, the older
/// ones being above what the chat shows.
pub(crate) fn in_order(recent: &[Said], lines: &[String]) -> (usize, usize) {
    let mut below = lines.len();
    let mut found: usize = 0;
    let mut compared: usize = 0;
    for (at, said) in recent.iter().take(COMPARED).enumerate() {
        let mut hit = None;
        for (j, line) in lines.iter().enumerate().take(below) {
            if shows(said, line) {
                hit = Some(j);
            }
        }
        if let Some(mut j) = hit {
            // A message read on several lines: older ones are above them all.
            while let Some(above) = j.checked_sub(1)
                && lines.get(above).is_some_and(|line| shows(said, line))
            {
                j = above;
            }
            below = j;
            found = found.saturating_add(1);
            compared = at.saturating_add(1);
        }
    }
    (found, compared)
}

/// Whether `line`, made comparable, shows `said`: a part of its text, or,
/// for a message with no text, its sender's name or its minute. A line is
/// a part of the text when it is in it with at most a quarter of its
/// characters read otherwise, an ellipsis it was cut short with left off;
/// one of two or three characters must be in it as it is.
pub(crate) fn shows(said: &Said, line: &str) -> bool {
    let text = said_text(&said.text);
    if !text.is_empty() {
        let read: Vec<char> = line.chars().collect();
        let mut stem = read.as_slice();
        while let Some((last, rest)) = stem.split_last() {
            if !ELLIPSIS.contains(last) {
                break;
            }
            stem = rest;
        }
        if stem.len() < 2 {
            return line == text;
        }
        let text: Vec<char> = text.chars().collect();
        return within(stem, &text).saturating_mul(4) <= stem.len()
            || (text.len() >= 2 && read.windows(text.len()).any(|w| w == text.as_slice()));
    }
    let sender = fold(&said.sender);
    if !sender.is_empty() && names(line, &sender) {
        return true;
    }
    said.minute.is_some_and(|m| minute(line) == Some(m))
}

/// `text` made comparable, without the `[握手]` a chat draws as an emoji.
fn said_text(text: &str) -> String {
    let mut kept = String::new();
    let mut code = String::new();
    let mut inside = false;
    for c in fold(text).chars() {
        match c {
            '[' if !inside => inside = true,
            ']' if inside => {
                inside = false;
                if code.chars().count() > 6 {
                    kept.push('[');
                    kept.push_str(&code);
                    kept.push(']');
                }
                code.clear();
            }
            c if inside => code.push(c),
            c => kept.push(c),
        }
    }
    if inside {
        kept.push('[');
        kept.push_str(&code);
    }
    kept
}

/// The minute of the day a time read as `line` says (`17:40`,
/// `Yesterday 17:40`), if it is one.
fn minute(line: &str) -> Option<u32> {
    let (hours, minutes) = line.rsplit_once(':')?;
    let digits = |s: &str| -> Option<u32> {
        if s.is_empty() || s.len() > 2 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    let start = hours.len().saturating_sub(2);
    let tail = hours.get(start..)?;
    let h = digits(tail).or_else(|| digits(hours.get(hours.len().saturating_sub(1)..)?))?;
    let m = digits(minutes).filter(|_| minutes.len() == 2)?;
    if h >= 24 || m >= 60 {
        return None;
    }
    Some(h.saturating_mul(60).saturating_add(m))
}

/// The member count a title made comparable shows: the digits of the last
/// parentheses holding only digits, the closing one possibly unread.
fn count(title: &str) -> Option<usize> {
    let chars: Vec<char> = title.chars().collect();
    let mut found = None;
    for (at, c) in chars.iter().enumerate() {
        if *c != '(' {
            continue;
        }
        let digits: String =
            chars.iter().skip(at.saturating_add(1)).take_while(|d| d.is_ascii_digit()).collect();
        let after = chars.get(at.saturating_add(1).saturating_add(digits.len()));
        if !digits.is_empty() && after.is_none_or(|a| *a == ')') {
            found = digits.parse().ok();
        }
    }
    found
}

/// Whether a name read as `line` (made comparable) is `name`: close to
/// it, or a start of it cut short.
fn names(line: &str, name: &str) -> bool {
    closeness(line, name).is_some()
}

/// What a name cut short in a list or a title ends in, as macOS reads it:
/// `…`, `⋯`, or one or more `.`.
const ELLIPSIS: [char; 3] = ['.', '…', '⋯'];

/// How far `read`, made comparable, is from `name`, made comparable: the
/// characters to change to make one the other, when few enough that it
/// reads as the name (a quarter of it); a name cut short is compared with
/// as much of the name as it holds. None when it does not read as it.
pub fn closeness(read: &str, name: &str) -> Option<usize> {
    let read: Vec<char> = read.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut stem = read.as_slice();
    while let Some((last, rest)) = stem.split_last() {
        if !ELLIPSIS.contains(last) {
            break;
        }
        stem = rest;
    }
    let cut = stem.len() < read.len();
    let whole = name.as_slice();
    let against = if cut { whole.get(..stem.len()).unwrap_or(whole) } else { whole };
    if stem.is_empty() || (cut && stem.len() >= name.len()) {
        return None;
    }
    let apart = distance(stem, against);
    (apart.saturating_mul(4) <= against.len()).then_some(apart)
}

/// The fewest characters to insert, remove or change to make `part` into
/// some stretch of `text`: how far `part` is from being in it.
fn within(part: &[char], text: &[char]) -> usize {
    // Where the stretch starts and ends in `text` is free.
    let mut row: Vec<usize> = vec![0; text.len().saturating_add(1)];
    for (i, cp) in part.iter().enumerate() {
        let mut next: Vec<usize> = vec![i.saturating_add(1)];
        for (j, ct) in text.iter().enumerate() {
            let diagonal = row.get(j).copied().unwrap_or(usize::MAX);
            let up = row.get(j.saturating_add(1)).copied().unwrap_or(usize::MAX);
            let left = next.last().copied().unwrap_or(usize::MAX);
            let change = if cp == ct { diagonal } else { diagonal.saturating_add(1) };
            next.push(change.min(up.saturating_add(1)).min(left.saturating_add(1)));
        }
        row = next;
    }
    row.iter().copied().min().unwrap_or(0)
}

/// The characters to insert, remove or change to make `a` into `b`.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = vec![0];
    for (j, _) in b.iter().enumerate() {
        row.push(j.saturating_add(1));
    }
    for (i, ca) in a.iter().enumerate() {
        let mut next: Vec<usize> = vec![i.saturating_add(1)];
        for (j, cb) in b.iter().enumerate() {
            let diagonal = row.get(j).copied().unwrap_or(usize::MAX);
            let up = row.get(j.saturating_add(1)).copied().unwrap_or(usize::MAX);
            let left = next.last().copied().unwrap_or(usize::MAX);
            let change = if ca == cb { diagonal } else { diagonal.saturating_add(1) };
            next.push(change.min(up.saturating_add(1)).min(left.saturating_add(1)));
        }
        row = next;
    }
    row.last().copied().unwrap_or(0)
}

/// Text made comparable loosely, as [`normalize`] does and further: every
/// dash, and `一` (which macOS reads a dash as), made `-`; a circled number
/// made its digits; a text cursor `|` and every emoji (which macOS reads as
/// whatever it likes) dropped. Both sides of a comparison
/// are folded alike, so what folds together is only told apart by the
/// member count and the messages.
pub fn fold(text: &str) -> String {
    let mut folded = String::new();
    for c in normalize(text).chars() {
        match c {
            '—' | '–' | '―' | '‐' | '‑' | '一' | '－' | '﹣' => folded.push('-'),
            '|' | '｜' | '丨' => {}
            c if emoji(c) => {}
            '①'..='⑳' => {
                let n = u32::from(c).saturating_sub(u32::from('①')).saturating_add(1);
                folded.push_str(&n.to_string());
            }
            c => folded.push(c),
        }
    }
    folded
}

/// Whether `c` is drawn as an emoji, or is part of one: a flag's letters,
/// a pictograph, a symbol, the selector or joiner between them.
fn emoji(c: char) -> bool {
    matches!(
        u32::from(c),
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE00..=0xFE0F | 0x200D | 0x20E3
    )
}
