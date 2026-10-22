use pi_agent_core::harness::tools::edit_diff::FuzzyMatchResult;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_184;
const EXPECTED_CASE_COUNT: usize = 8;

struct Case {
    name: &'static str,
    found: bool,
    index_bits: &'static str,
    match_length_bits: &'static str,
    used_fuzzy_match: bool,
    content_for_replacement: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASE_COUNT);

    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "fuzzy match result mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "fuzzy_match_result",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "found": case.found,
        "indexBits": case.index_bits,
        "matchLengthBits": case.match_length_bits,
        "usedFuzzyMatch": case.used_fuzzy_match,
        "contentForReplacement": case.content_for_replacement,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(REQUEST_ID, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let result = FuzzyMatchResult {
        found: case.found,
        index: binary64_from_bits(case.index_bits),
        match_length: binary64_from_bits(case.match_length_bits),
        used_fuzzy_match: case.used_fuzzy_match,
        content_for_replacement: JsString::from(case.content_for_replacement.to_owned()),
    };
    json!({
        "name": case.name,
        "found": result.found,
        "indexBits": binary64_bits(result.index),
        "matchLengthBits": binary64_bits(result.match_length),
        "usedFuzzyMatch": result.used_fuzzy_match,
        "contentForReplacement": result.content_for_replacement,
    })
}

fn binary64_from_bits(bits: &str) -> f64 {
    let value = u64::from_str_radix(bits, 16).expect("fixture bits must be lowercase hexadecimal");
    f64::from_bits(value)
}

fn binary64_bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn cases() -> [Case; EXPECTED_CASE_COUNT] {
    [
        ordinary_case(),
        fractional_case(),
        signed_zero_case(),
        subnormal_case(),
        infinity_case(),
        nan_case(),
        negative_finite_case(),
        unicode_case(),
    ]
}

fn ordinary_case() -> Case {
    Case {
        name: "ordinary values",
        found: true,
        index_bits: "4000000000000000",
        match_length_bits: "4014000000000000",
        used_fuzzy_match: false,
        content_for_replacement: "before",
    }
}

fn fractional_case() -> Case {
    Case {
        name: "fractional values and independent booleans",
        found: false,
        index_bits: "bfe0000000000000",
        match_length_bits: "400a000000000000",
        used_fuzzy_match: true,
        content_for_replacement: "",
    }
}

fn signed_zero_case() -> Case {
    Case {
        name: "signed zeros and control content",
        found: true,
        index_bits: "8000000000000000",
        match_length_bits: "0000000000000000",
        used_fuzzy_match: false,
        content_for_replacement: "\0\r\n",
    }
}

fn subnormal_case() -> Case {
    Case {
        name: "subnormal and maximum finite",
        found: false,
        index_bits: "0000000000000001",
        match_length_bits: "7fefffffffffffff",
        used_fuzzy_match: false,
        content_for_replacement: "代理🙂",
    }
}

fn infinity_case() -> Case {
    Case {
        name: "infinities",
        found: true,
        index_bits: "7ff0000000000000",
        match_length_bits: "fff0000000000000",
        used_fuzzy_match: true,
        content_for_replacement: "infinity",
    }
}

fn nan_case() -> Case {
    Case {
        name: "distinct NaN payloads",
        found: false,
        index_bits: "7ff8000000000001",
        match_length_bits: "7ff8000000000042",
        used_fuzzy_match: false,
        content_for_replacement: "nan",
    }
}

fn negative_finite_case() -> Case {
    Case {
        name: "negative finite values",
        found: false,
        index_bits: "c00c000000000000",
        match_length_bits: "bff8000000000000",
        used_fuzzy_match: true,
        content_for_replacement: "negative",
    }
}

fn unicode_case() -> Case {
    Case {
        name: "combined Unicode and line endings",
        found: true,
        index_bits: "3ff0000000000000",
        match_length_bits: "4000000000000000",
        used_fuzzy_match: false,
        content_for_replacement: "更新\r\n代理🙂\0",
    }
}
