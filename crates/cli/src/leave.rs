//! `wxread group leave`: reads each group asked for and what macOS and
//! WeChat allow, once a run, and without `--yes` or `--dry-run` answers
//! what leaving them would do; with `--dry-run` opens each one's chat in
//! WeChat's window and answers whether it is told to be the group; with
//! `--yes` leaves them one after another, as [`wxread_leave::run`] orders,
//! each by driving WeChat's window along the path [`wxread_leave::Path`]
//! checks, and answers one left only once the databases no longer list the
//! account among its members
//! (docs/adr/0015-a-group-is-left-by-driving-wechats-window-only-with-yes.md,
//! docs/adr/0017-the-group-opened-is-told-by-its-member-count-and-newest-messages.md).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use wxread_answer::Limit;
use wxread_contacts::{Contacts, Group};
use wxread_leave::{
    COMPARED, IDLE_SECONDS, IDLE_WAIT_SECONDS, LEFT_WAIT_SECONDS, Look, Mode, Move, Newest,
    Outcome, POLL_MILLIS, Path, Refusal, Said, Target, WeChat,
};
use wxread_messages::{Order, Range, Start};
use wxread_schema::CONTACT_DB;

use crate::account::Account;
use crate::contacts::{contacts, group_in, members, own_id};
use crate::media::MEDIA_DB_PATHS;
use crate::messages::{self, MESSAGE_DBS, NoMessages, Store};
use crate::world::World;
use crate::{Failed, window};

/// The id that stands for the ids read from stdin.
pub const STDIN: &str = "-";

/// What `group leave` was asked: the groups, `--account`, `--yes` and
/// `--dry-run`.
pub struct Asked {
    pub ids: Vec<String>,
    pub account: Option<String>,
    pub yes: bool,
    pub dry_run: bool,
}

