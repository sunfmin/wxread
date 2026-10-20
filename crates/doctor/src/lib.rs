//! What `wxread doctor` finds in what it read: WeChat's data directory,
//! each account under it with its databases, and the keys supplied; every
//! finding with the line that fixes it. The effect crate reads the world
//! into a [`Seen`] and prints the [`Report`]; nothing here reads or writes.

use flux_rs::attrs::*;
use wxread_keys::Keys;

/// The option that names the keys file, and the variable that holds the
/// keys themselves.
pub const KEYS_FLAG: &str = "--keys";
pub const KEYS_VAR: &str = "WXREAD_KEYS";
/// The option and variable that name WeChat's data directory.
pub const DATA_DIR_FLAG: &str = "--data-dir";
pub const DATA_DIR_VAR: &str = "WXREAD_DATA_DIR";
/// Where WeChat for Mac 4.x keeps its accounts, under the home directory.
pub const DATA_DIR_UNDER_HOME: &str =
    "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files";
/// The directory of an account that holds its databases.
pub const DB_STORAGE: &str = "db_storage";

/// Where a setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Flag,
    Var,
    Default,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Flag => "flag",
            Source::Var => "variable",
            Source::Default => "default",
        }
    }
}

/// What the effect crate read, before anything is judged of it.
#[derive(Debug, Clone)]
pub struct Seen {
    /// WeChat's data directory (`xwechat_files`), and where its path came from.
    pub data_dir: String,
    pub data_dir_from: Source,
    pub listing: Listing,
    pub keys: KeysSeen,
}

/// What reading the data directory found.
#[derive(Debug, Clone)]
pub enum Listing {
    Missing,
    NotADirectory,
    /// It could not be read: macOS refused, or another failure, as said.
    Unreadable(String),
    /// Each directory under it that holds a `db_storage`.
    Accounts(Vec<AccountSeen>),
}

/// One account's directory, and the databases under its `db_storage`.
#[derive(Debug, Clone)]
pub struct AccountSeen {
    /// The account directory's name (`<wxid>_<n>`).
    pub id: String,
    pub db_storage: String,
    /// Each `.db` file's path under `db_storage`, `/` between its parts; or
    /// why they could not be listed.
    pub databases: Result<Vec<String>, String>,
}

/// The keys as supplied, before they are parsed.
#[derive(Debug, Clone)]
pub enum KeysSeen {
    /// Neither `--keys` nor `WXREAD_KEYS` was given.
    None,
    /// `--keys <file>` named a file that could not be read, and why.
    Unreadable { file: String, why: String },
    /// The text of the keys, from the flag's file or the variable.
    Text { from: Source, text: String },
}

/// One thing wrong, and the line that fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub problem: String,
    pub fix: String,
}

/// What doctor answers.
#[derive(Debug, Clone)]
pub struct Report {
    pub accounts: Vec<AccountReport>,
    pub keys: KeysReport,
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether nothing is wrong.
    pub fn ok(&self) -> bool {
        self.findings.is_empty()
    }
}

/// An account, with how many of its databases the keys open.
#[derive(Debug, Clone)]
pub struct AccountReport {
    pub id: String,
    pub db_storage: String,
    pub tally: Tally,
    /// The databases there is no key of.
    pub unkeyed: Vec<String>,
}

/// What doctor says of the keys.
#[derive(Debug, Clone)]
pub struct KeysReport {
    /// Where they came from, when they were supplied at all.
    pub from: Option<Source>,
    /// How many databases they open; zero when they could not be read.
    pub databases: usize,
}

/// An account's databases, and how many of them have a key: never more
/// than there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[refined_by(databases: int, keyed: int)]
#[invariant(keyed <= databases)]
pub struct Tally {
    #[field(usize[databases])]
    databases: usize,
    #[field(usize[keyed])]
    keyed: usize,
}

impl Tally {
    #[spec(fn() -> Tally[0, 0])]
    pub fn new() -> Tally {
        Tally { databases: 0, keyed: 0 }
    }

    /// The tally with one more database, keyed or not. A count already at
    /// the largest there is stays there: no account has that many files.
    pub fn count(self, keyed: bool) -> Tally {
        if self.databases == usize::MAX {
            return self;
        }
        let keyed = if keyed { self.keyed + 1 } else { self.keyed };
        Tally { databases: self.databases + 1, keyed }
    }

    #[spec(fn(&Tally[@d, @k]) -> usize[d])]
    pub fn databases(&self) -> usize {
        self.databases
    }

    #[spec(fn(&Tally[@d, @k]) -> usize[k])]
    pub fn keyed(&self) -> usize {
        self.keyed
    }
}

impl Default for Tally {
    fn default() -> Tally {
        Tally::new()
    }
}

