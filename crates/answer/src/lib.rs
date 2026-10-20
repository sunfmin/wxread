//! What every answer that lists or dates something is written with: how
//! many things one page holds, the options that page, and a time as
//! RFC 3339 in UTC. Each list keeps its own cursor, since each sorts by
//! its own key; nothing here reads or writes.

use flux_rs::attrs::*;

/// The options that page.
pub const LIMIT_FLAG: &str = "--limit";
pub const CURSOR_FLAG: &str = "--cursor";

/// How many things a page holds unless `--limit` says, and at most.
pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 1000;

/// How many things one page holds: from one to [`MAX_LIMIT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[refined_by(n: int)]
#[invariant(1 <= n && n <= MAX_LIMIT)]
pub struct Limit {
    #[field(usize[n])]
    n: usize,
}

impl Limit {
    /// The limit `n`, when it is one.
    pub fn new(n: usize) -> Option<Limit> {
        match n {
            1..=MAX_LIMIT => Some(Limit { n }),
            _ => None,
        }
    }

    /// The limit `--limit` gave as `given` (none: [`DEFAULT_LIMIT`]), or
    /// what fixes it; `things` what a page holds (`chats`).
    pub fn parse(given: Option<&str>, things: &str) -> Result<Limit, String> {
        let Some(text) = given else {
            return Ok(Limit::default_limit());
        };
        text.parse().ok().and_then(Limit::new).ok_or_else(|| {
            format!(
                "{LIMIT_FLAG} {text} is not a number of {things} from 1 to {MAX_LIMIT}: pass one, or leave {LIMIT_FLAG} out for {DEFAULT_LIMIT}"
            )
        })
    }

    #[spec(fn() -> Limit[DEFAULT_LIMIT])]
    pub fn default_limit() -> Limit {
        Limit { n: DEFAULT_LIMIT }
    }

    #[spec(fn(Limit[@n]) -> usize{v: v == n && 1 <= v && v <= MAX_LIMIT})]
    pub fn get(self) -> usize {
        self.n
    }
}

/// `secs` since the epoch as RFC 3339 in UTC (`2026-10-20T01:02:03Z`); None
/// for none (0 or before) and for a time past the year 9999.
pub fn rfc3339(secs: i64) -> Option<String> {
    if secs <= 0 || secs > 253_402_300_799 {
        return None;
    }
    let days = secs.checked_div(86_400)?;
    let time = secs.checked_rem(86_400)?;
    // Howard Hinnant's civil_from_days, for a day on or after the epoch.
    let z = days.checked_add(719_468)?;
    let era = z.checked_div(146_097)?;
    let doe = z.checked_sub(era.checked_mul(146_097)?)?;
    let yoe = doe
        .checked_sub(doe.checked_div(1_460)?)?
        .checked_add(doe.checked_div(36_524)?)?
        .checked_sub(doe.checked_div(146_096)?)?
        .checked_div(365)?;
    let doy = doe.checked_sub(
        yoe.checked_mul(365)?
            .checked_add(yoe.checked_div(4)?)?
            .checked_sub(yoe.checked_div(100)?)?,
    )?;
    let mp = doy.checked_mul(5)?.checked_add(2)?.checked_div(153)?;
    let day =
        doy.checked_sub(mp.checked_mul(153)?.checked_add(2)?.checked_div(5)?)?.checked_add(1)?;
    let month = if mp < 10 { mp.checked_add(3)? } else { mp.checked_sub(9)? };
    let year = yoe.checked_add(era.checked_mul(400)?)?.checked_add(i64::from(month <= 2))?;
    let (hour, rest) = (time.checked_div(3_600)?, time.checked_rem(3_600)?);
    let (minute, second) = (rest.checked_div(60)?, rest.checked_rem(60)?);
    Some(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn times_are_rfc3339_in_utc() {
        assert_eq!(rfc3339(1).as_deref(), Some("1970-01-01T00:00:01Z"));
        assert_eq!(rfc3339(951_782_400).as_deref(), Some("2000-02-29T00:00:00Z"));
        assert_eq!(rfc3339(253_402_300_799).as_deref(), Some("9999-12-31T23:59:59Z"));
        assert_eq!(rfc3339(253_402_300_800), None);
        assert_eq!(rfc3339(-5), None);
    }

    #[test]
    fn a_limit_is_one_to_the_most() {
        assert_eq!(Limit::new(0), None);
        assert_eq!(Limit::new(MAX_LIMIT + 1), None);
        assert_eq!(Limit::new(MAX_LIMIT).map(Limit::get), Some(MAX_LIMIT));
    }

    #[test]
    fn a_limit_given_is_parsed_or_refused_with_its_fix() {
        assert_eq!(Limit::parse(None, "chats"), Ok(Limit::default_limit()));
        assert_eq!(Limit::parse(Some("7"), "chats").map(Limit::get), Ok(7));
        for text in ["0", "1001", "x", ""] {
            let why = Limit::parse(Some(text), "contacts").unwrap_err();
            assert!(why.contains("number of contacts from 1 to 1000"), "{why}");
            assert!(why.contains("leave --limit out for 100"), "{why}");
        }
    }

    proptest! {
        #[test]
        fn a_time_is_written_as_the_seconds_it_was(secs in 1i64..253_402_300_800) {
            let text = rfc3339(secs).unwrap();
            prop_assert_eq!(text.len(), 20);
            let (date, time) = text.split_once('T').unwrap();
            let parts: Vec<i64> = time.trim_end_matches('Z').split(':').map(|p| p.parse().unwrap()).collect();
            prop_assert_eq!(parts[0] * 3600 + parts[1] * 60 + parts[2], secs % 86_400);
            prop_assert!(date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-');
        }
    }
}
