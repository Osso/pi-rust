use pi_agent_core::harness::tools::edit_diff::AppliedEditsResult;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_183;
const EXPECTED_CASE_COUNT: usize = 4;

struct Case {
    name: &'static str,
    base_content: &'static str,
    new_content: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASE_COUNT);

    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "applied edits result mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "applied_edits_result",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "baseContent": case.base_content,
        "newContent": case.new_content,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let result = AppliedEditsResult {
        base_content: JsString::from(case.base_content.to_owned()),
        new_content: JsString::from(case.new_content.to_owned()),
    };
    json!({
        "name": case.name,
        "baseContent": result.base_content,
        "newContent": result.new_content,
    })
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "empty strings",
            base_content: "",
            new_content: "",
        },
        Case {
            name: "ordinary replacement",
            base_content: "before",
            new_content: "after",
        },
        Case {
            name: "unicode nul newline",
            base_content: "旧\n\0",
            new_content: "🙂\r\n",
        },
        Case {
            name: "duplicate values",
            base_content: "same\r\nline",
            new_content: "same\r\nline",
        },
    ]
}
