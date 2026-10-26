//! Reminder date/time boundaries from the locked reminders delta.
use crate::date::{days_from_civil, time_clip_millis, to_iso_string, utc_date_time};
use jiff::{Timestamp, tz::TimeZone};

const GUIDANCE: &str = "Use YYYY-MM-DDTHH:mm[:ss] or YYYY-MM-DD HH:mm[:ss] for local time; add Z or an explicit +/-HH:MM timezone offset to the T form to select an exact instant.";
const DAY: i64 = 86_400_000;

mod node_time_zones {
    include!("reminder_time_zones.rs");
}

fn node_timezone_name(zone: &TimeZone) -> Result<&str, String> {
    let name = zone
        .iana_name()
        .ok_or_else(|| "Reminder timezone has no IANA name.".to_owned())?;
    let index = node_time_zones::NODE_TIME_ZONES
        .binary_search_by_key(&name, |(id, _)| *id)
        .map_err(|_| format!("Reminder timezone {name} is absent from the pinned Node timezone table."))?;
    Ok(node_time_zones::NODE_TIME_ZONES[index].1)
}

fn error(message: &str) -> String {
    format!("{message} {GUIDANCE}")
}

fn digits(bytes: &[u8]) -> Option<i64> {
    bytes
        .iter()
        .try_fold(0, |n, b| b.is_ascii_digit().then_some(n * 10 + i64::from(b - b'0')))
}

fn parse_wall(input: &str) -> Result<(i64, Option<i64>), String> {
    let b = input.as_bytes();
    let invalid = || error("Invalid reminder due time.");
    if b.len() < 16 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') || b[13] != b':' {
        return Err(invalid());
    }
    let year = digits(&b[0..4]).ok_or_else(invalid)?;
    let month = digits(&b[5..7]).ok_or_else(invalid)?;
    let day = digits(&b[8..10]).ok_or_else(invalid)?;
    let hour = digits(&b[11..13]).ok_or_else(invalid)?;
    let minute = digits(&b[14..16]).ok_or_else(invalid)?;
    let mut pos = 16;
    let mut second = 0;
    let mut ms = 0;
    let mut fraction = false;
    if b.get(pos) == Some(&b':') {
        if b.len() < pos + 3 {
            return Err(invalid());
        }
        second = digits(&b[pos + 1..pos + 3]).ok_or_else(invalid)?;
        pos += 3;
        if b.get(pos) == Some(&b'.') {
            fraction = true;
            pos += 1;
            let start = pos;
            while b.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            let len = pos - start;
            if !(1..=3).contains(&len) {
                return Err(invalid());
            }
            ms = digits(&b[start..pos]).ok_or_else(invalid)? * 10_i64.pow((3 - len) as u32);
        }
    }
    let zone = match &b[pos..] {
        [] => None,
        [b'Z'] => Some(0),
        [sign @ (b'+' | b'-'), a, c, b':', d, e] if [a, c, d, e].iter().all(|x| x.is_ascii_digit()) => {
            let hours = i64::from((a - b'0') * 10 + (c - b'0'));
            let minutes = i64::from((d - b'0') * 10 + (e - b'0'));
            Some((hours * 60 + minutes) * 60_000 * if *sign == b'+' { 1 } else { -1 })
        }
        _ => return Err(invalid()),
    };
    if (zone.is_some() && b[10] != b'T') || (fraction && zone.is_none()) {
        return Err(error("Invalid reminder due time format."));
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return Err(error("Invalid calendar date or clock time."));
    }
    let wall = days_from_civil(year, month, day) * DAY + hour * 3_600_000 + minute * 60_000 + second * 1000 + ms;
    let fields = utc_date_time(wall);
    if fields.year != year || fields.month != month || fields.day != day {
        return Err(error("Invalid calendar date or clock time."));
    }
    if let Some(offset) = zone
        && (offset.abs() / 60_000 / 60 > 23 || offset.abs() / 60_000 % 60 > 59)
    {
        return Err(error("Invalid timezone offset."));
    }
    // Check the original minutes independently: normalization would hide :60.
    if b.len() >= 6 && matches!(b[b.len() - 6], b'+' | b'-') && digits(&b[b.len() - 2..]).unwrap_or(0) > 59 {
        return Err(error("Invalid timezone offset."));
    }
    Ok((wall, zone))
}

