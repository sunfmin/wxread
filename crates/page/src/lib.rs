//! What every list wxread answers shares: how many one page holds, the
//! options that page it, and times, written as RFC 3339 in UTC and read
//! back from what an agent passes. Nothing here reads or writes.

use flux_rs::attrs::*;

/// The options that page.
pub const LIMIT_FLAG: &str = "--limit";
pub const CURSOR_FLAG: &str = "--cursor";

/// How many a page holds unless `--limit` says, and at most.
pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 1000;

/// How many one page holds: from one to [`MAX_LIMIT`].
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

    #[spec(fn() -> Limit[DEFAULT_LIMIT])]
    pub fn default_limit() -> Limit {
        Limit { n: DEFAULT_LIMIT }
    }

    #[spec(fn(Limit[@n]) -> usize{v: v == n && 1 <= v && v <= MAX_LIMIT})]
    pub fn get(self) -> usize {
        self.n
    }

    /// The limit `--limit` says (`given`, None when it is left out), or
    /// what fixes it; `things` what a page holds (`chats`).
    pub fn parse(given: Option<&str>, things: &str) -> Result<Limit, String> {
        match given {
            None => Ok(Limit::default_limit()),
            Some(text) => text.parse().ok().and_then(Limit::new).ok_or_else(|| {
                format!(
                    "{LIMIT_FLAG} {text} is not a number of {things} from 1 to {MAX_LIMIT}: pass one, or leave {LIMIT_FLAG} out for {DEFAULT_LIMIT}"
                )
            }),
        }
    }
}

/// The latest second a time is written for: 9999-12-31T23:59:59Z.
const LAST_SECOND: i64 = 253_402_300_799;

/// `secs` since the epoch as RFC 3339 in UTC (`2026-10-20T01:02:03Z`); None
/// for none (0 or before) and for a time past the year 9999.
pub fn rfc3339(secs: i64) -> Option<String> {
    if secs <= 0 || secs > LAST_SECOND {
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

/// The second since the epoch `text` names: RFC 3339 with its offset
/// (`2026-10-20T09:30:00+08:00`, `2026-10-20T01:30:00Z`), or a date
/// (`2026-10-20`), its midnight in UTC. None for anything else, and for a
/// time before 1970 or past the year 9999.
pub fn seconds(text: &str) -> Option<i64> {
    let (date, time) = match text.split_once(['T', 't']) {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let mut parts = date.split('-');
    let year = digits(parts.next()?, 4)?;
    let month = digits(parts.next()?, 2)?;
    let day = digits(parts.next()?, 2)?;
    if parts.next().is_some() {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let of_day = match time {
        None => 0,
        Some(time) => clock(time)?,
    };
    let secs = days.checked_mul(86_400)?.checked_add(of_day)?;
    if secs.is_negative() || secs > LAST_SECOND { None } else { Some(secs) }
}

/// The seconds `time` (`09:30:00+08:00`, `01:30:00Z`) is past midnight in
/// UTC, its offset taken off: below zero or past a day when the offset
/// moves it to the day before or after.
fn clock(time: &str) -> Option<i64> {
    let (at, offset) = if let Some(at) = time.strip_suffix(['Z', 'z']) {
        (at, 0)
    } else {
        let split = time.rfind(['+', '-'])?;
        let (at, zone) = time.split_at(split);
        let (sign, zone) = zone.split_at(1);
        let (hours, minutes) = zone.split_once(':')?;
        let minutes = digits(hours, 2)?.checked_mul(60)?.checked_add(digits(minutes, 2)?)?;
        if minutes >= 24 * 60 {
            return None;
        }
        let offset = minutes.checked_mul(60)?;
        (at, if sign == "-" { offset.checked_neg()? } else { offset })
    };
    let mut parts = at.split(':');
    let hour = digits(parts.next()?, 2)?;
    let minute = digits(parts.next()?, 2)?;
    let second = digits(parts.next()?, 2)?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    hour.checked_mul(3_600)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)?
        .checked_sub(offset)
}

/// The number `text` writes in exactly `width` decimal digits.
fn digits(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The days from 1970-01-01 to `year`-`month`-`day`, when that is a day
/// of the year 1970 or after.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if year < 1970 || day < 1 || day > last {
        return None;
    }
    // Howard Hinnant's days_from_civil, for a year after 0.
    let y = year.checked_sub(i64::from(month <= 2))?;
    let era = y.checked_div(400)?;
    let yoe = y.checked_sub(era.checked_mul(400)?)?;
    let shifted = if month > 2 { month.checked_sub(3)? } else { month.checked_add(9)? };
    let doy = shifted
        .checked_mul(153)?
        .checked_add(2)?
        .checked_div(5)?
        .checked_add(day)?
        .checked_sub(1)?;
    let doe = yoe
        .checked_mul(365)?
        .checked_add(yoe.checked_div(4)?)?
        .checked_sub(yoe.checked_div(100)?)?
        .checked_add(doy)?;
    era.checked_mul(146_097)?.checked_add(doe)?.checked_sub(719_468)
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
    fn a_time_is_read_with_its_offset_and_a_date_is_its_midnight_in_utc() {
        assert_eq!(seconds("2025-10-09T08:53:20Z"), Some(1_760_000_000));
        assert_eq!(seconds("2025-10-09T16:53:20+08:00"), Some(1_760_000_000));
        assert_eq!(seconds("2025-10-09T03:53:20-05:00"), Some(1_760_000_000));
        assert_eq!(seconds("2025-10-09"), Some(1_759_968_000));
        assert_eq!(seconds("2000-02-29"), Some(951_782_400));
        assert_eq!(seconds("1970-01-01T08:00:00+08:00"), Some(0));
    }

    #[test]
    fn what_is_not_a_time_is_none() {
        for text in [
            "",
            "yesterday",
            "1760000000",
            "2025-10-9",
            "2025-13-01",
            "2025-02-29",
            "2025-10-09T08:53:20",
            "2025-10-09T24:00:00Z",
            "2025-10-09T08:53Z",
            "2025-10-09T08:53:20+0800",
            "1969-12-31",
            "1970-01-01T07:59:59+08:00",
            "+025-10-09",
        ] {
            assert_eq!(seconds(text), None, "{text}");
        }
    }

    #[test]
    fn a_limit_is_one_to_the_most() {
        assert_eq!(Limit::new(0), None);
        assert_eq!(Limit::new(MAX_LIMIT + 1), None);
        assert_eq!(Limit::new(MAX_LIMIT).map(Limit::get), Some(MAX_LIMIT));
        assert_eq!(Limit::parse(None, "chats"), Ok(Limit::default_limit()));
        assert_eq!(Limit::parse(Some("7"), "chats").map(Limit::get), Ok(7));
        for given in ["0", "1001", "x", ""] {
            let why = Limit::parse(Some(given), "messages").unwrap_err();
            assert!(why.contains("number of messages from 1 to 1000"), "{why}");
        }
    }

    proptest! {
        #[test]
        fn a_time_is_written_as_the_seconds_it_was(secs in 1i64..253_402_300_800) {
            let text = rfc3339(secs).unwrap();
            prop_assert_eq!(text.len(), 20);
            prop_assert_eq!(seconds(&text), Some(secs));
        }
    }
}
