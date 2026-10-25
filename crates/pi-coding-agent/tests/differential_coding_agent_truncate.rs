#[path = "support/corpus.rs"]
mod corpus;
#[path = "support/oracle.rs"]
pub(super) mod oracle;

use corpus::Corpus;
use pi_coding_agent::core::tools::truncate::{
    TruncationOptions, TruncationResult, truncate_head, truncate_line, truncate_tail,
};
use serde_json::{Value, json};

const ALPHABET: &[&str] = &["a", "b", " ", "\n", "\n", "\r\n", "é", "€", "😀", "\t", "\u{0}", "ab\n"];

enum Case {
    Head(String, Option<TruncationOptions>),
    Tail(String, Option<TruncationOptions>),
    Line(String, Option<f64>),
}

#[test]
fn coding_agent_truncate_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-truncate-oracle.ts",
        "packages/coding-agent/src/core/tools/truncate.ts",
    );
    let request =
        json!({"id": 27_001, "operation": "truncate", "cases": cases.iter().map(request_case).collect::<Vec<_>>()});
    let expected = oracle.request(&request);
    let actual = json!({"id": 27_001, "ok": true, "output": cases.iter().map(actual_case).collect::<Vec<_>>()});

    assert_eq!(
        expected["output"].as_array().map(Vec::len),
        Some(cases.len()),
        "{expected}"
    );
    for (index, (actual, expected)) in actual["output"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["output"].as_array().expect("oracle output array"))
        .enumerate()
    {
        assert_eq!(actual, expected, "case {index}");
    }
}

#[test]
fn frozen_fractional_byte_suffix_and_line_limit_contract() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-truncate-oracle.ts",
        "packages/coding-agent/src/core/tools/truncate.ts",
    );
    let result = oracle.request(&json!({"id": 27006, "cases": [
        {"kind": "tail", "content": "last", "options": {"maxLines": 10, "maxBytes": 1.25}},
        {"kind": "line", "line": "abcdef", "maxChars": 1.25},
        {"kind": "head", "content": "ab\nc", "options": {"maxLines": 1.25, "maxBytes": 100}}
    ]}));
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["output"][0]["content"], "st");
    assert_eq!(result["output"][0]["outputBytes"], 2);
    assert_eq!(
        result["output"][1],
        json!({"text": "a... [truncated]", "wasTruncated": true})
    );
    assert_eq!(result["output"][2]["content"], "ab\nc");
}

fn text(corpus: &mut Corpus, alphabet: &[&str], length: usize) -> String {
    (0..length).map(|_| alphabet[corpus.below(alphabet.len())]).collect()
}

fn cases() -> Vec<Case> {
    let mut cases = targeted_cases();
    for limit in [
        -1.0,
        -0.0,
        0.25,
        1.25,
        2.75,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e30,
    ] {
        let options = Some(TruncationOptions {
            max_lines: limit,
            max_bytes: limit,
        });
        cases.push(Case::Head("ab\nc€😀".into(), options));
        cases.push(Case::Tail("ab\nc€😀".into(), options));
        cases.push(Case::Line("ab😀cd".into(), Some(limit)));
    }
    let mut corpus = Corpus::new(0x7275_6e63);
    for _ in 0..1_500 {
        let length = corpus.below(40);
        let content = text(&mut corpus, ALPHABET, length);
        let limits = TruncationOptions {
            max_lines: corpus.below(8) as f64,
            max_bytes: corpus.below(48) as f64,
        };
        if corpus.below(2) == 0 {
            cases.push(Case::Head(content, Some(limits)));
        } else {
            cases.push(Case::Tail(content, Some(limits)));
        }
    }
    for _ in 0..200 {
        let length = corpus.below(12);
        let line = text(&mut corpus, &["a", "é", "€", "\n", " "], length);
        cases.push(Case::Line(line, Some(corpus.below(10) as f64)));
    }
    cases
}

fn targeted_cases() -> Vec<Case> {
    let big = "line\n".repeat(2_100);
    let wide = "x".repeat(10_241);
    vec![
        Case::Tail(
            "last".to_owned(),
            Some(TruncationOptions {
                max_lines: 10.0,
                max_bytes: 1.25,
            }),
        ),
        Case::Head(String::new(), None),
        Case::Tail(String::new(), None),
        Case::Head(big.clone(), None),
        Case::Tail(big, None),
        Case::Head(wide.clone(), None),
        Case::Tail(wide, None),
        Case::Head("a\nb\n".to_owned(), None),
        Case::Tail(
            "first\nxé😀".to_owned(),
            Some(TruncationOptions {
                max_lines: 10 as f64,
                max_bytes: 5 as f64,
            }),
        ),
        Case::Tail(
            "😀".to_owned(),
            Some(TruncationOptions {
                max_lines: 1 as f64,
                max_bytes: 0 as f64,
            }),
        ),
        Case::Head(
            "\n\n\n".to_owned(),
            Some(TruncationOptions {
                max_lines: 0 as f64,
                max_bytes: 0 as f64,
            }),
        ),
        Case::Line("x".repeat(501), None),
        Case::Line("x".repeat(500), None),
        Case::Line("abcdef".to_owned(), Some(0.0)),
    ]
}

fn encode_limit(value: f64) -> Value {
    if value.is_nan() {
        return json!("NaN");
    }
    if value == f64::INFINITY {
        return json!("Infinity");
    }
    if value == f64::NEG_INFINITY {
        return json!("-Infinity");
    }
    if value == 0.0 && value.is_sign_negative() {
        return json!("-0");
    }
    json!(value)
}

fn options_json(options: &Option<TruncationOptions>) -> Value {
    options.map_or(
        Value::Null,
        |value| json!({"maxLines": encode_limit(value.max_lines), "maxBytes": encode_limit(value.max_bytes)}),
    )
}

fn request_case(case: &Case) -> Value {
    let mut value = match case {
        Case::Head(content, options) => json!({"kind": "head", "content": content, "options": options_json(options)}),
        Case::Tail(content, options) => json!({"kind": "tail", "content": content, "options": options_json(options)}),
        Case::Line(line, max_chars) => json!({"kind": "line", "line": line, "maxChars": max_chars.map(encode_limit)}),
    };
    let object = value.as_object_mut().unwrap();
    object.retain(|_, field| !field.is_null());
    value
}

fn actual_case(case: &Case) -> Value {
    match case {
        Case::Head(content, options) => result_json(truncate_head(content, options.unwrap_or_default())),
        Case::Tail(content, options) => result_json(truncate_tail(content, options.unwrap_or_default())),
        Case::Line(line, max_chars) => {
            let result = truncate_line(line, *max_chars);
            json!({"text": result.text, "wasTruncated": result.was_truncated})
        }
    }
}

fn result_json(result: TruncationResult) -> Value {
    serde_json::to_value(result).expect("serialize the public result contract without rebuilding it")
}