/// What `group leave` answers of `world`: the plan, what a rehearsal told
/// of each group's chat, or the groups left; a run that stops partway, or
/// a rehearsal that does not tell every one, fails with the same answer.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let began = Instant::now();
    let mode = match (asked.yes, asked.dry_run) {
        (true, true) => bail!(
            "--dry-run never leaves and --yes leaves: pass --dry-run to check each group's chat, then --yes alone to leave them"
        ),
        (true, false) => Mode::Leave,
        (false, true) => Mode::Rehearse,
        (false, false) => Mode::Plan,
    };
    let ids = ids(&asked.ids)?;
    let account =
        Account::open(world, asked.account.as_deref(), &[CONTACT_DB, MESSAGE_DBS, MEDIA_DB_PATHS])?;
    // Every database is read out of its file once a run, here: only the
    // membership after a leave is read again.
    let contacts = contacts(&account, false)?;
    let own = own_id(&account, &contacts);
    let contact_db = account.database(CONTACT_DB)?;
    let store = Store::open(&account)?;
    let mut read = Vec::new();
    let mut faults = Vec::new();
    // The message databases holding each group's chat: where its notice of
    // the account leaving is looked for.
    let mut holding: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &ids {
        match target(&contact_db, &contacts, &own, &store, id) {
            Ok(target) => read.push(target),
            Err(e) => faults.push(format!("{e:#}")),
        }
        holding.insert(id.clone(), store.holding(id)?);
    }
    if !faults.is_empty() {
        bail!("{}", faults.join("; and "));
    }
    let read_in = began.elapsed();
    let mut groups: Vec<Value> =
        read.iter().map(|(group, last, _)| group_json(group, last)).collect();
    let targets: Vec<Option<Target>> = read.into_iter().map(|(_, _, target)| target).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let account_flag = asked.account.as_deref();
    let to_go: Vec<&str> = targets.iter().flatten().map(|t| t.id.as_str()).collect();
    let blockers = if to_go.is_empty() {
        Vec::new()
    } else {
        let (dir, _) = world.data_dir();
        let wechat = match window::wechat() {
            None => WeChat::NotRunning,
            Some(pid) => WeChat::LoggedIn(window::accounts_open(pid, &PathBuf::from(dir))),
        };
        let retry = wxread_leave::line(&to_go, account_flag, mode);
        wxread_leave::blockers(&window::missing(), &wechat, account.id(), &retry)
    };
    if mode != Mode::Plan
        && let Some(Refusal(first)) = blockers.first()
    {
        let rest: Vec<&str> = blockers.iter().skip(1).map(|Refusal(l)| l.as_str()).collect();
        let more =
            if rest.is_empty() { String::new() } else { format!("; and {}", rest.join("; and ")) };
        bail!("nothing was touched: {first}{more}");
    }
    // One session holds WeChat's window for every group: the Mac is waited
    // for to be left alone once, before the first, and the front is given
    // back once, after the last.
    let mut session: Option<Session> = None;
    let mut idle = Duration::ZERO;
    let mut spent: BTreeMap<String, Spent> = BTreeMap::new();
    let mut identities: BTreeMap<String, Value> = BTreeMap::new();
    let mut tried = 0;
    let ran = wxread_leave::run(&targets, mode, |target: &Target| {
        // Each one tried is the line that does it and those after it: what
        // a screen that stops the path says to run again.
        let retry = wxread_leave::line(&to_go[tried..], account_flag, mode);
        tried += 1;
        let session = match &mut session {
            Some(session) => session,
            None => {
                let since = Instant::now();
                let started = Session::start();
                idle = since.elapsed();
                session.insert(started.map_err(|e| anyhow!("{e:#}"))?)
            }
        };
        let mut path = Path::new(target.clone(), &retry, mode == Mode::Rehearse);
        let mut took = Spent::default();
        let since = Instant::now();
        let done = session.go(&mut path, &mut took).and_then(|clicked| match clicked {
            None => Ok(Outcome::Identified),
            Some(clicked) => {
                let dbs = holding.get(&target.id).map(Vec::as_slice).unwrap_or_default();
                let confirmed = confirm(&account, dbs, target, &own, clicked);
                took.confirm = clicked.elapsed();
                confirmed.map(Outcome::Left)
            }
        });
        took.total = since.elapsed();
        spent.insert(target.id.clone(), took);
        if let Some(identity) = path.identity() {
            identities.insert(target.id.clone(), identity_json(identity));
        }
        done.map_err(|e| anyhow!("{} ({}): {e:#}", target.id, target.display_name))
    });
    drop(session);
    let outcomes: Vec<Outcome> = ran.iter().map(|(o, _)| *o).collect();
    let mut stop = None;
    for (((group, target), (outcome, failed)), id) in
        groups.iter_mut().zip(&targets).zip(&ran).zip(&ids)
    {
        let rehearse = mode == Mode::Rehearse;
        group["plan"] =
            json!(target.as_ref().map(|t| wxread_leave::plan(t, rehearse)).unwrap_or_default());
        group["outcome"] = json!(outcome.name());
        group["confirmed_in"] = json!(outcome.confirmed_in());
        group["identity"] = identities.remove(*id).unwrap_or(Value::Null);
        group["seconds"] = spent.get(*id).map(Spent::json).unwrap_or(Value::Null);
        group["error"] = json!(failed.as_ref().map(|e| format!("{e:#}")));
        if *outcome == Outcome::Stopped {
            stop = failed.as_ref().map(|e| format!("stopped at {e:#}"));
        }
    }
    let mut answer = json!({
        "account": account.id(),
        "groups": groups,
        "blockers": blockers.iter().map(|Refusal(line)| line).collect::<Vec<_>>(),
        "next": wxread_leave::next(&ids, &outcomes, account_flag, !blockers.is_empty()),
        "seconds": {
            "read": seconds(read_in),
            "idle": seconds(idle),
            "total": seconds(began.elapsed()),
        },
    });
    if let Some(e) = stop {
        answer["error"] = json!(e);
        return Err(Failed(answer).into());
    }
    if outcomes.contains(&Outcome::NotIdentified) {
        answer["error"] = json!(
            "not every group's chat was told to be the group: each one not_identified has an error saying what was read and what did not match"
        );
        return Err(Failed(answer).into());
    }
    Ok(answer)
}

