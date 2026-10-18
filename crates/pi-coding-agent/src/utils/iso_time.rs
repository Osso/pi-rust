//! ISO-8601 timestamps: `Date#toISOString`, `new Date().toISOString()`, and the
//! ECMAScript date-time subset of `Date.parse`.

use std::time::{SystemTime, UNIX_EPOCH};

const MS_PER_DAY: i64 = 86_400_000;

/// `new Date().toISOString()`.
pub fn now_iso_string() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch");
    format_iso_millis(i64::try_from(elapsed.as_millis()).expect("timestamp fits in i64"))
}

/// `new Date(epoch_ms).toISOString()`.
pub fn format_iso_millis(epoch_ms: i64) -> String {
    let days = epoch_ms.div_euclid(MS_PER_DAY);
    let within_day = epoch_ms.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let minutes = within_day / 60_000;
    let seconds = (within_day % 60_000) / 1000;
    let millis = within_day % 1000;
    format!(
        "{}-{month:02}-{day:02}T{:02}:{:02}:{seconds:02}.{millis:03}Z",
        format_year(year),
        minutes / 60,
        minutes % 60,
    )
}

fn format_year(year: i64) -> String {
    if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{}{:06}", if year < 0 { '-' } else { '+' }, year.abs())
    }
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `Date.parse` for the ECMAScript date-time string format (what `toISOString`
/// produces). Values outside that grammar yield `None`, which `Date.parse`
/// reports as `NaN`.
pub fn parse_javascript_date(value: &str) -> Option<i64> {
    let parts = DateParts::parse(value)?;
    let local_millis = parts.to_epoch_millis()?;
    match parts.offset_minutes {
        Some(offset_minutes) => Some(local_millis - offset_minutes * 60_000),
        None => local_offset_minutes(local_millis).map(|offset| local_millis - offset * 60_000),
    }
}

/// Offset-less date-time strings are local time, as in `Date.parse`.
fn local_offset_minutes(epoch_ms: i64) -> Option<i64> {
    let timestamp = jiff::Timestamp::from_millisecond(epoch_ms).ok()?;
    let zoned = timestamp.to_zoned(jiff::tz::TimeZone::system());
    Some(i64::from(zoned.offset().seconds()) / 60)
}

struct DateParts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    /// `None` means a local (offset-less) date-time; date-only strings are UTC.
    offset_minutes: Option<i64>,
}

impl DateParts {
    fn parse(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        let (year, after_year) = parse_year(bytes)?;
        let month = parse_fixed(bytes, after_year + 1, 2)?;
        expect_byte(bytes, after_year + 3, b'-')?;
        let day = parse_fixed(bytes, after_year + 4, 2)?;
        let rest = bytes.get(after_year + 6..)?;
        match rest {
            [] => Some(Self::date_only(year, month, day)),
            [b'Z' | b'z'] => Some(Self::date_only(year, month, day)),
            [b'T' | b't' | b' ', time @ ..] => Self::parse_time(time, year, month, day),
            _ => None,
        }
    }

    fn date_only(year: i64, month: i64, day: i64) -> Self {
        Self {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
            offset_minutes: Some(0),
        }
    }

    fn parse_time(rest: &[u8], year: i64, month: i64, day: i64) -> Option<Self> {
        let hour = parse_fixed(rest, 0, 2)?;
        expect_byte(rest, 2, b':')?;
        let minute = parse_fixed(rest, 3, 2)?;
        let (second, millisecond, index) = parse_seconds(rest, 5)?;
        let offset_minutes = parse_optional_offset(rest.get(index..)?)?;
        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
            offset_minutes,
        })
    }

    fn to_epoch_millis(&self) -> Option<i64> {
        let (year, month, day) = normalize_day(self.year, self.month, self.day)?;
        if self.hour > 24 || self.minute > 59 || self.second > 59 {
            return None;
        }
        if self.hour == 24 && (self.minute != 0 || self.second != 0 || self.millisecond != 0) {
            return None;
        }
        let days = days_from_civil(year, month, day);
        Some(days * MS_PER_DAY + (self.hour * 3_600 + self.minute * 60 + self.second) * 1000 + self.millisecond)
    }
}

/// `MakeDay` normalization: day `31` of a 30-day month rolls into the next month;
/// day `0` or `32` is out of range.
fn normalize_day(year: i64, month: i64, day: i64) -> Option<(i64, i64, i64)> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let month_days = days_in_month(year, month);
    let months_ahead = (day - 1) / month_days;
    let normalized_month = month + months_ahead;
    Some((
        year + (normalized_month - 1) / 12,
        (normalized_month - 1) % 12 + 1,
        (day - 1) % month_days + 1,
    ))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn parse_year(bytes: &[u8]) -> Option<(i64, usize)> {
    match bytes.first()? {
        b'+' | b'-' => {
            let digits = parse_digits(bytes.get(1..7)?)?;
            let year = if bytes[0] == b'-' { -digits } else { digits };
            Some((year, 7))
        }
        _ => Some((parse_digits(bytes.get(0..4)?)?, 4)),
    }
}

/// Returns `(second, millisecond, next index)`; seconds are optional.
fn parse_seconds(rest: &[u8], index: usize) -> Option<(i64, i64, usize)> {
    if rest.get(index) != Some(&b':') {
        return Some((0, 0, index));
    }
    let second = parse_fixed(rest, index + 1, 2)?;
    let after_seconds = index + 3;
    if rest.get(after_seconds) != Some(&b'.') {
        return Some((second, 0, after_seconds));
    }
    let digits: Vec<u8> = rest
        .get(after_seconds + 1..)?
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .copied()
        .collect();
    if digits.is_empty() {
        return None;
    }
    let significant: Vec<u8> = digits.iter().take(3).copied().collect();
    let scale = 10_i64.pow(u32::try_from(3 - significant.len()).ok()?);
    Some((
        second,
        parse_digits(&significant)? * scale,
        after_seconds + 1 + digits.len(),
    ))
}

fn parse_optional_offset(rest: &[u8]) -> Option<Option<i64>> {
    match rest {
        [] => Some(None),
        [b'Z' | b'z'] => Some(Some(0)),
        [sign @ (b'+' | b'-'), ..] => parse_numeric_offset(rest, *sign).map(Some),
        _ => None,
    }
}

fn parse_numeric_offset(rest: &[u8], sign: u8) -> Option<i64> {
    let hours = parse_fixed(rest, 1, 2)?;
    let minutes = match rest {
        [_, _, _, b':', _, _] => parse_fixed(rest, 4, 2)?,
        [_, _, _, _, _] => parse_fixed(rest, 3, 2)?,
        _ => return None,
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    let total = hours * 60 + minutes;
    Some(if sign == b'-' { -total } else { total })
}

fn parse_fixed(bytes: &[u8], index: usize, length: usize) -> Option<i64> {
    parse_digits(bytes.get(index..index + length)?)
}

fn parse_digits(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    bytes.iter().try_fold(0_i64, |total, byte| {
        total.checked_mul(10)?.checked_add(i64::from(byte - b'0'))
    })
}

fn expect_byte(bytes: &[u8], index: usize, expected: u8) -> Option<()> {
    (bytes.get(index) == Some(&expected)).then_some(())
}
