#[path = "support/corpus.rs"]
mod corpus;
#[path = "support/oracle.rs"]
mod oracle;

use corpus::Corpus;
use pi_coding_agent::core::tools::truncate::{
    TruncatedBy, TruncationOptions, TruncationResult, truncate_head, truncate_line, truncate_tail,
};
use serde_json::{Value, json};

const ALPHABET: &[&str] = &["a", "b", " ", "\n", "\n", "\r\n", "é", "€", "😀", "\t", "\u{0}", "ab\n"];

enum Case {
    Head(String, Option<TruncationOptions>),
    Tail(String, Option<TruncationOptions>),
    Line(String, Option<usize>),
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

fn cases() -> Vec<Case> {
    let mut cases = targeted_cases();
    let mut corpus = Corpus::new(0x7275_6e63);
    for _ in 0..1_500 {
        let length = corpus.below(40);
        let content = corpus.text(ALPHABET, length);
        let limits = TruncationOptions {
            max_lines: corpus.below(8),
            max_bytes: corpus.below(48),
        };
        if corpus.below(2) == 0 {
            cases.push(Case::Head(content, Some(limits)));
        } else {
            cases.push(Case::Tail(content, Some(limits)));
        }
    }
    for _ in 0..200 {
        let length = corpus.below(12);
        let line = corpus.text(&["a", "é", "€", "\n", " "], length);
        cases.push(Case::Line(line, Some(corpus.below(10))));
    }
    cases
}

fn targeted_cases() -> Vec<Case> {
    let big = "line\n".repeat(2_100);
    let wide = "x".repeat(10_241);
    vec![
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
                max_lines: 10,
                max_bytes: 5,
            }),
        ),
        Case::Tail(
            "😀".to_owned(),
            Some(TruncationOptions {
                max_lines: 1,
                max_bytes: 0,
            }),
        ),
        Case::Head(
            "\n\n\n".to_owned(),
            Some(TruncationOptions {
                max_lines: 0,
                max_bytes: 0,
            }),
        ),
        Case::Line("x".repeat(501), None),
        Case::Line("x".repeat(500), None),
        Case::Line("abcdef".to_owned(), Some(0)),
    ]
}

fn options_json(options: &Option<TruncationOptions>) -> Value {
    options.map_or(
        Value::Null,
        |value| json!({"maxLines": value.max_lines, "maxBytes": value.max_bytes}),
    )
}

fn request_case(case: &Case) -> Value {
    let mut value = match case {
        Case::Head(content, options) => json!({"kind": "head", "content": content, "options": options_json(options)}),
        Case::Tail(content, options) => json!({"kind": "tail", "content": content, "options": options_json(options)}),
        Case::Line(line, max_chars) => json!({"kind": "line", "line": line, "maxChars": max_chars}),
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
    let truncated_by = result.truncated_by.map(|value| match value {
        TruncatedBy::Lines => "lines",
        TruncatedBy::Bytes => "bytes",
    });
    json!({
        "content": result.content,
        "truncated": result.truncated,
        "truncatedBy": truncated_by,
        "totalLines": result.total_lines,
        "totalBytes": result.total_bytes,
        "outputLines": result.output_lines,
        "outputBytes": result.output_bytes,
        "lastLinePartial": result.last_line_partial,
        "firstLineExceedsLimit": result.first_line_exceeds_limit,
        "maxLines": result.max_lines,
        "maxBytes": result.max_bytes,
    })
}
