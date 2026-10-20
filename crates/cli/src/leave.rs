//! `wxread group leave`: reads the group and what macOS and WeChat allow,
//! and without `--yes` answers what leaving it would do; with `--yes`
//! drives WeChat's window along the path [`wxread_leave::Path`] checks,
//! and answers only once the databases no longer list the account among
//! the group's members
//! (docs/adr/0011-a-group-is-left-by-driving-wechats-window-only-with-yes.md).

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use serde_json::{Value, json};
use wxread_contacts::Kind;
use wxread_leave::{
    IDLE_SECONDS, IDLE_WAIT_SECONDS, LEFT_WAIT_SECONDS, Move, Path, Refusal, Target, WeChat,
};
use wxread_schema::CONTACT_DB;

use crate::account::Account;
use crate::contacts::{contacts, own_id, read_group};
use crate::world::World;
use crate::{messages, window};

/// What `group leave` was asked: the group, `--account` and `--yes`.
pub struct Asked {
    pub id: String,
    pub account: Option<String>,
    pub yes: bool,
}

/// What `group leave` answers of `world`: the plan, or with `--yes` the
/// group left.
pub fn run(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB])?;
    let target = target(&account, &asked.id)?;
    let retry = wxread_leave::line(&target.id, asked.account.as_deref());
    let last = last_message(world, asked)?;
    let (dir, _) = world.data_dir();
    let pid = window::wechat();
    let wechat = match pid {
        None => WeChat::NotRunning,
        Some(pid) => WeChat::LoggedIn(window::accounts_open(pid, &PathBuf::from(dir))),
    };
    let blockers = wxread_leave::blockers(&window::missing(), &wechat, account.id(), &retry);
    let group = group_json(&target, &last);
    let plan = wxread_leave::plan(&target);
    if !asked.yes {
        let next = if blockers.is_empty() {
            format!("nothing was changed: run `{retry}` to leave it; it cannot be undone")
        } else {
            "nothing was changed: fix each of blockers, then run it again with --yes".to_string()
        };
        return Ok(json!({
            "account": account.id(),
            "group": group,
            "applied": false,
            "confirmed_in": null,
            "plan": plan,
            "blockers": blockers.iter().map(|Refusal(line)| line).collect::<Vec<_>>(),
            "next": next,
        }));
    }
    if let Some(Refusal(first)) = blockers.first() {
        let rest: Vec<&str> = blockers.iter().skip(1).map(|Refusal(l)| l.as_str()).collect();
        let more =
            if rest.is_empty() { String::new() } else { format!("; and {}", rest.join("; and ")) };
        bail!("nothing was touched: {first}{more}");
    }
    let pid =
        pid.ok_or_else(|| anyhow!("WeChat is not running: open it, then run `{retry}` again"))?;
    wait_for_idle()?;
    let clicked = drive(pid, &target, &retry)?;
    let own = own_id(&account, &contacts(&account, false)?);
    loop {
        let account = Account::open(world, asked.account.as_deref(), &[CONTACT_DB])?;
        let now = read_group(&account, &contacts(&account, false)?, &target.id)?;
        let members: Vec<String> = now.members.into_iter().map(|m| m.id).collect();
        let seconds = clicked.elapsed().as_secs();
        if wxread_leave::left(&members, &own) {
            return Ok(json!({
                "account": account.id(),
                "group": group,
                "applied": true,
                "confirmed_in": seconds,
                "plan": plan,
                "blockers": [],
                "next": format!("`wxread group {}` answers its members, the account no longer among them; to rejoin, a member must invite the account back", target.id),
            }));
        }
        if seconds >= LEFT_WAIT_SECONDS {
            let Refusal(line) = wxread_leave::not_confirmed(&target, seconds);
            bail!("{line}");
        }
        sleep(Duration::from_secs(1));
    }
}

/// The group as the answer names it, with its newest message `last`.
fn group_json(target: &Target, last: &Value) -> Value {
    json!({
        "id": target.id,
        "display_name": target.display_name,
        "name": target.name,
        "members": target.members,
        "last_message": last,
    })
}

/// The group `id` of `account` as a leave's target: refused when the
/// account is not in it, or another of its groups is shown as it is.
fn target(account: &Account, id: &str) -> anyhow::Result<Target> {
    let contacts = contacts(account, false)?;
    let group = read_group(account, &contacts, id)?;
    let own = own_id(account, &contacts);
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
        let members = read_group(account, &contacts, &other)?.members;
        if members.iter().any(|m| m.id == own) {
            same.push(other);
        }
    }
    let members: Vec<String> = group.members.iter().map(|m| m.id.clone()).collect();
    wxread_leave::target(id, &group.display_name, &group.name, &members, &own, &same)
        .map_err(|Refusal(line)| anyhow!(line))
}

/// The group's newest message, as `wxread messages` answers it; null when
/// WeChat on this Mac holds none of its messages.
fn last_message(world: &World, asked: &Asked) -> anyhow::Result<Value> {
    let read = messages::Asked {
        account: asked.account.clone(),
        chat: Some(asked.id.clone()),
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
