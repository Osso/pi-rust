//! Differential proof for `Date.parse` against Node under two time zones.

#[path = "support/oracle.rs"]
mod oracle;

use std::ffi::OsString;

use jiff::tz::TimeZone;
use pi_js_compat::date::date_parse_with_time_zone;
use serde_json::json;

const VALUES: &[&str] = &[
    // ECMAScript ISO 8601, explicit zones.
    "2020-01-01T00:00:00Z",
    "2020-01-01T00:00:00.123Z",
    "2020-01-01T00:00:00.123456789Z",
    "2020-01-01T00:00:00+02:00",
    "2020-01-01T00:00:00+0200",
    "2020-01-01T00:00:00-02:30",
    "2020-01-01T00:00Z",
    "2020-01-01 00:00:00Z",
    "2020-01-01  00:00:00Z",
    "2020-01-01\t00:00:00Z",
    "2020-01-01\n00:00:00Z",
    "2020-01-01t00:00:00z",
    "2020-01-01 00:00:00 +01:00",
    "2020-01-01T00:00:00+23:59",
    "2020-01-01T00:00:00-23:59",
    "2020-01-01T24:00:00Z",
    "2020-01-01T24:00Z",
    "0000-01-01T00:00:00Z",
    "+002020-01-01T00:00:00Z",
    "-000001-01-01T00:00:00Z",
    "+275760-09-13T00:00:00Z",
    "-271821-04-20T00:00:00Z",
    // Date-only and year/month forms (strict UTC, legacy local).
    "2020-01-01",
    "2020-01",
    "2020",
    "2020-1-1",
    "2020-01-1",
    "2020-1-01",
    "2020-001-01",
    "2020-01-001",
    "2020-0001-01",
    "2020-01-0001",
    "2020-1-1Z",
    "2020-01Z",
    "2020Z",
    "+002020-01-01",
    "+002020-1-1",
    "2020-02-28",
    "2020-02-29",
    "2020-02-30",
    "2020-02-31",
    "2021-02-28",
    "2021-02-29",
    "2021-02-30",
    "2021-02-31",
    "2020-04-30",
    "2020-04-31",
    "2020-06-31",
    // Zone-less date-times.
    "2020-01-01T00:00:00",
    "2020-01-01T00:00",
    "2020-07-01T00:00:00",
    "2020-03-08T01:59:59",
    "2020-03-08T02:00:00",
    "2020-03-08T02:30:00",
    "2020-03-08T03:00:00",
    "2020-11-01T01:00:00",
    "2020-11-01T01:30:00",
    "2020-11-01T02:00:00",
    "2020-01-01T00:00:00.5",
    "2020-01-01T00:00:00.0005",
    "2020-01-01T24:00:00",
    "1960-01-01T00:00:00",
    "1900-01-01T00:00:00",
    "1800-01-01T00:00:00",
    "9999-12-31T23:59:59",
    "0001-01-01T00:00:00",
    "+010000-01-01T00:00:00",
    "-000001-01-01T00:00:00",
    "-271821-04-20T00:00:00",
    "+275760-09-13T00:00:00",
    // HTTP dates (asctime is local in V8).
    "Sun, 06 Nov 1994 08:49:37 GMT",
    "Sunday, 06-Nov-94 08:49:37 GMT",
    "Sun Nov  6 08:49:37 1994",
    // Invalid.
    "",
    "not a date",
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
    "2020-01-01T",
    "2020-01-01T00",
];

fn compare(zone_name: &str, time_zone: &TimeZone) {
    let environment = [("TZ", OsString::from(zone_name))];
    let pairs: Vec<(&str, OsString)> = environment.iter().map(|(name, value)| (*name, value.clone())).collect();
    let mut oracle =
        oracle::TsOracle::spawn_with_environment("js-compat-oracle.ts", "packages/coding-agent/src/index.ts", &pairs);
    let response = oracle.request(&json!({
        "id": zone_name,
        "operation": "date_parse",
        "values": VALUES,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Option<i64>> = response["output"]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|value| value.as_i64())
        .collect();
    let actual: Vec<Option<i64>> = VALUES
        .iter()
        .map(|value| {
            let parsed = date_parse_with_time_zone(value, time_zone);
            (!parsed.is_nan()).then_some(parsed as i64)
        })
        .collect();
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "TZ={zone_name}: Date.parse({:?})", VALUES[index]);
    }
}

#[test]
fn matches_node_date_parse_in_utc() {
    compare("UTC", &TimeZone::UTC);
}

#[test]
fn matches_node_date_parse_in_america_chicago() {
    let zone = TimeZone::get("America/Chicago").expect("tzdb");
    compare("America/Chicago", &zone);
}

#[test]
fn matches_node_date_parse_in_the_inherited_zone() {
    use pi_js_compat::date::date_parse;

    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let response = oracle.request(&json!({
        "id": "inherited",
        "operation": "date_parse",
        "values": VALUES,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Option<i64>> = response["output"]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|value| value.as_i64())
        .collect();
    let actual: Vec<Option<i64>> = VALUES
        .iter()
        .map(|value| {
            let parsed = date_parse(value);
            (!parsed.is_nan()).then_some(parsed as i64)
        })
        .collect();
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "Date.parse({:?})", VALUES[index]);
    }
}
