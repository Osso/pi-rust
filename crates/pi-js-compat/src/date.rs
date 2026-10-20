//! JavaScript `Date.parse` for the V8-supported ISO 8601 and HTTP-date formats.

use std::time::{SystemTime, UNIX_EPOCH};

use jiff::civil;
use jiff::tz::TimeZone;

const MS_PER_DAY: i64 = 86_400_000;
const MAX_DATE_MILLIS: i64 = 8_640_000_000_000_000;

/// JavaScript `Date.parse(string)` in the process-local time zone.
pub fn date_parse(input: &str) -> f64 {
    date_parse_with_time_zone(input, &TimeZone::system())
}

/// JavaScript `Date.now()` in epoch milliseconds.
pub fn js_now_millis() -> f64 {
    system_time_millis(SystemTime::now()).map_or(f64::NAN, |millis| millis as f64)
}

/// [`date_parse`] as integral epoch milliseconds; `None` where JavaScript returns `NaN`.
pub fn date_parse_millis(input: &str) -> Option<i64> {
    let millis = date_parse(input);
    (!millis.is_nan()).then_some(millis as i64)
}

/// `Date.parse` resolved against an explicit time zone (the testable core).
pub fn date_parse_with_time_zone(input: &str, zone: &TimeZone) -> f64 {
    parse_date(input, zone).map_or(f64::NAN, |millis| millis as f64)
}

/// Local-time fields of a JavaScript `Date`: `getFullYear()`, `getMonth()`
/// (0-based), `getDate()`, `getHours()` and `getMinutes()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateFields {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hours: i64,
    pub minutes: i64,
}

/// Local-time fields for epoch milliseconds in the process time zone; `None`
/// for an invalid date (where every JavaScript getter returns `NaN`).
pub fn local_date_fields(millis: i64) -> Option<LocalDateFields> {
    local_date_fields_in(millis, &TimeZone::system())
}

/// UTC fields of a JavaScript `Date` (`getUTCFullYear`, `getUTCMonth`,
/// `getUTCDate`, `getUTCHours`, `getUTCMinutes`).
pub fn utc_date_fields(millis: i64) -> Option<LocalDateFields> {
    local_date_fields_in(millis, &TimeZone::UTC)
}

/// [`local_date_fields`] resolved against an explicit time zone.
pub fn local_date_fields_in(millis: i64, zone: &TimeZone) -> Option<LocalDateFields> {
    if !(-MAX_DATE_MILLIS..=MAX_DATE_MILLIS).contains(&millis) {
        return None;
    }
    let zoned = jiff::Timestamp::from_millisecond(millis).ok()?.to_zoned(zone.clone());
    Some(LocalDateFields {
        year: i64::from(zoned.year()),
        month: i64::from(zoned.month()) - 1,
        day: i64::from(zoned.day()),
        hours: i64::from(zoned.hour()),
        minutes: i64::from(zoned.minute()),
    })
}

fn parse_date(input: &str, zone: &TimeZone) -> Option<i64> {
    if let Some(millis) = parse_http_date(input, zone) {
        return Some(millis);
    }
    if let Some(millis) = parse_iso8601_millis(input, zone) {
        return Some(millis);
    }
    if let Some(millis) = parse_numeric_date(input, zone) {
        return Some(millis);
    }
    parse_legacy_date(input, zone)
}

