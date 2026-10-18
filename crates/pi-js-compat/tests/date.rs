//! `Date.parse` parity for the ECMAScript ISO 8601 and HTTP-date forms.

use jiff::tz::TimeZone;
use pi_js_compat::date::{LocalDateFields, date_parse_with_time_zone, days_from_civil, local_date_fields_in};

fn utc(value: &str) -> f64 {
    date_parse_with_time_zone(value, &TimeZone::UTC)
}

fn chicago(value: &str) -> f64 {
    let zone = TimeZone::get("America/Chicago").expect("tzdb");
    date_parse_with_time_zone(value, &zone)
}

#[test]
fn parses_http_and_iso_dates_in_utc() {
    let cases: &[(&str, f64)] = &[
        ("Sun, 06 Nov 1994 08:49:37 GMT", 784_111_777_000.0),
        ("Sunday, 06-Nov-94 08:49:37 GMT", 784_111_777_000.0),
        ("Sun Nov  6 08:49:37 1994", 784_111_777_000.0),
        ("2020-01-01T00:00:00Z", 1_577_836_800_000.0),
        ("2020-01-01T00:00:00.123Z", 1_577_836_800_123.0),
        ("2020-01-01T00:00:00.123456789Z", 1_577_836_800_123.0),
        ("2020-01-01T00:00:00+02:00", 1_577_829_600_000.0),
        ("2020-01-01T00:00:00+0200", 1_577_829_600_000.0),
        ("2020-01-01T00:00:00-02:30", 1_577_845_800_000.0),
        ("2020-01-01T00:00Z", 1_577_836_800_000.0),
        ("2020-01-01 00:00:00Z", 1_577_836_800_000.0),
        ("2020-01-01t00:00:00z", 1_577_836_800_000.0),
        ("2020-01-01", 1_577_836_800_000.0),
        ("2020-01", 1_577_836_800_000.0),
        ("2020", 1_577_836_800_000.0),
        ("2020-1-1", 1_577_836_800_000.0),
        ("2020-01-1", 1_577_836_800_000.0),
        ("2020-001-01", 1_577_836_800_000.0),
        ("2020-01-001", 1_577_836_800_000.0),
        ("2020-02-30T00:00:00Z", 1_583_020_800_000.0),
        ("2020-02-31T00:00:00Z", 1_583_107_200_000.0),
        ("2020-04-31T00:00:00Z", 1_588_291_200_000.0),
        ("2020-01-01T24:00:00Z", 1_577_923_200_000.0),
        ("2020-01-01T24:00Z", 1_577_923_200_000.0),
        ("0000-01-01T00:00:00Z", -62_167_219_200_000.0),
        ("+002020-01-01T00:00:00Z", 1_577_836_800_000.0),
        ("-000001-01-01T00:00:00Z", -62_198_755_200_000.0),
        ("+275760-09-13T00:00:00Z", 8_640_000_000_000_000.0),
        ("-271821-04-20T00:00:00Z", -8_640_000_000_000_000.0),
    ];
    for (input, expected) in cases {
        let parsed = utc(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "Date.parse({input:?})");
    }
}

#[test]
fn rejects_values_outside_the_supported_grammar() {
    for input in [
        "",
        "not a date",
        "Jan 1 2020",
        "1/1/2020",
        "2020-13-01T00:00:00Z",
        "2020-00-01T00:00:00Z",
        "2020-01-00T00:00:00Z",
        "2020-01-32T00:00:00Z",
        "2020-01-01T00:60:00Z",
        "2020-01-01T23:59:60Z",
        "2020-01-01T24:00:01Z",
        "2020-01-01T24:00:00.5Z",
        "2020-01-01T1:00Z",
        "2020-01-01T01:0Z",
        "2020-01-01T00:00:00+24:00",
        "2020-01-01T00:00:00+00:60",
        "2020-01-01T00:00:00+02",
        "2020-01-01T00:00:00.Z",
        " 2020-01-01T00:00:00Z ",
        "+999999-01-01T00:00:00Z",
        "-999999-01-01T00:00:00Z",
        "+275760-09-13T00:00:00.001Z",
        "-271821-04-19T23:59:59.999Z",
        "2020-1-1T00:00:00Z",
        "2020-001-01T00:00:00Z",
    ] {
        assert!(utc(input).is_nan(), "Date.parse({input:?}) should be NaN");
    }
}

#[test]
fn resolves_zone_less_values_in_the_system_zone() {
    let cases: &[(&str, f64)] = &[
        ("2020-01-01T00:00:00", 1_577_858_400_000.0),
        ("2020-07-01T00:00:00", 1_593_579_600_000.0),
        ("2020-01-01", 1_577_836_800_000.0),
        ("2020-1-1", 1_577_858_400_000.0),
        ("2020-01-01T00:00", 1_577_858_400_000.0),
        ("Sun Nov  6 08:49:37 1994", 784_133_377_000.0),
        ("+010000-01-01T00:00:00", 253_402_322_400_000.0),
        ("-000001-01-01T00:00:00", -62_198_734_164_000.0),
        ("-271821-04-20T00:00:00", -8_639_999_978_964_000.0),
    ];
    for (input, expected) in cases {
        let parsed = chicago(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "Date.parse({input:?})");
    }
    assert!(chicago("+275760-09-13T00:00:00").is_nan());
}

#[test]
fn uses_compatible_disambiguation_for_dst_transitions() {
    let cases: &[(&str, f64)] = &[
        ("2020-03-08T01:59:59", 1_583_654_399_000.0),
        ("2020-03-08T02:00:00", 1_583_654_400_000.0),
        ("2020-03-08T02:30:00", 1_583_656_200_000.0),
        ("2020-03-08T03:00:00", 1_583_654_400_000.0),
        ("2020-11-01T01:00:00", 1_604_210_400_000.0),
        ("2020-11-01T01:30:00", 1_604_212_200_000.0),
        ("2020-11-01T02:00:00", 1_604_217_600_000.0),
    ];
    for (input, expected) in cases {
        let parsed = chicago(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "Date.parse({input:?})");
    }
}

#[test]
fn days_from_civil_matches_known_dates() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(1969, 12, 31), -1);
    assert_eq!(days_from_civil(2000, 2, 29), 11_016);
    assert_eq!(days_from_civil(2020, 1, 1), 18_262);
    assert_eq!(days_from_civil(0, 1, 1), -719_528);
}

#[test]
fn reads_local_date_fields_like_date_getters() {
    let zone = TimeZone::get("America/Chicago").expect("tzdb");
    // 2026-01-01T05:30:00Z is 2025-12-31 23:30 in Chicago (UTC-6).
    assert_eq!(
        local_date_fields_in(1_767_245_400_000, &zone),
        Some(LocalDateFields {
            year: 2025,
            month: 11,
            day: 31,
            hours: 23,
            minutes: 30,
        })
    );
    assert_eq!(
        local_date_fields_in(0, &TimeZone::UTC),
        Some(LocalDateFields {
            year: 1970,
            month: 0,
            day: 1,
            hours: 0,
            minutes: 0,
        })
    );
    assert_eq!(local_date_fields_in(8_640_000_000_000_001, &TimeZone::UTC), None);
}
