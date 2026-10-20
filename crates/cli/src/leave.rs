//! `wxread group leave`: reads each group asked for and what macOS and
//! WeChat allow, and without `--yes` answers what leaving them would do;
//! with `--yes` leaves them one after another, as [`wxread_leave::run`]
//! orders, each by driving WeChat's window along the path
//! [`wxread_leave::Path`] checks, and answers one left only once the
//! databases no longer list the account among its members
//! (docs/adr/0015-a-group-is-left-by-driving-wechats-window-only-with-yes.md).

use std::io::Read as _;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use serde_json::{Value, json};
use wxread_contacts::{Contacts, Kind};
use wxread_leave::{
    IDLE_SECONDS, IDLE_WAIT_SECONDS, LEFT_WAIT_SECONDS, Move, Path, Refusal, Target, WeChat,
};
use wxread_schema::CONTACT_DB;

use crate::account::Account;
use crate::contacts::{contacts, own_id, read_group};
use crate::world::World;
use crate::{Failed, messages, window};

/// The id that stands for the ids read from stdin.
pub const STDIN: &str = "-";

/// What `group leave` was asked: the groups, `--account` and `--yes`.
pub struct Asked {
    pub ids: Vec<String>,
    pub account: Option<String>,
    pub yes: bool,
}

/// What `group leave` answers of `world`: the plan, or with `--yes` the
/// groups left; a run that stops partway fails with the same answer, its
/// error added.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let ids = ids(&asked.ids)?;
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB])?;
    let contacts = contacts(&account, false)?;
    let own = own_id(&account, &contacts);
    let mut read = Vec::new();
    let mut faults = Vec::new();
    for id in &ids {
        match target(&account, &contacts, &own, id) {
            Ok(target) => read.push(target),
            Err(e) => faults.push(format!("{e:#}")),
        }
    }
    if !faults.is_empty() {
        bail!("{}", faults.join("; and "));
    }
    let mut groups = Vec::new();
    for (group, _) in &read {
        groups.push(group_json(group, &last_message(world, asked, &group.id)?));
    }
    let targets: Vec<Option<Target>> = read.into_iter().map(|(_, target)| target).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let account_flag = asked.account.as_deref();
    let to_leave: Vec<&str> = targets.iter().flatten().map(|t| t.id.as_str()).collect();
    let blockers = if to_leave.is_empty() {
        Vec::new()
    } else {
        let (dir, _) = world.data_dir();
        let wechat = match window::wechat() {
            None => WeChat::NotRunning,
            Some(pid) => WeChat::LoggedIn(window::accounts_open(pid, &PathBuf::from(dir))),
        };
        let retry = wxread_leave::line(&to_leave, account_flag);
        wxread_leave::blockers(&window::missing(), &wechat, account.id(), &retry)
    };
    if asked.yes
        && let Some(Refusal(first)) = blockers.first()
    {
        let rest: Vec<&str> = blockers.iter().skip(1).map(|Refusal(l)| l.as_str()).collect();
        let more =
            if rest.is_empty() { String::new() } else { format!("; and {}", rest.join("; and ")) };
        bail!("nothing was touched: {first}{more}");
    }
    // Each one left is the line that leaves it and those after it: what a
    // screen that stops the path says to run again.
    let mut tried = 0;
    let (outcomes, stop) = wxread_leave::run(&targets, asked.yes, |target: &Target| {
        let retry = wxread_leave::line(&to_leave[tried..], account_flag);
        tried += 1;
        leave(world, asked, target, &own, &retry)
            .map_err(|e| anyhow!("stopped at {} ({}): {e:#}", target.id, target.display_name))
    });
    for ((group, target), outcome) in groups.iter_mut().zip(&targets).zip(&outcomes) {
        group["plan"] = json!(target.as_ref().map(wxread_leave::plan).unwrap_or_default());
        group["outcome"] = json!(outcome.name());
        group["confirmed_in"] = json!(outcome.confirmed_in());
    }
    let mut answer = json!({
        "account": account.id(),
        "groups": groups,
        "blockers": blockers.iter().map(|Refusal(line)| line).collect::<Vec<_>>(),
        "next": wxread_leave::next(&ids, &outcomes, account_flag, !blockers.is_empty()),
    });
    match stop {
        None => Ok(answer),
        Some(e) => {
            answer["error"] = json!(format!("{e:#}"));
            Err(Failed(answer).into())
        }
    }
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

