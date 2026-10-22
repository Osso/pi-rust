//! JavaScript date helpers; Date.parse uses the Node v26.11.1 V8 scanner/composers.

#[path = "date_parser.rs"]
mod parser;

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

/// `Date.prototype.toLocaleString()` pinned to en-US, in the process-local zone.
/// Returns `None` if the local zone cannot be resolved or the timestamp is
/// outside Jiff's supported calendar range. Approved locale pin:
/// 2026-10-20; spaces match Node v26.11.1 (ASCII, including before AM/PM).
pub fn to_locale_string(millis: i64) -> Option<String> {
    let zone = TimeZone::try_system().ok()?;
    let date = jiff::Timestamp::from_millisecond(millis).ok()?.to_zoned(zone);
    let hour = i32::from(date.hour());
    let hour12 = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    let year = i32::from(date.year());
    let display_year = if year <= 0 { 1 - year } else { year };
    Some(format!(
        "{}/{}/{display_year}, {hour12}:{:02}:{:02} {}",
        date.month(),
        date.day(),
        date.minute(),
        date.second(),
        if hour < 12 { "AM" } else { "PM" },
    ))
}

/// JavaScript `TimeClip`: integral epoch milliseconds, or `None` for Invalid Date.
/// Call only at an actual `new Date(number)` boundary, not on message timestamps.
pub fn time_clip_millis(millis: f64) -> Option<i64> {
    if !millis.is_finite() || millis.abs() > MAX_DATE_MILLIS as f64 {
        return None;
    }
    Some(millis.trunc() as i64)
}

/// JavaScript Date's numeric time value: TimeClip at `new Date(number)`.
pub fn time_clip(millis: f64) -> f64 {
    time_clip_millis(millis).map_or(f64::NAN, |millis| millis as f64)
}

/// `Date.prototype.toISOString` rejects Invalid Date with RangeError.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTimeValue;

impl InvalidTimeValue {
    pub fn name(&self) -> &'static str {
        "RangeError"
    }
}
impl std::fmt::Display for InvalidTimeValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Invalid time value")
    }
}
impl std::error::Error for InvalidTimeValue {}

/// `new Date(millis).toISOString()`, including the invalid-date failure boundary.
pub fn to_iso_string_value(millis: f64) -> Result<String, InvalidTimeValue> {
    time_clip_millis(millis).map(to_iso_string).ok_or(InvalidTimeValue)
}

/// [`date_parse`] as integral epoch milliseconds; `None` where JavaScript returns `NaN`.
pub fn date_parse_millis(input: &str) -> Option<i64> {
    let millis = date_parse(input);
    (!millis.is_nan()).then_some(millis as i64)
}

/// `Date.parse` resolved against an explicit time zone (the testable core).
pub fn date_parse_with_time_zone(input: &str, zone: &TimeZone) -> f64 {
    date_parse_utf16_with_time_zone(&input.encode_utf16().collect::<Vec<_>>(), zone)
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

/// Lossless JavaScript `Date.parse` for UTF-16 strings in the process-local zone.
pub fn date_parse_utf16(input: &[u16]) -> f64 {
    date_parse_utf16_with_time_zone(input, &TimeZone::system())
}

/// Lossless UTF-16 parsing with explicit local-time resolution.
pub fn date_parse_utf16_with_time_zone(input: &[u16], zone: &TimeZone) -> f64 {
    let parsed = || {
        let p = parser::parse(input)?;
        let [hour, minute, second, millis] = p.time;
        finish_date(
            days_from_civil(p.year, p.month, p.day),
            hour,
            minute,
            second,
            millis,
            p.offset,
            zone,
        )
    };
    parsed().map_or(f64::NAN, |millis| millis as f64)
}

fn system_time_millis(time: SystemTime) -> Option<i64> {
    if let Ok(elapsed) = time.duration_since(UNIX_EPOCH) {
        return i64::try_from(elapsed.as_millis()).ok();
    }
    let elapsed = UNIX_EPOCH.duration_since(time).ok()?;
    i64::try_from(elapsed.as_millis()).ok().map(|millis| -millis)
}

fn finish_date(
    days: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
    offset_seconds: Option<i64>,
    zone: &TimeZone,
) -> Option<i64> {
    let wall = days
        .checked_mul(MS_PER_DAY)?
        .checked_add((hour * 3_600 + minute * 60 + second).checked_mul(1_000)?)?
        .checked_add(millis)?;
    match offset_seconds {
        Some(offset) => in_range(wall.checked_sub(offset.checked_mul(1_000)?)?),
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
    // Future POSIX transitions depend on weekday and leap year, not just month.
    // A 400-year Gregorian cycle preserves both. Ancient years retain LMT.
    let offset_year = if year >= 9999 {
        9200 + year.rem_euclid(400)
    } else {
        -9600 + year.rem_euclid(400)
    };
    let offset_datetime = clamped_datetime(offset_year, month, day, hour, minute, second, millis)?;
    let zoned = offset_datetime.to_zoned(zone.clone()).ok()?;
    let mapped_wall = days_from_civil(offset_year, month, day) * MS_PER_DAY
        + hour * 3_600_000
        + minute * 60_000
        + second * 1_000
        + millis;
    // Wall-minus-instant also retains compatible gap disambiguation, whereas
    // the returned zoned offset describes the time *after* crossing the gap.
    let offset_millis = mapped_wall.checked_sub(zoned.timestamp().as_millisecond())?;
    in_range(wall_millis.checked_sub(offset_millis)?)
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
