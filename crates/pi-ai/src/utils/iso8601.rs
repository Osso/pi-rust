//! Minimal RFC 3339 `YYYY-MM-DDTHH:MM:SS(.sss)?Z` parsing.

/// Milliseconds since the Unix epoch.
pub fn parse_iso8601_millis(value: &str) -> Option<i64> {
    // `YYYY-MM-DDTHH:MM:SS(.sss)?Z`
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes.get(4) != Some(&b'-') || bytes.get(10) != Some(&b'T') || bytes.last() != Some(&b'Z') {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    let millis = if bytes.get(19) == Some(&b'.') {
        let fraction = value.get(20..value.len() - 1)?;
        let digits = fraction.len();
        let parsed = fraction.parse::<i64>().ok()?;
        if digits <= 3 {
            parsed * 10i64.pow(3 - digits as u32)
        } else {
            parsed / 10i64.pow(digits as u32 - 3)
        }
    } else {
        0
    };
    let days = days_from_civil(year, month as u32, day as u32);
    Some(((days * 86_400 + hour * 3600 + minute * 60 + second) * 1000) + millis)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
