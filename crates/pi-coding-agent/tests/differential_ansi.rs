use crate::shared_oracle as oracle;

use pi_coding_agent::utils::ansi::strip_ansi;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_001;

fn compatibility_inputs() -> Vec<String> {
    let mut inputs = vec![
        "plain".to_owned(),
        "a\x1b[31mred\x1b[0mz".to_owned(),
        "a\x1b]8;;https://example.com\x07link\x1b]8;;\x07z".to_owned(),
        "a\x1b]unterminated".to_owned(),
        "a\x1b]funterminated".to_owned(),
        "a\x1bPabc\x1b\\z".to_owned(),
        "a\x1b^abc\x07z".to_owned(),
        "a\x1b_abc\u{9c}z".to_owned(),
        "a\u{90}abc\u{9c}z".to_owned(),
        "a\u{9d}abc\u{9c}z".to_owned(),
        "a\u{9b}31mred".to_owned(),
        "a\x1b(0x".to_owned(),
        "a\x1b*0x".to_owned(),
        "a\x1b+c".to_owned(),
        "a\x1b/0x".to_owned(),
        "a\x1bcok".to_owned(),
        "a\x1b\\ok".to_owned(),
    ];
    let characters = [
        "a", "f", "0", "1", ";", ":", "[", "]", "(", ")", "#", "?", "m", "P", "_", "\\", "\u{7}", "\u{1b}", "\u{9b}",
        "\u{9c}", "\u{90}", "\u{9d}",
    ];

    for character in characters {
        inputs.push(format!("x\u{1b}{character}y"));
        inputs.push(format!("x\u{9b}{character}y"));
        for sampled in characters.iter().step_by(3) {
            inputs.push(format!("x\u{1b}{character}{sampled}y"));
        }
    }

    inputs
}

#[test]
fn coding_agent_ansi_matches_the_pinned_typescript_source() {
    let inputs = compatibility_inputs();
    let mut oracle = oracle::TsOracle::spawn("coding-agent-ansi-oracle.ts", "packages/coding-agent/src/utils/ansi.ts");
    let expected = oracle.request(&request(&inputs));
    let actual = response(&inputs);

    assert_eq!(actual, expected, "coding-agent ansi mismatch");
}

fn request(inputs: &[String]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "coding_agent_ansi",
        "cases": inputs
            .iter()
            .enumerate()
            .map(|(index, input)| json!({"name": format!("case {index}"), "input": input}))
            .collect::<Vec<_>>(),
    })
}

fn response(inputs: &[String]) -> Value {
    let results = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| json!({"name": format!("case {index}"), "value": strip_ansi(&JsString::from(input))}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
