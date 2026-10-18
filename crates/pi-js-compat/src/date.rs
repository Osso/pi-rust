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

/// [`date_parse`] as integral epoch milliseconds; `None` where JavaScript returns `NaN`.
pub fn date_parse_millis(input: &str) -> Option<i64> {
    let millis = date_parse(input);
    (!millis.is_nan()).then_some(millis as i64)
}

/// `Date.parse` resolved against an explicit time zone (the testable core).
pub fn date_parse_with_time_zone(input: &str, zone: &TimeZone) -> f64 {
    parse_date(input, zone).map_or(f64::NAN, |millis| millis as f64)
}

fn parse_date(input: &str, zone: &TimeZone) -> Option<i64> {
    if let Some(millis) = parse_http_date(input, zone) {
        return Some(millis);
    }
    parse_iso8601_millis(input, zone)
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
    let (year, month, day, hour, minute, second, millis) = civil_from_millis(wall_millis);
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

fn civil_from_millis(millis: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let days = millis.div_euclid(MS_PER_DAY);
    let within_day = millis.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = within_day / 3_600_000;
    let minute = (within_day % 3_600_000) / 60_000;
    let second = (within_day % 60_000) / 1_000;
    let millis = within_day % 1_000;
    (year, month, day, hour, minute, second, millis)
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