fn offset_at(millis: i64, zone: &TimeZone) -> i64 {
    let timestamp = Timestamp::from_millisecond(millis).expect("four-digit reminder date fits jiff");
    i64::from(zone.to_offset(timestamp).seconds()) * 1000
}

/// Parse a reminder using the current machine timezone.
pub fn parse_reminder_due_at(input: &str) -> Result<String, String> {
    parse_reminder_due_at_in(input, &TimeZone::system())
}

/// Parse a reminder using an explicit timezone.
pub fn parse_reminder_due_at_in(input: &str, zone: &TimeZone) -> Result<String, String> {
    let (wall, offset) = parse_wall(input)?;
    if let Some(offset) = offset {
        return Ok(to_iso_string(wall - offset));
    }
    let mut offsets = Vec::new();
    for day in -2..=2 {
        let offset = offset_at(wall + day * DAY, zone);
        if !offsets.contains(&offset) {
            offsets.push(offset);
        }
    }
    let candidates: Vec<_> = offsets
        .into_iter()
        .map(|offset| wall - offset)
        .filter(|&candidate| candidate + offset_at(candidate, zone) == wall)
        .collect();
    match candidates.as_slice() {
        [] => Err(error(
            "This local time is nonexistent because the timezone clock moves forward.",
        )),
        [instant] => Ok(to_iso_string(*instant)),
        _ => Err(error(
            "This local time is ambiguous because the timezone clock moves backward.",
        )),
    }
}

fn read_absolute(input: &str, zone: &TimeZone) -> Result<i64, String> {
    let b = input.as_bytes();
    if !input.ends_with('Z')
        && !(b.len() >= 6
            && matches!(b[b.len() - 6], b'+' | b'-')
            && b[b.len() - 3] == b':'
            && digits(&b[b.len() - 5..b.len() - 3]).is_some()
            && digits(&b[b.len() - 2..]).is_some())
    {
        return Err(error("Expected an absolute reminder due time."));
    }
    let (wall, offset) = parse_wall(input)?;
    let _ = zone;
    Ok(wall - offset.expect("absolute suffix validated"))
}

/// Format an absolute reminder in the current machine timezone.
pub fn format_reminder_due_at(input: &str) -> Result<String, String> {
    format_reminder_due_at_in(input, &TimeZone::system())
}

/// Format an absolute reminder in the supplied timezone.
pub fn format_reminder_due_at_in(input: &str, zone: &TimeZone) -> Result<String, String> {
    let millis = read_absolute(input, zone)?;
    let offset = offset_at(millis, zone);
    let fields = utc_date_time(millis + offset);
    let label = if offset == 0 {
        "GMT+00:00".to_owned()
    } else {
        let seconds = offset.abs() / 1000;
        if seconds % 60 == 0 {
            format!(
                "GMT{}{:02}:{:02}",
                if offset < 0 { '-' } else { '+' },
                seconds / 3600,
                seconds / 60 % 60
            )
        } else {
            format!(
                "GMT{}{:02}:{:02}:{:02}",
                if offset < 0 { '-' } else { '+' },
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        }
    };
    let name = node_timezone_name(zone)?;
    let year = if fields.year <= 0 { 1 - fields.year } else { fields.year };
    Ok(format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}:{:02} {label} [{name}]",
        fields.month, fields.day, fields.hour, fields.minute, fields.second
    ))
}

