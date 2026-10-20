//! `wxread doctor`: reads WeChat's data directory, each account's
//! `db_storage` and the keys supplied, and prints what the doctor crate
//! finds in them. It writes nothing.

use serde_json::{Value, json};
use wxread_doctor::{KEYS_FLAG, KEYS_VAR, Report, Seen, Source};

use crate::world::World;

/// What doctor answers of `world`.
pub fn run(world: &World) -> Value {
    let seen = world.seen();
    to_json(&seen, &wxread_doctor::diagnose(&seen))
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