fn legacy_month(word: &str) -> Option<i64> {
    match word.get(..3)?.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn legacy_number(word: &str) -> Option<i64> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

fn legacy_date_fields(words: &[&str]) -> Option<(i64, i64, i64)> {
    let (year, month, day) = if let Some(month) = legacy_month(words[0]) {
        (legacy_number(words[2])?, month, legacy_number(words[1])?)
    } else if let Some(month) = legacy_month(words[1]) {
        let first = legacy_number(words[0])?;
        let last = legacy_number(words[2])?;
        if first > 31 {
            (first, month, last)
        } else {
            (last, month, first)
        }
    } else {
        let first = legacy_number(words[0])?;
        let second = legacy_number(words[1])?;
        let last = legacy_number(words[2])?;
        if first > 31 {
            (first, second, last)
        } else {
            (last, first, second)
        }
    };
    let year = match year {
        0..=49 => year + 2000,
        50..=99 => year + 1900,
        _ => year,
    };
    if year > 275760 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    normalize_day(year, month, day)
}

fn legacy_time(words: &[&str], named_month: bool) -> Option<TimeSuffix> {
    let mut words: Vec<_> = words.iter().map(|word| (*word).to_owned()).collect();
    if let Some(last) = words.last_mut()
        && (last.eq_ignore_ascii_case("GMT") || last.eq_ignore_ascii_case("UTC"))
    {
        *last = "Z".into();
    }
    if words.is_empty() || words == ["Z"] {
        return Some(TimeSuffix {
            hour: 0,
            minute: 0,
            second: 0,
            millis: 0,
            offset_minutes: (!words.is_empty()).then_some(0),
        });
    }
    let suffix = format!(" {}", words.join(" "));
    let mut index = 0;
    let mut time = parse_time_suffix(suffix.as_bytes(), &mut index)?;
    // V8 discards an out-of-range seconds field in named-month legacy dates,
    // but rejects it in slash and ISO forms (Node differential boundary proof).
    if named_month && time.second >= 60 {
        time.second = 0;
    }
    (index == suffix.len() && time.is_valid()).then_some(time)
}

fn partition_legacy_date_words<'a>(words: &[&'a str]) -> Option<(Vec<&'a str>, Vec<&'a str>)> {
    let first = *words.first()?;
    let (date, remaining) = if first.contains(['/', '-']) {
        let date: Vec<_> = first.split(['/', '-']).collect();
        (date, words[1..].to_vec())
    } else if words.get(2).is_some_and(|word| word.contains(':')) && words.len() >= 4 {
        (
            vec![words[0], words[1], words[3]],
            std::iter::once(words[2]).chain(words[4..].iter().copied()).collect(),
        )
    } else if words.len() >= 3 {
        (words[..3].to_vec(), words[3..].to_vec())
    } else {
        return None;
    };
    (date.len() == 3).then_some((date, remaining))
}

fn parse_legacy_date(input: &str, zone: &TimeZone) -> Option<i64> {
    let mut words: Vec<_> = input
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|word| !word.is_empty())
        .collect();
    if words.first().and_then(|word| word.get(..3)).is_some_and(|prefix| {
        matches!(
            prefix.to_ascii_lowercase().as_str(),
            "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun"
        )
    }) {
        words.remove(0);
    }
    let (date, remaining) = partition_legacy_date_words(&words)?;
    let (year, month, day) = legacy_date_fields(&date)?;
    let time = legacy_time(&remaining, date.iter().any(|word| legacy_month(word).is_some()))?;
    finish_date(
        days_from_civil(year, month, day),
        time.hour,
        time.minute,
        time.second,
        time.millis,
        time.offset_minutes,
        zone,
    )
}

// V8's legacy single-number grammar: 0 is Jan 2000, 1..12 are months
// in 2001, 13..31 are invalid, 32..99 are two-digit years. Strict ISO
// four-digit/signed-six-digit years have already been handled above.
fn parse_numeric_date(input: &str, zone: &TimeZone) -> Option<i64> {
    let input = crate::string::js_trim(input);
    let (digits, utc) = match input.strip_suffix(['Z', 'z']) {
        Some(digits) => (crate::string::js_trim(digits), true),
        None => (input, false),
    };
    let digits = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = digits.parse::<i64>().ok()?;
    let (year, month) = match number {
        0 => (2000, 1),
        1..=12 => (2001, number),
        13..=31 => return None,
        32..=49 => (2000 + number, 1),
        50..=99 => (1900 + number, 1),
        _ => (number, 1),
    };
    if year > 275760 {
        return None;
    }
    finish_date(days_from_civil(year, month, 1), 0, 0, 0, 0, utc.then_some(0), zone)
}

