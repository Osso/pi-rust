//! ISO-8601 timestamps: `Date#toISOString` and `new Date().toISOString()`.

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
