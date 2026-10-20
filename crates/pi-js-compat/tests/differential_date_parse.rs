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

fn compare_values(zone_name: &str, time_zone: &TimeZone, values: &[&str]) {
    let environment = [("TZ", OsString::from(zone_name))];
    let pairs: Vec<(&str, OsString)> = environment.iter().map(|(name, value)| (*name, value.clone())).collect();
    let mut oracle =
        oracle::TsOracle::spawn_with_environment("js-compat-oracle.ts", "packages/coding-agent/src/index.ts", &pairs);
    let response = oracle.request(&json!({
        "id": zone_name,
        "operation": "date_parse",
        "values": values,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Option<i64>> = response["output"]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|value| value.as_i64())
        .collect();
    let actual: Vec<Option<i64>> = values
        .iter()
        .map(|value| {
            let parsed = date_parse_with_time_zone(value, time_zone);
            (!parsed.is_nan()).then_some(parsed as i64)
        })
        .collect();
    let mismatches: Vec<_> = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .map(|(index, (actual, expected))| {
            format!(
                "TZ={zone_name}: Date.parse({:?}): Rust {actual:?}, Node {expected:?}",
                values[index]
            )
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn matches_node_date_parse_in_utc() {
    compare_values("UTC", &TimeZone::UTC, VALUES);
}

#[test]
fn matches_node_date_parse_in_america_chicago() {
    let zone = TimeZone::get("America/Chicago").expect("tzdb");
    compare_values("America/Chicago", &zone, VALUES);
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

#[test]
fn numeric_dates_match_node_in_utc_and_chicago() {
    let mut values: Vec<String> = (0..=100).map(|n| n.to_string()).collect();
    values.extend(
        [
            "000", "001", "009", "012", "013", "049", "050", "099", "999", "1000", "9999", "10000", "99999", "275760",
            "275761", "999999", "00000", "00001", "00049", "+0", "-0", "+1", "-1", " 0 ", "0Z", "0z", "-000001",
            "+000001", "1x",
        ]
        .map(str::to_owned),
    );
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    for (name, zone) in [
        ("UTC", TimeZone::UTC),
        ("America/Chicago", TimeZone::get("America/Chicago").unwrap()),
    ] {
        compare_values(name, &zone, &refs);
    }
}

#[test]
fn legacy_month_slash_and_pre_epoch_http_dates_match_node_across_zones() {
    let values = [
        "Jan 1 2020",
        "January 1, 2020",
        "1 Jan 2020",
        "2020 Jan 1",
        "jan 1 20",
        "1/1/2020",
        "01/02/2020",
        "1/2/20",
        "12/31/49",
        "12/31/50",
        "1960/1/1",
        "99/1/1",
        "20/1/1",
        "13/1/2020",
        "0/1/2020",
        "1/0/2020",
        "1/32/2020",
        "2/30/2020",
        "2/29/2021",
        "4/31/2020",
        "Foo 1 2020",
        "Jan 0 2020",
        "Jan 32 2020",
        "Jan 1 invalid",
        "Jan 1 2020 extra",
        "1/1/2020 extra",
        "Jan 1 2020 00:00:00",
        "1/1/2020 00:00:00",
        "Mar 8 2020 02:30:00",
        "Nov 1 2020 01:30:00",
        "Jan 1 1960 00:00:00 GMT",
        "Jan 1 1960 00:00:00 UTC",
        "Jan 1 1960 00:00:00 +0200",
        "Jan 1 1960 00:00:00 -0600",
        "Fri, 01 Jan 1960 00:00:00 GMT",
        "Friday, 01-Jan-60 00:00:00 GMT",
        "Fri Jan  1 00:00:00 1960",
        "Mon, 01 Jan 1900 00:00:00 GMT",
        "Wed, 31 Dec 1969 23:59:59 GMT",
        "Thu, 01 Jan 1970 00:00:00 GMT",
        "Mon, 01 Jan 1960 00:00:00 GMT",
        "Fri, 00 Jan 1960 00:00:00 GMT",
        "Fri, 32 Jan 1960 00:00:00 GMT",
        "Fri, 01 Foo 1960 00:00:00 GMT",
        "Fri, 01 Jan 1960 24:00:01 GMT",
        "Fri, 01 Jan 1960 00:60:00 GMT",
        "Fri, 01 Jan 1960 00:00:60 GMT",
        "Thu, 01 Jan 0001 00:00:00 GMT",
        "Fri, 01 Jan 1960 00:00:59 GMT",
        "Fri, 01 Jan 1960 00:00:61 GMT",
        "Fri, 01 Jan 1960 00:00:99 GMT",
        "Jan 1 1960 00:00:60",
        "1/1/1960 00:00:60",
        "1960-01-01T00:00:60Z",
    ];
    compare_values("UTC", &TimeZone::UTC, &values);
    compare_values("America/Chicago", &TimeZone::get("America/Chicago").unwrap(), &values);
}
