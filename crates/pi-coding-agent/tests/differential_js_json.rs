use crate::shared_oracle as oracle;

use oracle::TsOracle;
use pi_js_compat::js_json::{parse, stringify, stringify_pretty};
use serde_json::{Value, json};

const SEEDS: [&str; 11] = [
    "{\"providers\":{\"anthropic\":{\"type\":\"api_key\",\"key\":\"sk-1\"}},\"n\":[1,-2.5e3,0.1,true,false,null]}",
    "{\n  \"/home/user/project\": true,\n  \"/tmp\": false,\n  \"/opt\": null\n}\n",
    "[\"a\\\"b\", \"\\u00e9\\n\", {\"1\": 1, \"0\": 0, \"z\": {}}, []]",
    "{\r\n\t\"permissionRules\": {\"allow\": {\"bash\": [\"ls -la\", \"git status\"]}}\r\n}",
    "  123.456e-7  ",
    "{\"models\":[{\"id\":\"m\",\"cost\":{\"input\":0.000003,\"output\":15}}],\"checkedAt\":1760000000000}",
    "\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
    "[1e400, -1e-400, 9007199254740993, -0.0, 5e-324]",
    "{\"k\":\"v\"}",
    "[true,false,null]",
    "{\"a\":[{\"b\":\"c\"}]}",
];
const INSERTIONS: [&str; 26] = [
    "x", ",", "}", "]", "\"", "\\", ":", "\n", "\r", " ", "-", "0", ".", "e", "{", "[", "\u{1}", "é", "\t", "t", "f",
    "n", "u", "+", "\\u", "😀",
];

fn mutated_inputs() -> Vec<String> {
    let mut inputs: Vec<String> = SEEDS.iter().map(|seed| (*seed).to_owned()).collect();
    for seed in SEEDS {
        let chars: Vec<char> = seed.chars().collect();
        for index in 0..=chars.len() {
            inputs.push(chars[..index].iter().collect());
            if index < chars.len() {
                inputs.push(chars[..index].iter().chain(&chars[index + 1..]).collect());
            }
            let insertion = INSERTIONS[index % INSERTIONS.len()];
            let mut inserted: String = chars[..index].iter().collect();
            inserted.push_str(insertion);
            inserted.extend(&chars[index..]);
            inputs.push(inserted);
        }
    }
    inputs.extend(
        [
            "NaN",
            "undefined",
            "Infinity",
            "[object Object]",
            "-Infinity",
            "",
            " ",
            "\u{feff}{}",
            "\u{a0}1",
        ]
        .map(str::to_owned),
    );
    inputs
}

fn rust_result(text: &str) -> Value {
    match parse(text) {
        Ok(value) => json!({"compact": stringify(&value), "pretty": stringify_pretty(&value)}),
        Err(error) => json!({"error": error.to_string()}),
    }
}

#[test]
fn js_json_matches_the_javascript_engine() {
    let inputs = mutated_inputs();
    let mut oracle = TsOracle::spawn("js-json-oracle.ts", "packages/coding-agent/package.json");
    let response = oracle.request(&json!({"id": 1, "texts": inputs}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = response["output"].as_array().expect("oracle output must be an array");
    assert_eq!(expected.len(), inputs.len());
    let mismatches: Vec<String> = inputs
        .iter()
        .zip(expected)
        .filter(|(input, expected)| rust_result(input) != **expected)
        .map(|(input, expected)| format!("{input:?}: rust {} vs js {expected}", rust_result(input)))
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} cases differ:\n{}",
        mismatches.len(),
        inputs.len(),
        mismatches.join("\n")
    );
}
