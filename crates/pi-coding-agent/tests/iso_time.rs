//! `Date#toISOString` formatting.

use pi_coding_agent::utils::iso_time::format_iso_millis;

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
fn round_trips_iso_strings_through_the_shared_date_parser() {
    for millis in [0, -1, 1_767_225_600_000, -62_167_219_200_000, 253_402_300_799_999] {
        let formatted = format_iso_millis(millis);
        let parsed = pi_js_compat::date::date_parse(&formatted);
        assert_eq!(parsed as i64, millis, "round trip {millis}");
    }
}