/// What fixes keys that are missing.
pub const GET_KEYS: &str = "`wxread keys --help` says how to get the keys; then pass the file with `--keys <file>`, or its contents as WXREAD_KEYS";
/// What fixes reading another app's data, which macOS refused.
pub const ALLOW_READING: &str = "allow the app running wxread (your terminal, or the agent's host) to read other apps' data: System Settings › Privacy & Security › Full Disk Access, then run it again";

/// Everything wrong with what was `seen`, each finding with its fix.
pub fn diagnose(seen: &Seen) -> Report {
    let mut findings = Vec::new();
    let (keys, keys_report) = judge_keys(&seen.keys, &mut findings);
    let accounts = match &seen.listing {
        Listing::Accounts(accounts) => accounts.as_slice(),
        _ => &[],
    };
    findings.extend(data_dir_finding(&seen.data_dir, seen.data_dir_from, &seen.listing));
    let mut reports = Vec::new();
    for account in accounts {
        reports.push(judge_account(account, keys.as_ref(), &mut findings));
    }
    Report { accounts: reports, keys: keys_report, findings }
}

/// What is wrong with the data directory `dir`, named from `from`, as
/// `listing` found it: nothing, when it lists an account.
pub fn data_dir_finding(dir: &str, from: Source, listing: &Listing) -> Option<Finding> {
    let elsewhere = match from {
        Source::Default => format!(
            "if WeChat keeps its files elsewhere, pass {DATA_DIR_FLAG} <its xwechat_files directory>"
        ),
        Source::Flag | Source::Var => format!(
            "pass {DATA_DIR_FLAG} (or set {DATA_DIR_VAR} to) the xwechat_files directory WeChat keeps its accounts in, by default ~/{DATA_DIR_UNDER_HOME}"
        ),
    };
    let finding = match listing {
        Listing::Missing => Finding {
            problem: format!("WeChat's data directory {dir} does not exist"),
            fix: format!("install WeChat for Mac 4.x and log in; {elsewhere}"),
        },
        Listing::NotADirectory => Finding {
            problem: format!("WeChat's data directory {dir} is not a directory"),
            fix: elsewhere,
        },
        Listing::Unreadable(why) => Finding {
            problem: format!("WeChat's data directory {dir} cannot be read ({why})"),
            fix: ALLOW_READING.to_string(),
        },
        Listing::Accounts(accounts) if accounts.is_empty() => Finding {
            problem: format!("no account is under {dir}: no directory there holds a {DB_STORAGE}"),
            fix: format!("log in to WeChat for Mac 4.x on this Mac; {elsewhere}"),
        },
        Listing::Accounts(_) => return None,
    };
    Some(finding)
}

/// The keys, when they parse, and what to report of them.
fn judge_keys(seen: &KeysSeen, findings: &mut Vec<Finding>) -> (Option<Keys>, KeysReport) {
    match seen {
        KeysSeen::None => {
            findings.push(Finding {
                problem: format!(
                    "no keys were supplied: neither {KEYS_FLAG} nor {KEYS_VAR} is set"
                ),
                fix: GET_KEYS.to_string(),
            });
            (None, KeysReport { from: None, databases: 0 })
        }
        KeysSeen::Unreadable { file, why } => {
            findings.push(Finding {
                problem: format!("the keys file {file} cannot be read ({why})"),
                fix: format!(
                    "pass {KEYS_FLAG} a file that holds the keys `wxread keys` answers; {GET_KEYS}"
                ),
            });
            (None, KeysReport { from: Some(Source::Flag), databases: 0 })
        }
        KeysSeen::Text { from, text } => {
            let place = match from {
                Source::Var => KEYS_VAR,
                Source::Flag | Source::Default => "the keys file",
            };
            match Keys::parse(text) {
                Ok(keys) => {
                    let databases = keys.len();
                    (Some(keys), KeysReport { from: Some(*from), databases })
                }
                Err(faults) => {
                    for fault in faults {
                        findings.push(Finding {
                            problem: format!("in {place}, {fault}"),
                            fix: "supply the keys exactly as `wxread keys` answers them; `wxread keys --help` says their shape".to_string(),
                        });
                    }
                    (None, KeysReport { from: Some(*from), databases: 0 })
                }
            }
        }
    }
}

