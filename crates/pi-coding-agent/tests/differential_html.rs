use crate::shared_oracle as oracle;

use pi_coding_agent::utils::html::{decode_html_entity, decode_html_entity_at};
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_002;

enum Case {
    Entity(&'static str),
    At(&'static str, usize),
}

const CASES: [Case; 31] = [
    Case::Entity("amp"),
    Case::Entity("lt"),
    Case::Entity("gt"),
    Case::Entity("quot"),
    Case::Entity("apos"),
    Case::Entity("nbsp"),
    Case::Entity("AMP"),
    Case::Entity(""),
    Case::Entity("#x41"),
    Case::Entity("#X41"),
    Case::Entity("#x1f600"),
    Case::Entity("#65"),
    Case::Entity("#0"),
    Case::Entity("#x41zz"),
    Case::Entity("# 65"),
    Case::Entity("#x 41"),
    Case::Entity("#x+41"),
    Case::Entity("#x-41"),
    Case::Entity("#"),
    Case::Entity("#x"),
    Case::Entity("#xzz"),
    Case::Entity("#x110000"),
    Case::Entity("#-1"),
    Case::Entity("#x10FFFF"),
    Case::At("a&amp;b", 1),
    Case::At("&#65;", 0),
    Case::At("a&amp b", 1),
    Case::At("&0123456789abcdef;", 0),
    Case::At("&unknown;", 0),
    Case::At("&amp;", 1),
    Case::At("", 0),
];

#[test]
fn coding_agent_html_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-html-oracle.ts", "packages/coding-agent/src/utils/html.ts");
    let expected = oracle.request(&request());
    let actual = response();

    assert_eq!(actual, expected, "coding-agent html mismatch");
}

fn request() -> Value {
    let cases = CASES
        .iter()
        .enumerate()
        .map(|(index, case)| match case {
            Case::Entity(input) => json!({
                "kind": "entity",
                "name": format!("case {index}"),
                "input": input,
            }),
            Case::At(input, position) => json!({
                "kind": "at",
                "name": format!("case {index}"),
                "input": input,
                "index": position,
            }),
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_html", "cases": cases})
}

fn response() -> Value {
    let results = CASES
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let value = match case {
                Case::Entity(input) => decode_html_entity(&(*input).into()).map_or(Value::Null, |text| json!(text)),
                Case::At(input, position) => decode_html_entity_at(&(*input).into(), *position).map_or(
                    Value::Null,
                    |decoded| json!({"text": decoded.text, "length": decoded.length}),
                ),
            };
            json!({"name": format!("case {index}"), "value": value})
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
