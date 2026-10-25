use crate::shared_oracle as oracle;

use pi_coding_agent::utils::frontmatter::parse_frontmatter;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_007;

const CASES: [&str; 28] = [
    "---\nname: \"skill-name\"\ndescription: 'A desc'\nfoo-bar: value\n---\n\nBody text",
    "---\r\nname: test\r\n---\r\nLine one\r\nLine two",
    "---\nfoo: [bar\n---\nBody",
    "---\ndescription: |\n  Line one\n  Line two\n---\n\nBody",
    "---\ndescription: >\n  Line one\n  Line two\n---\nBody",
    "---\ndescription: |-\n  Line one\n  Line two\n---\nBody",
    "Just text\nsecond line",
    "---\nname: test\nBody without terminator",
    "---\n# just a comment\n---\nBody",
    "---\n---\nBody",
    "---\nkey: value\n---",
    "",
    "---\nlist:\n  - one\n  - two\nnested:\n  enabled: true\n  count: 3\n  empty: null\n---\nBody",
    "---\nflow: {a: 1, b: [x, y]}\n---\nBody",
    "---\nfloat: 1.5\nnegative: -2\nstring: \"007\"\n---\nBody",
    "---\nquoted: 'it''s'\ndouble: \"a: b\"\n---\nBody",
    "---\nunicode: caf\u{e9}\n---\nBody",
    "---\nitems:\n  - name: first\n    value: 1\n  - name: second\n    value: 2\n---\nBody",
    "---\nmulti: |\n  first\n\n  third\n---\nBody",
    "---\nkey: value\n---\n\n  indented body  \n",
    "---\na: 1\n---\n---\nBody",
    "----\nBody",
    "\n---\nkey: value\n---\nBody",
    "---\nyes_value: yes\nno_value: no\n---\nBody",
    "---\n- one\n- two\n---\nBody",
    "---\njust a scalar\n---\nBody",
    "---\nquoted: \"line\\nbreak\"\n---\nBody",
    "---\nnull_value:\n---\nBody",
];

#[test]
fn coding_agent_frontmatter_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-frontmatter-oracle.ts",
        "packages/coding-agent/src/utils/frontmatter.ts",
    );
    let expected = oracle.request(&request());
    let actual = response();

    assert_eq!(actual, expected, "coding-agent frontmatter mismatch");
}

fn request() -> Value {
    let cases = CASES
        .iter()
        .enumerate()
        .map(|(index, content)| json!({"name": format!("case {index}"), "content": content}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_frontmatter", "cases": cases})
}

fn response() -> Value {
    let results = CASES
        .iter()
        .enumerate()
        .map(|(index, content)| match parse_frontmatter(*content) {
            Ok(parsed) => json!({
                "name": format!("case {index}"),
                "ok": true,
                "value": {"frontmatter": parsed.frontmatter, "body": parsed.body},
            }),
            Err(_) => json!({"name": format!("case {index}"), "ok": false}),
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
