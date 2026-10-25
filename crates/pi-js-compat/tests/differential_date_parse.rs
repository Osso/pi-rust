//! Differential proof for `Date.parse` against Node under two time zones.

use crate::shared_oracle_2 as oracle;

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
    let environment = if zone_name == "inherited" {
        vec![]
    } else {
        vec![("TZ", OsString::from(zone_name))]
    };
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

#[test]
fn v8_reported_counterexamples_match_node() {
    let values = ["Jan 1 2020 EST", "2020.1.1", "Jan1 2020", "Jan 1 2020 (ignored)"];
    compare_values("UTC", &TimeZone::UTC, &values);
}

#[test]
fn v8_scanner_boundaries_and_bounded_combinations_match_node() {
    let mut values: Vec<String> = [
        "Jan 1 2020 EST",
        "2020.1.1",
        "Jan1 2020",
        "Jan 1 2020 (ignored)",
        "-000000",
        "-000000-01-01",
        "+000000",
        "+000000-01-01T00:00Z",
        "00000000000000002020",
        "123456789123456789",
        "00000000000000000001",
        "Jan 1 2020 00:00:60",
        "Jan 1 2020 00:00:99",
        "Jan 1 2020 00:00:100",
        "Jan 1 2020 GMT+99:99",
        "Jan 1 2020 GMT+9999",
        "Jan 1 2020 GMT+99999",
        "Jan 1 2020 GMT+1:",
        "Jan 1 2020 GMT+1:60",
        "Jan 1 2020 GMT+1:59",
        "Jan 1 2020 GMT+300000:",
        "Jan 1 2020 GMT+596523:",
        "Jan 1 2020 GMT+596524:",
        "Jan 1 2020 GMT+1193047:",
        "Jan 1 2020 GMT-1193047:59",
        "Jan 1 2020 GMT+999999999:",
        "Jan 1 2020 GMT+",
        "Jan 1 2020 GMT-0",
        "Jan 1 2020 UT",
        "Jan 1 2020 AM",
        "Jan 1 2020 12::",
        "Jan 1 2020 12:.34",
        "Jan 1 2020 12:34.5",
        "Jan 1 2020 12:34:56.00000000000000000001",
        "Jan 1 2020 12:34:56.12345678912345",
        "Jan 1 2020 (",
        "Jan 1 2020 ((nested))",
        "Jan 1 2020 )",
        "Jan 1 2020 +",
        "junk 2020/1/1",
        "junk2020/1/1",
        ") + - ! 2020/1/1",
        "Jan 1 2020 junk",
        "2020-01-01T00:00Z ",
        "2020-01-01T00:00:00.1",
        "2020-01-01T00:00.1",
        "2020-01-01T24:00:00.0000000000000000001Z",
        "2020-01-01T24:00:00.0000Z",
        "+275760-09-13T00:00:00Z",
        "+275760-09-13T00:00:00.001Z",
        "-271821-04-20T00:00:00Z",
        "-271821-04-19T23:59:59.999Z",
        "+275760-09-13T23:59:00+23:59",
        "-271821-04-19T00:01:00-23:59",
        "Jan 1 999999999 GMT+9999",
        "Jan 1 2147483647",
        "Jan 1 100000000000",
        "Jan\0 1 2020",
        "Jan 1 2020\0junk",
        "2020\0junk",
        "\u{a0}Jan 1 2020\u{feff}",
        "Jan\u{2028}1 2020",
        "Jan\u{2029}1 2020",
        "Jan\u{2003}1 2020",
        "Jan\u{85}1 2020",
        "Jan\u{180e}1 2020",
        "+010000-03-12T02:30",
        "+010000-11-05T01:30",
        "+275000-03-09T02:30",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for date in [
        "2020/1/1",
        "2020.1.1",
        "Jan1 2020",
        "1 Jan 20",
        "1-2-3",
        "2020-01-01",
        "Mar 8 2020",
        "Nov 1 2020",
    ] {
        for prefix in ["", "junk ", "(ignored)", ") "] {
            for suffix in [
                "",
                " EST",
                " GMT+1:30",
                " 02:30:00",
                " 01:30:00",
                " (ignored)",
                ")",
                " junk",
                " 12:00pm",
                " 00:00:60",
            ] {
                values.push(format!("{prefix}{date}{suffix}"));
            }
        }
    }
    let refs: Vec<_> = values.iter().map(String::as_str).collect();
    compare_values("inherited", &TimeZone::system(), &refs);
    compare_values("UTC", &TimeZone::UTC, &refs);
    compare_values("America/Chicago", &TimeZone::get("America/Chicago").unwrap(), &refs);
}

#[test]
fn v8_lossless_utf16_matches_node() {
    use pi_js_compat::date::{date_parse_utf16, date_parse_utf16_with_time_zone};
    let mut values: Vec<Vec<u16>> = Vec::new();
    for unit in [0xd800, 0xdc00, 0xffff, 0x2028, 0x2029, 0xfeff, 0] {
        for (prefix, suffix) in [
            ("", " Jan 1 2020"),
            ("Jan", " 1 2020"),
            ("Jan 1 2020 (", ")"),
            ("Jan 1 2020", "junk"),
        ] {
            values.push(
                prefix
                    .encode_utf16()
                    .chain([unit])
                    .chain(suffix.encode_utf16())
                    .collect(),
            );
        }
    }
    values.push("Jan 1 2020".encode_utf16().chain([0xd83d, 0xde00]).collect());
    for name in ["UTC", "America/Chicago", "inherited"] {
        let env = if name == "inherited" {
            vec![]
        } else {
            vec![("TZ", OsString::from(name))]
        };
        let zone = if name == "inherited" {
            TimeZone::system()
        } else {
            TimeZone::get(name).unwrap()
        };
        let mut oracle =
            oracle::TsOracle::spawn_with_environment("js-compat-oracle.ts", "packages/coding-agent/src/index.ts", &env);
        let response = oracle.request(&json!({"id":name,"operation":"date_parse","values":[],"utf16_values":values}));
        assert_eq!(response["ok"], true, "{response}");
        let expected = response["output"]["results"].as_array().unwrap();
        assert_eq!(expected.len(), values.len());
        for (units, expected) in values.iter().zip(expected) {
            let millis = if name == "inherited" {
                date_parse_utf16(units)
            } else {
                date_parse_utf16_with_time_zone(units, &zone)
            };
            let actual = (!millis.is_nan()).then_some(millis as i64);
            assert_eq!(actual, expected.as_i64(), "TZ={name} units={units:?}");
        }
    }
}
