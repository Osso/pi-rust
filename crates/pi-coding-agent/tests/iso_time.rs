//! `Date#toISOString` formatting and the ECMAScript `Date.parse` subset used by
//! session-health and the debug/repl audit trail.

use pi_coding_agent::utils::iso_time::{format_iso_millis, parse_javascript_date};

/// Midnight local time on 2026-01-01 in the system zone.
fn local_midnight() -> i64 {
    parse_javascript_date("2026-01-01T00:00:00").expect("offset-less time parses")
}

#[test]
fn formats_epoch_milliseconds_like_to_iso_string() {
    let cases = [
        (0, "1970-01-01T00:00:00.000Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
        (1_767_225_600_000, "2026-01-01T00:00:00.000Z"),
        (951_782_400_000, "2000-02-29T00:00:00.000Z"),
        (-62_167_219_200_000, "0000-01-01T00:00:00.000Z"),
        (-62_198_755_200_000, "-000001-01-01T00:00:00.000Z"),
        (253_402_300_799_999, "9999-12-31T23:59:59.999Z"),
        (253_402_300_800_000, "+010000-01-01T00:00:00.000Z"),
    ];
    for (millis, expected) in cases {
        assert_eq!(format_iso_millis(millis), expected, "{millis}");
    }
}

#[test]
fn parses_the_iso_subset_and_round_trips_with_formatting() {
    let cases = [
        ("1970-01-01T00:00:00.000Z", 0),
        ("2026-01-01T00:00:00.000Z", 1_767_225_600_000),
        ("2026-01-01T00:00:00Z", 1_767_225_600_000),
        ("2026-01-01", 1_767_225_600_000),
        ("2026-01-01Z", 1_767_225_600_000),
        ("2026-01-01T00:00", local_midnight()),
        ("2026-01-01 00:00:00Z", 1_767_225_600_000),
        ("2026-01-01T02:00:00.000+02:00", 1_767_225_600_000),
        ("2025-12-31T19:00:00.000-05:00", 1_767_225_600_000),
        ("2026-01-01T00:00:00.000+0000", 1_767_225_600_000),
        ("2026-01-01T00:00:00-0530", 1_767_245_400_000),
        ("2026-02-30T00:00:00Z", 1_772_409_600_000),
        ("2026-02-31T00:00:00Z", 1_772_496_000_000),
        ("2026-01-01T24:00:00Z", 1_767_312_000_000),
        ("2026-01-01T00:00:00.123456789123Z", 1_767_225_600_123),
        ("2026-01-01t00:00:00z", 1_767_225_600_000),
        ("+002026-01-01T00:00:00Z", 1_767_225_600_000),
        ("-000001-01-01T00:00:00Z", -62_198_755_200_000),
    ];
    for (value, expected) in cases {
        assert_eq!(parse_javascript_date(value), Some(expected), "{value}");
    }
    for millis in [0, -1, 1_767_225_600_000, -62_167_219_200_000, 253_402_300_799_999] {
        assert_eq!(
            parse_javascript_date(&format_iso_millis(millis)),
            Some(millis),
            "round trip {millis}"
        );
    }
}

#[test]
fn rejects_values_outside_the_ecmascript_grammar() {
    let invalid = [
        "",
        "not a date",
        "2026-1-1T00:00:00Z",
        " 2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Zx",
        "2026-01-01T00:00:00.Z",
        "2026-01-01T00:00:00,5Z",
        "2026-01-01T00:00:00+2",
        "2026-01-01T00:00:00+24:00",
        "2026-01-01T00:00:00+00",
        "2026-01-01T25:00:00Z",
        "2026-01-01T00:60:00Z",
        "2026-01-01T24:00:00.001Z",
        "2026-01-32T00:00:00Z",
        "2026-01-00T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-01-01T",
        "2026-01-01T00:00:00+02:0",
        "2026-01-01T00:00:00+020:00",
    ];
    for value in invalid {
        assert_eq!(parse_javascript_date(value), None, "{value}");
    }
}

#[test]
fn resolves_offset_less_times_in_the_system_zone() {
    let system_offset_minutes = pi_coding_agent::utils::iso_time::parse_javascript_date("2026-01-01T00:00:00")
        .map(|local| (local - 1_767_225_600_000) / 60_000)
        .expect("offset-less time parses");
    let expected = -system_offset_minutes;
    assert!(
        (-14 * 60..=14 * 60).contains(&expected),
        "system offset {expected} minutes must be a real UTC offset"
    );
}