/// Compare against the end of the current local day; `now` is JS epoch milliseconds.
pub fn is_reminder_due_today(input: &str, now: f64) -> Result<bool, String> {
    is_reminder_due_today_in(input, now, &TimeZone::system())
}

/// Compare against the end of the current day in an explicit timezone.
pub fn is_reminder_due_today_in(input: &str, now: f64, zone: &TimeZone) -> Result<bool, String> {
    let now = time_clip_millis(now).ok_or_else(|| "Invalid now date for reminder due-today comparison.".to_owned())?;
    let due = read_absolute(input, zone)?;
    let local = now + offset_at(now, zone);
    let tomorrow = (local.div_euclid(DAY) + 1) * DAY;
    let fields = utc_date_time(tomorrow);
    let date = jiff::civil::date(fields.year as i16, fields.month as i8, fields.day as i8).at(0, 0, 0, 0);
    let end = date
        .to_zoned(zone.clone())
        .map_err(|e| e.to_string())?
        .timestamp()
        .as_millisecond();
    Ok(due < end)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cases_match_locked_typescript() {
        use serde_json::{Value, json};
        use std::process::Command;
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        for name in [
            "UTC",
            "America/Chicago",
            "Australia/Lord_Howe",
            "Pacific/Apia",
            "US/Central",
        ] {
            let output = Command::new("/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/node_modules/.bin/tsx")
                .arg(format!("{root}/oracle/reminder-time-oracle.ts"))
                .env("TZ", name)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
            let zone = TimeZone::get(name).unwrap();
            for case in expected {
                let input = case["input"].as_str().unwrap();
                let actual = match case["op"].as_str().unwrap() {
                    "parse" => parse_reminder_due_at_in(input, &zone).map(|v| json!(v)),
                    "format" => format_reminder_due_at_in(input, &zone).map(|v| json!(v)),
                    "due" => is_reminder_due_today_in(input, case["now"].as_f64().unwrap(), &zone).map(|v| json!(v)),
                    _ => unreachable!(),
                };
                let actual = match actual {
                    Ok(v) => json!({"value":v}),
                    Err(e) => json!({"error":e}),
                };
                assert_eq!(actual, case["result"], "{name} {case}");
            }
        }
    }
    #[test]
    fn timezone_names_match_node_for_all_supported_tzdata_ids() {
        use serde_json::Value;
        use std::process::Command;
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let output = Command::new("node")
            .arg(format!("{root}/oracle/reminder-time-zones.mjs"))
            .arg("--json")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(reference["icu"], "78.3");
        assert_eq!(reference["tz"], "2026a");
        assert_eq!(reference["tzdata"], "2026e");
        let entries = reference["entries"].as_array().unwrap();
        assert_eq!(entries.len(), node_time_zones::NODE_TIME_ZONES.len());
        for (entry, &(table_id, table_name)) in entries.iter().zip(node_time_zones::NODE_TIME_ZONES) {
            let id = entry[0].as_str().unwrap();
            let expected = entry[1].as_str().unwrap();
            assert_eq!((table_id, table_name), (id, expected));
            let zone = TimeZone::get(id).unwrap_or_else(|error| panic!("{id}: {error}"));
            let formatted = format_reminder_due_at_in("2026-07-21T19:30:12Z", &zone).unwrap();
            assert!(formatted.ends_with(&format!("[{expected}]")), "{id}: {formatted}");
        }
    }

    #[test]
    fn rejects_invalid_clock_before_due_time() {
        let zone = TimeZone::get("UTC").unwrap();
        assert_eq!(
            is_reminder_due_today_in("bad", f64::NAN, &zone).unwrap_err(),
            "Invalid now date for reminder due-today comparison."
        );
    }

    #[test]
    fn parses_chicago_local_time() {
        let zone = TimeZone::get("America/Chicago").unwrap();
        assert_eq!(
            parse_reminder_due_at_in("2026-07-21T14:30", &zone).unwrap(),
            "2026-07-21T19:30:00.000Z"
        );
    }
}
