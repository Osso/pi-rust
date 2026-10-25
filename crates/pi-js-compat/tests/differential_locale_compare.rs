//! Differential proof for `locale_compare` against Node's `Intl.Collator`.

use crate::shared_oracle_2 as oracle;

use std::cmp::Ordering;

use pi_js_compat::{compare_in_locale, host_locale, locale_compare};
use serde_json::{Value, json};

const PAIRS: &[[&str; 2]] = &[
    ["", ""],
    ["", "a"],
    ["a", ""],
    ["a", "a"],
    ["a", "A"],
    ["A", "a"],
    ["a", "b"],
    ["B", "a"],
    ["a", "á"],
    ["á", "a"],
    ["á", "Á"],
    ["ä", "å"],
    ["å", "ä"],
    ["ö", "ø"],
    ["ø", "œ"],
    ["ß", "ss"],
    ["ss", "ß"],
    ["straße", "strasse"],
    ["STRASSE", "straße"],
    ["é", "e\u{301}"],
    ["e\u{301}", "é"],
    ["ñ", "n"],
    ["ü", "u"],
    ["ł", "l"],
    ["æ", "ae"],
    ["Æ", "ae"],
    ["ı", "i"],
    ["I", "ı"],
    ["file1", "file2"],
    ["file2", "file10"],
    ["file02", "file2"],
    ["1", "2"],
    ["10", "2"],
    ["01", "1"],
    ["001", "01"],
    ["a-b", "ab"],
    ["ab", "a-b"],
    ["a_b", "a-b"],
    ["a.b", "a b"],
    ["a b", "a.b"],
    ["a/b", "a-b"],
    ["a+b", "a@b"],
    ["co-op", "coop"],
    ["co op", "co-op"],
    ["Co-Op", "co-op"],
    ["Ångström", "Angstrom"],
    ["ängström", "Ångström"],
    ["中文", "日本語"],
    ["日本語", "한국어"],
    ["한국어", "中文"],
    ["русский", "ελληνικά"],
    ["ελληνικά", "عربي"],
    ["عربي", "עברית"],
    ["😀", "😁"],
    ["😁", "🎉"],
    ["a😀", "a😁"],
    ["Z", "z"],
    ["z", "Z"],
    ["abc", "ABC"],
    ["ABC", "abc"],
    ["a", "A "],
    ["a ", "a"],
];

fn sign(ordering: Ordering) -> i64 {
    match ordering {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn node_results(oracle: &mut oracle::TsOracle, request_id: &str, locale: Option<&str>) -> Vec<i64> {
    let pairs: Vec<Value> = PAIRS.iter().map(|[left, right]| json!([left, right])).collect();
    let response = oracle.request(&json!({
        "id": request_id,
        "operation": "locale_compare",
        "locale": locale,
        "pairs": pairs,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    response["output"]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|value| value.as_i64().expect("sign"))
        .collect()
}

#[test]
fn matches_node_intl_collator_for_explicit_locales() {
    let mut ts = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    for locale in ["en-US", "und", "sv", "de", "tr", "ja", "zh"] {
        let expected = node_results(&mut ts, locale, Some(locale));
        let actual: Vec<i64> = PAIRS
            .iter()
            .map(|[left, right]| sign(compare_in_locale(left, right, locale)))
            .collect();
        assert_eq!(actual, expected, "locale {locale}");
    }
}

#[test]
fn matches_node_default_locale_compare() {
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "js-compat-oracle.ts",
        "packages/coding-agent/src/index.ts",
        &[("LANG", "en_US.UTF-8".into()), ("LC_ALL", "en_US.UTF-8".into())],
    );
    let expected = node_results(&mut ts, "default", None);
    let actual: Vec<i64> = PAIRS
        .iter()
        .map(|[left, right]| sign(compare_in_locale(left, right, "en-US")))
        .collect();
    assert_eq!(actual, expected, "a.localeCompare(b)");
}

#[test]
fn host_locale_compare_uses_the_resolved_environment_locale() {
    let locale = host_locale();
    for [left, right] in PAIRS {
        assert_eq!(
            locale_compare(left, right),
            compare_in_locale(left, right, &locale),
            "{left:?} vs {right:?} in {locale}"
        );
    }
}
