use crate::shared_oracle as oracle;

use pi_tui::fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

struct MatchCase {
    query: &'static str,
    text: &'static str,
}

const MATCH_CASES: &[MatchCase] = &[
    MatchCase {
        query: "",
        text: "anything",
    },
    MatchCase {
        query: "longquery",
        text: "short",
    },
    MatchCase {
        query: "test",
        text: "test",
    },
    MatchCase {
        query: "abc",
        text: "aXbXc",
    },
    MatchCase {
        query: "abc",
        text: "cba",
    },
    MatchCase {
        query: "ABC",
        text: "abc",
    },
    MatchCase {
        query: "abc",
        text: "ABC",
    },
    MatchCase {
        query: "foo",
        text: "foobar",
    },
    MatchCase {
        query: "foo",
        text: "f_o_o_bar",
    },
    MatchCase {
        query: "fb",
        text: "foo-bar",
    },
    MatchCase {
        query: "fb",
        text: "afbx",
    },
    MatchCase {
        query: "codex52",
        text: "gpt-5.2-codex",
    },
    MatchCase {
        query: "cl",
        text: "clone",
    },
    MatchCase {
        query: "app",
        text: "application",
    },
    MatchCase { query: "a", text: "ä" },
    MatchCase {
        query: "Ä", text: "ä"
    },
    MatchCase {
        query: "İ", text: "i̇"
    },
    MatchCase {
        query: "😀",
        text: "a😀b",
    },
    MatchCase {
        query: "strasse",
        text: "straße",
    },
    MatchCase {
        query: "SS", text: "ß"
    },
    MatchCase {
        query: "openai/gpt",
        text: "openai-codex gpt-5.5",
    },
    MatchCase {
        query: "gpt-5.5",
        text: "openai-codex/gpt-5.4",
    },
    MatchCase {
        query: "a/b",
        text: "a b",
    },
    MatchCase {
        query: "  spaced  ",
        text: "spaced",
    },
    MatchCase {
        query: "x\u{00a0}y",
        text: "x y",
    },
    MatchCase {
        query: "1abc",
        text: "abc1",
    },
    MatchCase {
        query: "abc1",
        text: "1abc",
    },
    MatchCase {
        query: "a1b",
        text: "a1b",
    },
    MatchCase {
        query: "日本",
        text: "日本語",
    },
];

fn rust_match(case: &MatchCase) -> FuzzyMatch {
    fuzzy_match(case.query, case.text)
}

#[test]
fn fuzzy_match_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases: Vec<Value> = MATCH_CASES
        .iter()
        .map(|case| json!({ "query": case.query, "text": case.text }))
        .collect();
    let response = oracle.request(&json!({
        "id": "fuzzy-match",
        "operation": "fuzzy_match",
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), MATCH_CASES.len());
    for (case, expected) in MATCH_CASES.iter().zip(results) {
        let actual = rust_match(case);
        assert_eq!(
            actual.matches,
            expected["matches"].as_bool().unwrap(),
            "match mismatch for {:?} / {:?}",
            case.query,
            case.text
        );
        let expected_score = expected["scoreBits"].as_str().unwrap();
        let actual_score = format!("{:016x}", actual.score.to_bits());
        assert_eq!(
            actual_score, expected_score,
            "score mismatch for {:?} / {:?}",
            case.query, case.text
        );
    }
}

struct FilterCase {
    query: &'static str,
    items: &'static [&'static [&'static str]],
}

const FILTER_CASES: &[FilterCase] = &[
    FilterCase {
        query: "",
        items: &[&["apple"], &["banana"], &["cherry"]],
    },
    FilterCase {
        query: "an",
        items: &[&["apple"], &["banana"], &["cherry"]],
    },
    FilterCase {
        query: "app",
        items: &[&["a_p_p"], &["app"], &["application"]],
    },
    FilterCase {
        query: "cl",
        items: &[&["clone"], &["cl"]],
    },
    FilterCase {
        query: "gpt-5.5",
        items: &[&["gpt-5.4", "openai-codex/gpt-5.4", "openai-codex gpt-5.4"]],
    },
    FilterCase {
        query: "openai/gpt",
        items: &[&["gpt-5.4", "openai-codex/gpt-5.4", "openai-codex gpt-5.4"]],
    },
    FilterCase {
        query: "openai-codex/gpt-5.5",
        items: &[&["gpt-5.5", "openai-codex gpt-5.5"]],
    },
    FilterCase {
        query: "foo",
        items: &[&["foo", "1"], &["bar", "2"], &["foobar", "3"]],
    },
    FilterCase {
        query: "  ",
        items: &[&["a"], &["b"]],
    },
    FilterCase {
        query: "/",
        items: &[&["a"], &["b"]],
    },
    FilterCase {
        query: "test",
        items: &[&["test"], &["TEST"], &["t_e_s_t"], &["testing"]],
    },
];

fn rust_filter(case: &FilterCase) -> Vec<Vec<String>> {
    let items: Vec<Vec<String>> = case
        .items
        .iter()
        .map(|candidates| candidates.iter().map(|value| value.to_string()).collect())
        .collect();
    fuzzy_filter(&items, case.query, |candidates| candidates.clone())
}

#[test]
fn fuzzy_filter_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);
    let cases: Vec<Value> = FILTER_CASES
        .iter()
        .map(|case| {
            json!({
                "query": case.query,
                "items": case.items.iter().map(|candidates| candidates.to_vec()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let response = oracle.request(&json!({
        "id": "fuzzy-filter",
        "operation": "fuzzy_filter",
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), FILTER_CASES.len());
    for (case, expected) in FILTER_CASES.iter().zip(results) {
        let actual = rust_filter(case);
        let expected: Vec<Vec<String>> = expected
            .as_array()
            .expect("filter result array")
            .iter()
            .map(|candidates| {
                candidates
                    .as_array()
                    .expect("candidate array")
                    .iter()
                    .map(|value| value.as_str().expect("candidate string").to_string())
                    .collect()
            })
            .collect();
        assert_eq!(actual, expected, "filter mismatch for query {:?}", case.query);
    }
}