fn parse_http_date(input: &str, zone: &TimeZone) -> Option<i64> {
    let time = httpdate::parse_http_date(input).ok()?;
    let utc_millis = system_time_millis(time)?;
    if input.contains("GMT") {
        return in_range(utc_millis);
    }
    // The asctime form has no zone; V8 reads its wall clock in local time.
    resolve_local_from_wall(utc_millis, zone)
}

fn system_time_millis(time: SystemTime) -> Option<i64> {
    if let Ok(elapsed) = time.duration_since(UNIX_EPOCH) {
        return i64::try_from(elapsed.as_millis()).ok();
    }
    let elapsed = UNIX_EPOCH.duration_since(time).ok()?;
    i64::try_from(elapsed.as_millis()).ok().map(|millis| -millis)
}

struct DatePrefix {
    year: i64,
    month: i64,
    day: i64,
    padded_month: bool,
    padded_day: bool,
    has_month: bool,
    has_day: bool,
}

struct TimeSuffix {
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
    offset_minutes: Option<i64>,
}

fn parse_iso8601_millis(text: &str, zone: &TimeZone) -> Option<i64> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let prefix = parse_date_prefix(bytes, &mut index)?;
    if !(1..=12).contains(&prefix.month) || !(1..=31).contains(&prefix.day) {
        return None;
    }
    let (year, month, day) = normalize_day(prefix.year, prefix.month, prefix.day)?;
    let days = days_from_civil(year, month, day);
    if index == bytes.len() {
        let utc = !prefix.has_month || (prefix.padded_month && (!prefix.has_day || prefix.padded_day));
        return finish_date(days, 0, 0, 0, 0, if utc { Some(0) } else { None }, zone);
    }
    if is_zulu_suffix(bytes, index) {
        return finish_date(days, 0, 0, 0, 0, Some(0), zone);
    }
    if !prefix.is_strict_date() {
        return None;
    }
    let time = parse_time_suffix(bytes, &mut index)?;
    if index != bytes.len() || !time.is_valid() {
        return None;
    }
    finish_date(
        days,
        time.hour,
        time.minute,
        time.second,
        time.millis,
        time.offset_minutes,
        zone,
    )
}

impl DatePrefix {
    fn is_strict_date(&self) -> bool {
        self.has_month && self.has_day && self.padded_month && self.padded_day
    }
}

impl TimeSuffix {
    fn is_valid(&self) -> bool {
        self.hour <= 24
            && self.minute <= 59
            && self.second <= 59
            && (self.hour != 24 || (self.minute == 0 && self.second == 0 && self.millis == 0))
    }
}

fn parse_date_prefix(bytes: &[u8], index: &mut usize) -> Option<DatePrefix> {
    let year = parse_year(bytes, index)?;
    let mut prefix = DatePrefix {
        year,
        month: 1,
        day: 1,
        padded_month: false,
        padded_day: false,
        has_month: false,
        has_day: false,
    };
    if bytes.get(*index) != Some(&b'-') {
        return Some(prefix);
    }
    *index += 1;
    let (month, month_digits) = parse_variable_digits(bytes, index)?;
    prefix.month = month;
    prefix.padded_month = month_digits == 2;
    prefix.has_month = true;
    if bytes.get(*index) != Some(&b'-') {
        return Some(prefix);
    }
    *index += 1;
    let (day, day_digits) = parse_variable_digits(bytes, index)?;
    prefix.day = day;
    prefix.padded_day = day_digits == 2;
    prefix.has_day = true;
    Some(prefix)
}

fn is_zulu_suffix(bytes: &[u8], index: usize) -> bool {
    matches!(bytes.get(index), Some(b'Z' | b'z')) && index + 1 == bytes.len()
}

