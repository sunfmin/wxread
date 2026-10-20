//! `wxread doctor`: reads WeChat's data directory, each account's
//! `db_storage` and the keys supplied, and prints what the doctor crate
//! finds in them. It writes nothing.

use serde_json::{Value, json};
use wxread_doctor::{KEYS_FLAG, KEYS_VAR, Permission, Report, Seen, Source};

use crate::world::World;

/// What doctor answers of `world`.
pub fn run(world: &World) -> Value {
    let seen = world.seen();
    let mut answer = to_json(&seen, &wxread_doctor::diagnose(&seen));
    answer["permissions"] = permissions();
    answer
}

/// Whether macOS gives the app running wxread each permission
/// `group leave` needs; reading needs neither, so none is a finding.
fn permissions() -> Value {
    let mut all = serde_json::Map::new();
    for p in Permission::ALL {
        let granted = granted(p);
        all.insert(
            p.name().to_string(),
            json!({"granted": granted, "needed_for": p.needed_for(), "fix": (!granted).then(|| p.fix())}),
        );
    }
    Value::Object(all)
}

#[cfg(target_os = "macos")]
fn granted(p: Permission) -> bool {
    crate::window::granted(p)
}

#[cfg(not(target_os = "macos"))]
fn granted(_: Permission) -> bool {
    false
}

fn to_json(seen: &Seen, report: &Report) -> Value {
    let accounts: Vec<Value> = report
        .accounts
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "db_storage": a.db_storage,
                "databases": a.tally.databases(),
                "keyed": a.tally.keyed(),
                "unkeyed": a.unkeyed,
            })
        })
        .collect();
    let findings: Vec<Value> =
        report.findings.iter().map(|f| json!({"problem": f.problem, "fix": f.fix})).collect();
    json!({
        "ok": report.ok(),
        "data_dir": {"path": seen.data_dir, "from": seen.data_dir_from.name()},
        "accounts": accounts,
        "keys": {
            "supplied": report.keys.from.is_some(),
            "from": report.keys.from.map(Source::name),
            "looked": [format!("{KEYS_FLAG} <file>"), KEYS_VAR],
            "databases": report.keys.databases,
        },
        "findings": findings,
    })
}
