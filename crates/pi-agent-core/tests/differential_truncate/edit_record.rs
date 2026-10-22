use pi_agent_core::harness::tools::edit_diff::Edit;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_181;
const EXPECTED_CASE_COUNT: usize = 4;

struct Case {
    name: &'static str,
    old_text: &'static str,
    new_text: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASE_COUNT);

    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "edit record mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "edit_record",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "oldText": case.old_text,
        "newText": case.new_text,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let edit = Edit {
        old_text: JsString::from(case.old_text.to_owned()),
        new_text: JsString::from(case.new_text.to_owned()),
    };
    json!({
        "name": case.name,
        "oldText": edit.old_text,
        "newText": edit.new_text,
    })
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "empty strings",
            old_text: "",
            new_text: "",
        },
        Case {
            name: "ordinary replacement",
            old_text: "before",
            new_text: "after",
        },
        Case {
            name: "unicode nul newline",
            old_text: "旧\n\0",
            new_text: "🙂\r\n",
        },
        Case {
            name: "duplicate values",
            old_text: "same\r\nline",
            new_text: "same\r\nline",
        },
    ]
}
