use crate::shared_oracle as oracle;

use indexmap::IndexMap;

use pi_coding_agent::utils::footer_status::{format_extension_status_text_lines, sanitize_footer_status_text};
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_004;

enum Case {
    Sanitize(&'static str),
    Lines(&'static [(&'static str, &'static str)]),
}

const CASES: [Case; 13] = [
    Case::Sanitize("a\r\n\tb"),
    Case::Sanitize("  hello   world  "),
    Case::Sanitize("\r\n"),
    Case::Sanitize("a\u{a0}\u{a0}b"),
    Case::Sanitize("a\t\tb\nc"),
    Case::Sanitize("goal\tstatus"),
    Case::Lines(&[]),
    Case::Lines(&[("goal", "G")]),
    Case::Lines(&[("z", "Z"), ("a", "A")]),
    Case::Lines(&[("b", "  B  "), ("goal", "Ship\tit"), ("a", "A")]),
    Case::Lines(&[("goal", "  ")]),
    Case::Lines(&[("a", "")]),
    Case::Lines(&[("a", ""), ("b", "")]),
];

#[test]
fn coding_agent_footer_status_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-footer-status-oracle.ts",
        "packages/coding-agent/src/utils/footer-status.ts",
    );
    let expected = oracle.request(&request());
    let actual = response();

    assert_eq!(actual, expected, "coding-agent footer-status mismatch");
}

fn request() -> Value {
    let cases = CASES
        .iter()
        .enumerate()
        .map(|(index, case)| match case {
            Case::Sanitize(input) => json!({
                "kind": "sanitize",
                "name": format!("case {index}"),
                "input": input,
            }),
            Case::Lines(statuses) => json!({
                "kind": "lines",
                "name": format!("case {index}"),
                "statuses": statuses,
            }),
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_footer_status", "cases": cases})
}

fn response() -> Value {
    let results = CASES
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let value = match case {
                Case::Sanitize(input) => json!(sanitize_footer_status_text(*input)),
                Case::Lines(statuses) => {
                    let map = statuses
                        .iter()
                        .map(|(key, text)| ((*key).into(), (*text).into()))
                        .collect::<IndexMap<_, _>>();
                    json!(format_extension_status_text_lines(&map))
                }
            };
            json!({"name": format!("case {index}"), "value": value})
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