fn judge_account(
    account: &AccountSeen,
    keys: Option<&Keys>,
    findings: &mut Vec<Finding>,
) -> AccountReport {
    let mut tally = Tally::new();
    let mut unkeyed = Vec::new();
    match &account.databases {
        Err(why) => findings.push(Finding {
            problem: format!(
                "account {}'s databases in {} cannot be listed ({why})",
                account.id, account.db_storage
            ),
            fix: ALLOW_READING.to_string(),
        }),
        Ok(databases) => {
            for path in databases {
                let keyed = keys.is_some_and(|keys| keys.get(path).is_some());
                tally = tally.count(keyed);
                if !keyed {
                    unkeyed.push(path.clone());
                }
            }
        }
    }
    // Missing keys are said once, above, when none were supplied at all.
    if keys.is_some() && !unkeyed.is_empty() {
        findings.push(Finding {
            problem: format!(
                "account {} has {} of {} databases with no key: {}",
                account.id,
                unkeyed.len(),
                tally.databases(),
                unkeyed.join(", ")
            ),
            fix: format!(
                "the keys supplied are another account's, or older than these databases: read them again for this account, {GET_KEYS}"
            ),
        });
    }
    AccountReport { id: account.id.clone(), db_storage: account.db_storage.clone(), tally, unkeyed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn account(id: &str, databases: &[&str]) -> AccountSeen {
        AccountSeen {
            id: id.to_string(),
            db_storage: format!("/x/{id}/db_storage"),
            databases: Ok(databases.iter().map(|d| d.to_string()).collect()),
        }
    }

    fn seen(listing: Listing, keys: KeysSeen) -> Seen {
        Seen { data_dir: "/x".into(), data_dir_from: Source::Default, listing, keys }
    }

    fn keys_of(paths: &[&str]) -> KeysSeen {
        let entries: Vec<String> =
            paths.iter().map(|p| format!("\"{p}\": {{\"enc_key\": \"{KEY}\"}}")).collect();
        KeysSeen::Text { from: Source::Var, text: format!("{{{}}}", entries.join(", ")) }
    }

    #[test]
    fn an_account_with_every_key_is_ok() {
        let paths = ["message/message_0.db", "contact/contact.db"];
        let report =
            diagnose(&seen(Listing::Accounts(vec![account("a_1", &paths)]), keys_of(&paths)));
        assert!(report.ok(), "{:?}", report.findings);
        assert_eq!(report.accounts[0].tally.databases(), 2);
        assert_eq!(report.accounts[0].tally.keyed(), 2);
        assert_eq!(report.keys.databases, 2);
        assert_eq!(report.keys.from, Some(Source::Var));
    }

    #[test]
    fn no_keys_is_one_finding_that_points_at_the_keys_help() {
        let report =
            diagnose(&seen(Listing::Accounts(vec![account("a_1", &["m.db"])]), KeysSeen::None));
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].fix.contains("`wxread keys --help`"));
        assert_eq!(report.accounts[0].unkeyed, ["m.db"]);
    }

    #[test]
    fn a_database_with_no_key_is_named_with_its_account() {
        let report = diagnose(&seen(
            Listing::Accounts(vec![account("a_1", &["m.db", "c.db"])]),
            keys_of(&["m.db"]),
        ));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].problem,
            "account a_1 has 1 of 2 databases with no key: c.db"
        );
    }

    #[test]
    fn every_fault_is_found_at_once() {
        let report = diagnose(&seen(
            Listing::Missing,
            KeysSeen::Text { from: Source::Flag, text: "{\"a.db\": {}, \"b.db\": 1}".into() },
        ));
        let problems: Vec<&str> = report.findings.iter().map(|f| f.problem.as_str()).collect();
        assert_eq!(
            problems,
            [
                "in the keys file, the key of a.db has no enc_key",
                "in the keys file, the key of b.db is not an object {\"enc_key\", \"salt\"}",
                "WeChat's data directory /x does not exist",
            ]
        );
        assert!(report.findings.iter().all(|f| !f.fix.is_empty()));
    }

    #[test]
    fn a_data_dir_given_names_the_default_in_its_fix() {
        let mut s = seen(Listing::NotADirectory, keys_of(&[]));
        s.data_dir_from = Source::Flag;
        let report = diagnose(&s);
        assert!(report.findings[0].fix.contains(DATA_DIR_UNDER_HOME));
    }

    #[test]
    fn what_cannot_be_read_says_to_allow_reading() {
        let mut a = account("a_1", &[]);
        a.databases = Err("Operation not permitted".into());
        for listing in
            [Listing::Unreadable("Operation not permitted".into()), Listing::Accounts(vec![a])]
        {
            let report = diagnose(&seen(listing, keys_of(&[])));
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.findings[0].fix, ALLOW_READING);
        }
    }

    #[test]
    fn no_account_says_to_log_in() {
        let report = diagnose(&seen(Listing::Accounts(vec![]), keys_of(&[])));
        assert!(report.findings[0].fix.starts_with("log in to WeChat"));
    }

    proptest! {
        #[test]
        fn a_tally_counts_what_it_was_given(keyed in proptest::collection::vec(any::<bool>(), 0..50)) {
            let tally = keyed.iter().fold(Tally::new(), |t, &k| t.count(k));
            prop_assert_eq!(tally.databases(), keyed.len());
            prop_assert_eq!(tally.keyed(), keyed.iter().filter(|&&k| k).count());
        }
    }
}