/// The groups `given` names, each once, in order: [`STDIN`] stands for
/// the ids read from stdin, split by whitespace.
fn ids(given: &[String]) -> anyhow::Result<Vec<String>> {
    let mut ids: Vec<String> = Vec::new();
    for id in given {
        let read = if id == STDIN {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).map_err(|e| {
                anyhow!("the ids on stdin could not be read ({e}): pass each group's id on the command line instead")
            })?;
            let read: Vec<String> = text.split_whitespace().map(str::to_string).collect();
            if read.is_empty() {
                bail!(
                    "no group's id was given on stdin: pipe in each one's id (<n>@chatroom), one a line, or pass them on the command line instead of {STDIN}"
                );
            }
            read
        } else {
            vec![id.clone()]
        };
        for id in read {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

/// The group `id`, read out of `db` (whose contacts are `contacts`, the
/// account's own id `own`), its newest message as `store` reads it (null
/// for none), and it as a leave's target, with its newest messages; None
/// when the account is not among its members.
fn target(
    db: &Connection,
    contacts: &Contacts,
    own: &str,
    store: &Store<'_>,
    id: &str,
) -> anyhow::Result<(Group, Value, Option<Target>)> {
    let group = group_in(db, contacts, id, own)?;
    let limit = Limit::new(COMPARED).ok_or_else(|| anyhow!("no page holds {COMPARED} messages"))?;
    let range = Range::parse(None, None).map_err(|e| anyhow!(e))?;
    let newest = match store.page(id, range, Start::First(Order::NewestFirst), limit) {
        Ok(page) => page.messages,
        Err(e) if e.is::<NoMessages>() => Vec::new(),
        Err(e) => return Err(e),
    };
    let last = newest.first().cloned().unwrap_or(Value::Null);
    let members: Vec<String> = group.members.iter().map(|m| m.id.clone()).collect();
    if wxread_leave::left(&members, own) {
        return Ok((group, last, None));
    }
    let recent = newest.iter().map(said).collect();
    let target = wxread_leave::target(id, &group.display_name, &group.name, &members, own, recent)
        .map_err(|Refusal(line)| anyhow!(line))?;
    Ok((group, last, Some(target)))
}

/// A message as `wxread messages` answers it, as its chat shows it: its
/// sender's display name, the minute of the day it was sent at on this
/// Mac's clock, and the text it shows, if any: a text's, a notice's, a
/// system message's; a card's title, summary and source, one a line.
fn said(message: &Value) -> Said {
    let kind = message["kind"].as_str().unwrap_or_default();
    let body = &message[kind];
    let shown: Vec<&str> =
        ["text", "title", "summary", "source"].iter().filter_map(|k| body[*k].as_str()).collect();
    Said {
        sender: message["sender"]["display_name"].as_str().unwrap_or_default().to_string(),
        minute: message["at"].as_str().and_then(local_minute),
        text: shown.join("\n"),
    }
}

/// The minute of the day, on this Mac's clock, of the time `at`
/// (RFC 3339) names.
fn local_minute(at: &str) -> Option<u32> {
    let seconds = libc::time_t::try_from(wxread_answer::seconds(at)?).ok()?;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::zeroed();
    let done = unsafe { libc::localtime_r(&seconds, tm.as_mut_ptr()) };
    if done.is_null() {
        return None;
    }
    let tm = unsafe { tm.assume_init() };
    u32::try_from(tm.tm_hour * 60 + tm.tm_min).ok()
}

/// The group as the answer names it, with its newest message `last`.
fn group_json(group: &Group, last: &Value) -> Value {
    json!({
        "id": group.id,
        "display_name": group.display_name,
        "name": group.name,
        "members": group.members.len(),
        "last_message": last,
    })
}

/// What the chat opened showed of which group it is, as the answer names
/// it.
fn identity_json(identity: &wxread_leave::Identity) -> Value {
    json!({
        "members_shown": identity.members,
        "messages_found": identity.found,
        "messages_compared": identity.compared,
    })
}

/// The seconds a step took, to the millisecond.
fn seconds(took: Duration) -> f64 {
    (took.as_secs_f64() * 1000.0).round() / 1000.0
}

/// How long each step of one group took: the search, telling its chat is
/// the group, leaving it (its chat info and the confirmation), and the
/// databases confirming it.
#[derive(Default)]
struct Spent {
    search: Duration,
    identify: Duration,
    leave: Duration,
    confirm: Duration,
    total: Duration,
}

impl Spent {
    fn add(&mut self, look: Look, took: Duration) {
        let step = match look.step() {
            "search" => &mut self.search,
            "identify" => &mut self.identify,
            _ => &mut self.leave,
        };
        *step += took;
    }

    fn json(&self) -> Value {
        json!({
            "search": seconds(self.search),
            "identify": seconds(self.identify),
            "leave": seconds(self.leave),
            "confirm": seconds(self.confirm),
            "total": seconds(self.total),
        })
    }
}

/// WeChat's window, held for a run: in front from its first group to its
/// last, and the app that was in front before given it back when dropped.
struct Session {
    window: window::Window,
    before: Option<i32>,
}

impl Session {
    /// The session of WeChat's window, once the Mac is left alone.
    fn start() -> anyhow::Result<Session> {
        let pid = window::wechat()
            .ok_or_else(|| anyhow!("WeChat is not running: open it, then run it again"))?;
        wait_for_idle()?;
        let window = window::Window::of(pid)?;
        let before = window::front_app().filter(|app| *app != pid);
        Ok(Session { window, before })
    }

    /// Drives the window along `path`, counting each step's time in
    /// `spent`: answers when OK was clicked on the confirmation, or None
    /// when a rehearsal told the chat is the group.
    fn go(&mut self, path: &mut Path, spent: &mut Spent) -> anyhow::Result<Option<Instant>> {
        let window = &mut self.window;
        let mut next = path.first();
        loop {
            next = match next {
                Move::Then(acts, look) => {
                    let since = Instant::now();
                    let seen = acts
                        .iter()
                        .try_for_each(|act| window.act(act))
                        .and_then(|()| watch(window, path, look));
                    spent.add(look, since.elapsed());
                    seen?
                }
                Move::Again => path.waited(),
                Move::Identified => return Ok(None),
                Move::Confirm(acts) => {
                    acts.iter().try_for_each(|act| window.act(act))?;
                    return Ok(Some(Instant::now()));
                }
                Move::Stop(acts, Refusal(line)) => {
                    for act in &acts {
                        let _ = window.act(act);
                    }
                    bail!(line);
                }
            };
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.window.restore();
        if let Some(app) = self.before {
            window::give_front(app);
        }
    }
}

/// Looks at `window` until `path` sees in it the screen `look` waits for,
/// or the look's patience has passed: the move then.
fn watch(window: &mut window::Window, path: &mut Path, look: Look) -> anyhow::Result<Move> {
    let since = Instant::now();
    let patience = Duration::from_millis(look.patience());
    loop {
        let next = path.seen(&window.look()?);
        if next != Move::Again {
            return Ok(next);
        }
        if since.elapsed() >= patience {
            return Ok(path.waited());
        }
    }
}

/// Reads, every [`POLL_MILLIS`] from `clicked` (OK's click), the newest
/// message of `target`'s chat out of the message databases `dbs` hold it
/// in, and its members out of the contact database, until the chat holds
/// the account `own`'s notice of leaving it or the account is no longer
/// among them: the seconds that took. Not within [`LEFT_WAIT_SECONDS`],
/// the leave is not confirmed.
fn confirm(
    account: &Account,
    dbs: &[String],
    target: &Target,
    own: &str,
    clicked: Instant,
) -> anyhow::Result<u64> {
    let ok_at = SystemTime::now()
        .checked_sub(clicked.elapsed())
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX));
    loop {
        for db in dbs {
            let newest = messages::newest(account, db, &target.id)?.map(|row| Newest {
                system: wxread_messages::kinds::split(row.local_type).0
                    == wxread_messages::kinds::SYSTEM,
                server_id: row.server_id,
                sender: row.sender,
                at: row.create_time,
            });
            if newest.is_some_and(|n| wxread_leave::left_by_notice(&n, own, ok_at)) {
                return Ok(clicked.elapsed().as_secs());
            }
        }
        let now = members(&account.database(CONTACT_DB)?, &target.id)?;
        let seconds = clicked.elapsed().as_secs();
        if wxread_leave::left(&now, own) {
            return Ok(seconds);
        }
        if seconds >= LEFT_WAIT_SECONDS {
            let Refusal(line) = wxread_leave::not_confirmed(target, seconds);
            bail!("{line}");
        }
        sleep(Duration::from_millis(POLL_MILLIS));
    }
}

/// Waits until the person at the Mac has left the keyboard and mouse alone
/// for [`IDLE_SECONDS`], for at most [`IDLE_WAIT_SECONDS`], looking every
/// tenth of a second.
fn wait_for_idle() -> anyhow::Result<()> {
    let since = Instant::now();
    while window::idle() < IDLE_SECONDS as f64 {
        if since.elapsed().as_secs() >= IDLE_WAIT_SECONDS {
            bail!(
                "the Mac was in use for {IDLE_WAIT_SECONDS} s, so nothing was touched: run it again when the keyboard and mouse can be left alone for a few seconds"
            );
        }
        sleep(Duration::from_millis(100));
    }
    Ok(())
}
