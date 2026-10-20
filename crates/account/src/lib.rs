//! What a command that reads an account decides before it reads: which
//! account, out of those the data directory holds, and why one of its
//! databases cannot be opened, each refusal naming the database and its
//! fix. The effect crate reads the files; nothing here does.

use wxread_doctor::{AccountSeen, DATA_DIR_FLAG, KEYS_FLAG, KEYS_VAR, KeysSeen, Listing, Source};
use wxread_keys::{Key, Keys};
use wxread_sqlcipher::Refusal;

/// The option that picks an account.
pub const ACCOUNT_FLAG: &str = "--account";

/// What fixes keys that are missing, or no longer open their databases.
pub const GET_KEYS: &str = "get the keys with `sudo wxread keys` (`wxread keys --help` says how), then supply them to every command: --keys <file>, or their contents as WXREAD_KEYS";

/// The account `wanted` names, or with none named the one account there
/// is, out of what `listing` found in the data directory `dir` (named from
/// `from`); or why there is none to read, and its fix.
pub fn choose<'a>(
    dir: &str,
    from: Source,
    listing: &'a Listing,
    wanted: Option<&str>,
) -> Result<&'a AccountSeen, String> {
    if let Some(finding) = wxread_doctor::data_dir_finding(dir, from, listing) {
        return Err(format!("{}: {}", finding.problem, finding.fix));
    }
    let accounts = match listing {
        Listing::Accounts(accounts) => accounts.as_slice(),
        _ => &[],
    };
    let ids = accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join(", ");
    match (wanted, accounts) {
        (Some(id), _) => accounts.iter().find(|a| a.id == id).ok_or_else(|| {
            format!(
                "no account {id} is under {dir}, whose accounts are {ids}: pass {ACCOUNT_FLAG} one of them (`wxread doctor` lists them), or {DATA_DIR_FLAG} the data directory that holds {id}"
            )
        }),
        (None, [one]) => Ok(one),
        (None, _) => Err(format!(
            "{} accounts are logged in on this Mac ({ids}): pass {ACCOUNT_FLAG} <id>, one of them",
            accounts.len()
        )),
    }
}

/// The keys as supplied, or why they cannot be had: none supplied, the
/// file unreadable, or every fault in them; `needed` the databases the
/// command reads, named in the refusal.
pub fn keys(seen: &KeysSeen, needed: &[&str]) -> Result<Keys, String> {
    match seen {
        KeysSeen::None => Err(format!(
            "no keys were supplied, and {} need them: neither {KEYS_FLAG} nor {KEYS_VAR} is set; {GET_KEYS}",
            needed.join(" and ")
        )),
        KeysSeen::Unreadable { file, why } => Err(format!(
            "the keys file {file} cannot be read ({why}): pass {KEYS_FLAG} the file the keys are in; {GET_KEYS}"
        )),
        KeysSeen::Text { from, text } => Keys::parse(text).map_err(|faults| {
            let place = match from {
                Source::Var => KEYS_VAR,
                Source::Flag | Source::Default => "the keys file",
            };
            format!(
                "in {place}, {}: supply the keys exactly as `wxread keys` answers them; `wxread keys --help` says their shape",
                faults.join("; ")
            )
        }),
    }
}

/// The key of database `path` of `account`, or the refusal that names it.
pub fn key_of<'a>(keys: &'a Keys, account: &str, path: &str) -> Result<&'a Key, String> {
    keys.get(path).ok_or_else(|| {
        format!(
            "the keys supplied have no key of {path}, which account {account} reads it from: they are another account's, or older than its databases; {GET_KEYS}"
        )
    })
}

/// Why database `path` of `account` was not opened.
pub fn refused(account: &str, path: &str, refusal: Refusal) -> String {
    match refusal {
        Refusal::KeyDoesNotOpen => format!(
            "the key of {path} does not open it: WeChat created account {account}'s databases again (logging out and in, some updates) after the keys were read; {GET_KEYS}"
        ),
        Refusal::Short => format!(
            "{path} of account {account} is shorter than one page, so not a database WeChat for Mac 4.x wrote: run `wxread doctor`, and let WeChat finish creating it"
        ),
    }
}

/// Why database `path` of `account` could not be read, as the system said.
pub fn unreadable(account: &str, path: &str, why: &str) -> String {
    format!(
        "account {account}'s database {path} cannot be read ({why}): run `wxread doctor`, which names the fix of a missing database or of macOS refusing to let it be read"
    )
}

/// Why database `path`, decrypted, is not the database wxread reads.
pub fn not_wechats(path: &str, why: &str) -> String {
    format!(
        "{path} decrypted is not the database WeChat for Mac 4.x keeps ({why}): wxread reads WeChat 4.x's tables, and this WeChat wrote others; tell wxread's maintainers which WeChat version wrote it (https://github.com/sunfmin/wxread/issues)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(id: &str) -> AccountSeen {
        AccountSeen {
            id: id.into(),
            db_storage: format!("/x/{id}/db_storage"),
            databases: Ok(vec![]),
        }
    }

    #[test]
    fn one_account_is_used_and_several_need_naming() {
        let one = Listing::Accounts(vec![seen("a_1")]);
        assert_eq!(choose("/x", Source::Default, &one, None).unwrap().id, "a_1");
        let two = Listing::Accounts(vec![seen("a_1"), seen("b_2")]);
        let why = choose("/x", Source::Default, &two, None).unwrap_err();
        assert_eq!(
            why,
            "2 accounts are logged in on this Mac (a_1, b_2): pass --account <id>, one of them"
        );
        assert_eq!(choose("/x", Source::Default, &two, Some("b_2")).unwrap().id, "b_2");
        let why = choose("/x", Source::Default, &two, Some("c_3")).unwrap_err();
        assert!(
            why.starts_with("no account c_3 is under /x, whose accounts are a_1, b_2"),
            "{why}"
        );
    }

    #[test]
    fn no_account_is_doctors_finding() {
        for listing in [Listing::Missing, Listing::Accounts(vec![])] {
            let why = choose("/x", Source::Default, &listing, None).unwrap_err();
            assert!(why.contains("log in"), "{why}");
        }
    }

    #[test]
    fn missing_and_bad_keys_name_the_databases_and_the_fix() {
        let why = keys(&KeysSeen::None, &["a.db", "b.db"]).unwrap_err();
        assert!(why.contains("a.db and b.db") && why.contains("sudo wxread keys"), "{why}");
        let text = KeysSeen::Text { from: Source::Var, text: r#"{"a.db": {}, "b.db": 1}"#.into() };
        let why = keys(&text, &[]).unwrap_err();
        assert!(
            why.starts_with("in WXREAD_KEYS, the key of a.db has no enc_key; the key of b.db"),
            "{why}"
        );
        let none = Keys::parse("{}").unwrap();
        assert!(key_of(&none, "a_1", "a.db").unwrap_err().contains("no key of a.db"));
    }
}