fn parse_time_suffix(bytes: &[u8], index: &mut usize) -> Option<TimeSuffix> {
    let whitespace_separator = parse_time_separator(bytes, index)?;
    let hour = parse_two_digits(bytes, index)?;
    expect_byte(bytes, index, b':')?;
    let minute = parse_two_digits(bytes, index)?;
    let second = parse_optional_seconds(bytes, index)?;
    let millis = parse_optional_fraction(bytes, index)?;
    if whitespace_separator {
        skip_whitespace(bytes, index);
    }
    let offset_minutes = parse_zone(bytes, index)?;
    Some(TimeSuffix {
        hour,
        minute,
        second,
        millis,
        offset_minutes,
    })
}

fn parse_time_separator(bytes: &[u8], index: &mut usize) -> Option<bool> {
    match bytes.get(*index) {
        Some(b'T' | b't') => {
            *index += 1;
            Some(false)
        }
        Some(byte) if is_ascii_whitespace(*byte) => {
            skip_whitespace(bytes, index);
            Some(true)
        }
        _ => None,
    }
}

fn skip_whitespace(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(|byte| is_ascii_whitespace(*byte)) {
        *index += 1;
    }
}

fn parse_optional_seconds(bytes: &[u8], index: &mut usize) -> Option<i64> {
    if bytes.get(*index) != Some(&b':') {
        return Some(0);
    }
    *index += 1;
    parse_two_digits(bytes, index)
}

fn parse_optional_fraction(bytes: &[u8], index: &mut usize) -> Option<i64> {
    if bytes.get(*index) != Some(&b'.') {
        return Some(0);
    }
    *index += 1;
    parse_fraction_millis(bytes, index)
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn finish_date(
    days: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
    offset_minutes: Option<i64>,
    zone: &TimeZone,
) -> Option<i64> {
    let wall = days
        .checked_mul(MS_PER_DAY)?
        .checked_add((hour * 3_600 + minute * 60 + second).checked_mul(1_000)?)?
        .checked_add(millis)?;
    match offset_minutes {
        Some(offset) => in_range(wall.checked_sub(offset.checked_mul(60_000)?)?),
        None => resolve_local_from_wall(wall, zone),
    }
}

fn resolve_local_from_wall(wall_millis: i64, zone: &TimeZone) -> Option<i64> {
    let UtcDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond: millis,
    } = utc_date_time(wall_millis);
    let datetime = clamped_datetime(year, month, day, hour, minute, second, millis)?;
    if (-9999..=9999).contains(&year)
        && let Ok(zoned) = datetime.to_zoned(zone.clone())
    {
        return in_range(zoned.timestamp().as_millisecond());
    }
    // Outside jiff's timestamp range the offset is taken from the nearest
    // representable year; POSIX TZ rules (and LMT for ancient dates) are stable
    // there.
    let offset_year = year.clamp(-9998, 9998);
    let offset_datetime = clamped_datetime(offset_year, month, day, hour, minute, second, millis)?;
    let zoned = offset_datetime.to_zoned(zone.clone()).ok()?;
    let offset_seconds = i64::from(zoned.offset().seconds());
    in_range(wall_millis.checked_sub(offset_seconds.checked_mul(1_000)?)?)
}

fn clamped_datetime(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
) -> Option<civil::DateTime> {
    let year = year.clamp(-9999, 9999) as i16;
    let mut day = day as i8;
    loop {
        if let Ok(datetime) = civil::DateTime::new(
            year,
            month as i8,
            day,
            hour as i8,
            minute as i8,
            second as i8,
            i32::try_from(millis * 1_000_000).ok()?,
        ) {
            return Some(datetime);
        }
        if day <= 1 {
            return None;
        }
        day -= 1;
    }
}

fn in_range(millis: i64) -> Option<i64> {
    (millis.abs() <= MAX_DATE_MILLIS).then_some(millis)
}