/// Leaves `target` through WeChat's window, once the Mac is left alone,
/// and answers the seconds after OK the databases took to no longer list
/// the account `own` among its members; `retry` is what a stop says to run
/// again.
fn leave(
    world: &World,
    asked: &Asked,
    target: &Target,
    own: &str,
    retry: &str,
) -> anyhow::Result<u64> {
    let pid = window::wechat()
        .ok_or_else(|| anyhow!("WeChat is not running: open it, then run `{retry}`"))?;
    wait_for_idle()?;
    let clicked = drive(pid, target, retry)?;
    loop {
        let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB])?;
        let now = read_group(&account, &contacts(&account, false)?, &target.id)?;
        let members: Vec<String> = now.members.into_iter().map(|m| m.id).collect();
        let seconds = clicked.elapsed().as_secs();
        if wxread_leave::left(&members, own) {
            return Ok(seconds);
        }
        if seconds >= LEFT_WAIT_SECONDS {
            let Refusal(line) = wxread_leave::not_confirmed(target, seconds);
            bail!("{line}");
        }
        sleep(Duration::from_secs(1));
    }
}

/// The group as the answer names it, with its newest message `last`.
fn group_json(group: &wxread_contacts::Group, last: &Value) -> Value {
    json!({
        "id": group.id,
        "display_name": group.display_name,
        "name": group.name,
        "members": group.members.len(),
        "last_message": last,
    })
}

/// The group `id` of `account`, whose contacts are `contacts` and whose
/// own id is `own`, and it as a leave's target; None when the account is
/// not among its members. Refused when another of the account's groups is
/// shown as it is.
fn target(
    account: &Account,
    contacts: &Contacts,
    own: &str,
    id: &str,
) -> anyhow::Result<(wxread_contacts::Group, Option<Target>)> {
    let group = read_group(account, contacts, id)?;
    let members: Vec<String> = group.members.iter().map(|m| m.id.clone()).collect();
    if wxread_leave::left(&members, own) {
        return Ok((group, None));
    }
    // Every group shown as this one is, as `wxread contacts` answers it (a
    // group with no name of its own by its members'), that the account is
    // still in: one it has left reads apart in the window, its title
    // holding no member count.
    let shown: Vec<String> = contacts
        .values()
        .cloned()
        .map(wxread_contacts::contact)
        .filter(|c| c.kind == Kind::Group && c.display_name == group.display_name)
        .map(|c| c.id)
        .collect();
    let mut same = Vec::new();
    for other in shown {
        let members = read_group(account, contacts, &other)?.members;
        if members.iter().any(|m| m.id == own) {
            same.push(other);
        }
    }
    let target = wxread_leave::target(id, &group.display_name, &group.name, &members, own, &same)
        .map_err(|Refusal(line)| anyhow!(line))?;
    Ok((group, Some(target)))
}

/// The group's newest message, as `wxread messages` answers it; null when
/// WeChat on this Mac holds none of its messages.
fn last_message(world: &World, asked: &Asked, id: &str) -> anyhow::Result<Value> {
    let read = messages::Asked {
        account: asked.account.clone(),
        chat: Some(id.to_string()),
        from: None,
        to: None,
        limit: Some("1".to_string()),
        cursor: None,
        oldest_first: false,
    };
    match messages::run(world, &read) {
        Ok(mut page) => Ok(page["messages"].get_mut(0).map(Value::take).unwrap_or(Value::Null)),
        Err(e) if e.is::<messages::NoMessages>() => Ok(Value::Null),
        Err(e) => Err(e),
    }
}

/// Waits until the person at the Mac has left the keyboard and mouse alone
/// for [`IDLE_SECONDS`], for at most [`IDLE_WAIT_SECONDS`].
fn wait_for_idle() -> anyhow::Result<()> {
    let since = Instant::now();
    while window::idle() < IDLE_SECONDS as f64 {
        if since.elapsed().as_secs() >= IDLE_WAIT_SECONDS {
            bail!(
                "the Mac was in use for {IDLE_WAIT_SECONDS} s, so nothing was touched: run it again when the keyboard and mouse can be left alone for a few seconds"
            );
        }
        sleep(Duration::from_millis(500));
    }
    Ok(())
}

/// Drives WeChat's window (process `pid`) along the path to leave
/// `target`, up to OK on the confirmation, and says when OK was clicked.
/// The app that was in front gets it back, however it ended.
fn drive(pid: i32, target: &Target, retry: &str) -> anyhow::Result<Instant> {
    let mut window = window::Window::of(pid)?;
    let before = window::front_app().filter(|app| *app != pid);
    let mut path = Path::new(target.clone(), retry);
    let mut next = path.first();
    let ended = loop {
        match next {
            Move::Then(acts, _) => {
                if let Err(e) = acts.iter().try_for_each(|act| window.act(act)) {
                    break Err(e);
                }
                match window.look() {
                    Ok(screen) => next = path.seen(&screen),
                    Err(e) => break Err(e),
                }
            }
            Move::Confirm(acts) => {
                let clicked = acts.iter().try_for_each(|act| window.act(act));
                break clicked.map(|()| Instant::now());
            }
            Move::Stop(acts, Refusal(line)) => {
                for act in &acts {
                    let _ = window.act(act);
                }
                break Err(anyhow!(line));
            }
        }
    };
    if let Some(app) = before {
        window::give_front(app);
    }
    ended
}
