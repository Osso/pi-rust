use crate::shared_oracle as oracle;

use pi_coding_agent::utils::json::strip_json_comments;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_003;

const INPUTS: [&str; 20] = [
    "{\"a\":1,//c\n\"b\":2}",
    "// only comment",
    "{\"a\":1,}",
    "{\"a\":[1,2,],}",
    "[1 , \n ]",
    "{\"url\":\"http://x//y\"}",
    "{\"s\":\",}\"}",
    "{\"s\":\"a\\\"//b\"}",
    "{\"s\":\"a\\\\\\\\\",\"x\":1}",
    "{\"a\":\"//\"}//tail",
    "{\"a\":1,//c\r\n\"b\":2}",
    "[1,\t]",
    "{\"a\":1,,\n}",
    "\"unterminated",
    "{\"a\":\"escaped \\\\\" quote\",\"b\":[true,false,null],}",
    "{\n  // leading\n  \"a\": 1, // trailing\n  \"b\": [2, 3,],\n}",
    "[,]",
    "{}",
    "//no newline at end",
    "{\"a\":\"//\",\"b\":1}//",
];

#[test]
fn coding_agent_json_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-json-oracle.ts", "packages/coding-agent/src/utils/json.ts");
    let expected = oracle.request(&request());
    let actual = response();

    assert_eq!(actual, expected, "coding-agent json mismatch");
}

fn request() -> Value {
    let cases = INPUTS
        .iter()
        .enumerate()
        .map(|(index, input)| json!({"name": format!("case {index}"), "input": input}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_json", "cases": cases})
}

fn response() -> Value {
    let results = INPUTS
        .iter()
        .enumerate()
        .map(|(index, input)| json!({"name": format!("case {index}"), "value": strip_json_comments(input)}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