fn parse_year(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let sign = match bytes.get(*index) {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => 0,
    };
    let digits = if sign == 0 { 4 } else { 6 };
    let start = *index + usize::from(sign != 0);
    let year = parse_fixed_digits(bytes.get(start..start + digits)?)?;
    *index = start + digits;
    Some(if sign == 0 { year } else { sign * year })
}

fn parse_variable_digits(bytes: &[u8], index: &mut usize) -> Option<(i64, usize)> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    let digits = bytes.get(start..*index)?;
    if digits.is_empty() {
        return None;
    }
    Some((parse_fixed_digits(digits)?, digits.len()))
}

fn parse_two_digits(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let value = parse_fixed_digits(bytes.get(*index..*index + 2)?)?;
    *index += 2;
    Some(value)
}

fn parse_fixed_digits(bytes: &[u8]) -> Option<i64> {
    bytes.iter().try_fold(0_i64, |value, byte| {
        let digit = byte.checked_sub(b'0').filter(|digit| *digit < 10)?;
        value.checked_mul(10)?.checked_add(i64::from(digit))
    })
}

fn parse_fraction_millis(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    let digits = bytes.get(start..*index)?;
    if digits.is_empty() {
        return None;
    }
    let kept = &digits[..digits.len().min(3)];
    let millis = parse_fixed_digits(kept)?;
    Some(millis * 10_i64.pow(3 - kept.len() as u32))
}

fn parse_zone(bytes: &[u8], index: &mut usize) -> Option<Option<i64>> {
    match bytes.get(*index) {
        None => Some(None),
        Some(b'Z' | b'z') => {
            *index += 1;
            Some(Some(0))
        }
        Some(b'+' | b'-') => {
            let sign = if bytes[*index] == b'-' { -1 } else { 1 };
            *index += 1;
            let hours = parse_two_digits(bytes, index)?;
            if bytes.get(*index) == Some(&b':') {
                *index += 1;
            }
            let minutes = parse_two_digits(bytes, index)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            Some(Some(sign * (hours * 60 + minutes)))
        }
        Some(_) => None,
    }
}

fn expect_byte(bytes: &[u8], index: &mut usize, expected: u8) -> Option<()> {
    (bytes.get(*index) == Some(&expected)).then(|| *index += 1)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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

/// Proleptic-Gregorian UTC fields of epoch milliseconds; `month` is 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcDateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub millisecond: i64,
}

/// Splits epoch milliseconds into UTC calendar fields.
pub fn utc_date_time(millis: i64) -> UtcDateTime {
    let within_day = millis.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(millis.div_euclid(MS_PER_DAY));
    UtcDateTime {
        year,
        month,
        day,
        hour: within_day / 3_600_000,
        minute: (within_day % 3_600_000) / 60_000,
        second: (within_day % 60_000) / 1_000,
        millisecond: within_day % 1_000,
    }
}

/// `new Date(millis).toISOString()` for a valid date: four-digit years in
/// 0..=9999, otherwise a signed six-digit expanded year.
pub fn to_iso_string(millis: i64) -> String {
    let UtcDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
    } = utc_date_time(millis);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{}{:06}", if year < 0 { '-' } else { '+' }, year.abs())
    };
    format!("{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z")
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

#[cfg(test)]
mod tests {
    use super::{js_now_millis, system_time_millis, utc_date_fields};

    #[test]
    fn utc_fields_match_javascript_get_utc_getters() {
        let fields = utc_date_fields(1_800_000_000_000).expect("valid date");
        assert_eq!(
            (fields.year, fields.month, fields.day, fields.hours, fields.minutes),
            (2027, 0, 15, 8, 0)
        );
        assert!(utc_date_fields(i64::MAX).is_none());
    }

    #[test]
    fn now_millis_tracks_the_system_clock() {
        let before = system_time_millis(std::time::SystemTime::now()).expect("system time");
        let now = js_now_millis();
        let after = system_time_millis(std::time::SystemTime::now()).expect("system time");
        assert!(
            (now as i64) >= before && (now as i64) <= after,
            "{now} not in [{before}, {after}]"
        );
    }
}
