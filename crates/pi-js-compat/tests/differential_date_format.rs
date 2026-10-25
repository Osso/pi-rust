use crate::shared_oracle_2 as oracle;
use jiff::tz::TimeZone;
use pi_js_compat::date::{
    LocaleDateFormat, days_from_civil, local_date_fields_in, time_clip_millis, to_locale_string, to_locale_string_in,
};
use serde_json::{Value, json};
use std::ffi::OsString;

fn values() -> Vec<f64> {
    let mut values = vec![
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        -0.5,
        0.5,
        -8_640_000_000_000_001.0,
        -8_640_000_000_000_000.0,
        -8_639_999_999_999_999.0,
        8_639_999_999_999_999.0,
        8_640_000_000_000_000.0,
        8_640_000_000_000_001.0,
    ];
    for year in [
        -271_820, -100_000, -10_000, -9_999, -9_998, -1, 0, 1, 1582, 1800, 1883, 1900, 1970, 2026, 9_998, 9_999,
        10_000, 100_000, 275_759,
    ] {
        for (month, day) in [
            (1, 1),
            (2, 28),
            (3, 1),
            (3, 8),
            (4, 5),
            (6, 1),
            (10, 25),
            (11, 1),
            (12, 31),
        ] {
            for within in [0, 1, 3_599_999, 43_210_987, 86_399_999] {
                let millis = (days_from_civil(year, month, day) * 86_400_000 + within) as f64;
                if millis.abs() <= 8_640_000_000_000_000.0 {
                    values.push(millis);
                }
            }
        }
    }
    let mut seed = 0x91e1_0da5_c79e_7b1du64;
    for _ in 0..1024 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        values.push((seed % 17_280_000_000_000_001) as f64 - 8_640_000_000_000_000.0);
    }
    values
}
fn wire(values: &[f64]) -> Value {
    json!({"values":values.iter().map(|value|pi_js_compat::number::js_number_to_string(*value)).collect::<Vec<_>>()})
}
#[test]
fn local_calendar_fields_cover_the_full_js_date_range() {
    let values = values();
    let mut failures = Vec::new();
    for name in [
        "UTC",
        "America/Chicago",
        "Europe/Paris",
        "Asia/Kolkata",
        "Australia/Lord_Howe",
        "Pacific/Apia",
        "Africa/Monrovia",
    ] {
        let zone = TimeZone::get(name).unwrap();
        let mut oracle = oracle::TsOracle::spawn_with_environment(
            "date-format-oracle.mjs",
            "packages/tui/src",
            &[("TZ", OsString::from(name))],
        );
        let expected = oracle.request(&wire(&values));
        for (value, expected) in values.iter().zip(expected.as_array().unwrap()) {
            let fields = time_clip_millis(*value).and_then(|millis| local_date_fields_in(millis, &zone));
            let actual = fields
                .map(|fields| json!([fields.year, fields.month, fields.day, fields.hours, fields.minutes]))
                .unwrap_or(Value::Null);
            if actual != expected["fields"] {
                if failures.len() < 5 {
                    eprintln!(
                        "zone {name} at {value}: native {actual} / source {}",
                        expected["fields"]
                    );
                }
                failures.push((name, *value));
            }
            for (format, field) in [
                (LocaleDateFormat::DateTimeEnUs, "full"),
                (LocaleDateFormat::TimeEnUs, "timeUs"),
                (LocaleDateFormat::TimeEnGb, "time"),
            ] {
                let actual = to_locale_string_in(*value, format, &zone);
                if actual != expected[field] {
                    if failures.len() < 5 {
                        eprintln!(
                            "zone {name} at {value} {format:?}: native {actual} / source {}",
                            expected[field]
                        );
                    }
                    failures.push((name, *value));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} calendar values differ", failures.len());
}
#[test]
fn inherited_locale_display_covers_the_full_js_date_range() {
    let values = values();
    let mut oracle = oracle::TsOracle::spawn("date-format-oracle.mjs", "packages/tui/src");
    let expected = oracle.request(&wire(&values));
    let mut failures = Vec::new();
    for (value, expected) in values.iter().zip(expected.as_array().unwrap()) {
        let actual = to_locale_string(*value, LocaleDateFormat::DateTimeEnUs);
        if actual != expected["full"] {
            if failures.len() < 5 {
                eprintln!("at {value}: native {actual} / source {}", expected["full"]);
            }
            failures.push(*value);
        }
    }
    assert!(failures.is_empty(), "{} locale values differ", failures.len());
}
