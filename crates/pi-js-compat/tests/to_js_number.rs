//! Node-checked ToNumber over lossless JSON values, including binary64 bits.
use crate::shared_oracle_2 as oracle;
use pi_js_compat::js_json::{parse_lossless, to_js_number};
use serde_json::{Value, json};

#[test]
fn to_number_matches_node_values_coercion_errors_and_utf16_without_replacement() {
    let mut cases = vec![
        "null".to_owned(),
        "false".into(),
        "true".into(),
        "0".into(),
        "-0".into(),
        "1.25".into(),
        "1e400".into(),
        "-1e400".into(),
        "[]".into(),
        "[null]".into(),
        "[false]".into(),
        "[true]".into(),
        "[1,2]".into(),
        "{}".into(),
        r#"{"toString":0}"#.into(),
        r#"{"valueOf":0}"#.into(),
        r#"{"toString":null,"valueOf":true}"#.into(),
        "[{}]".into(),
        r#"[{"toString":0}]"#.into(),
        r#""\ud800""#.into(),
        r#""\udc00""#.into(),
        r#"["\ud800"]"#.into(),
        r#"[["\ufeff-0"]]"#.into(),
    ];
    for text in [
        "",
        "  ",
        "\u{a0}\u{feff}12\u{2028}",
        "0",
        "-0",
        "1.5",
        " 42 ",
        ".5",
        "5.",
        "1e3",
        "1E-2",
        "+7",
        "-7",
        "0x1F",
        "0X1f",
        "0b101",
        "0o17",
        "Infinity",
        "-Infinity",
        "+Infinity",
        "\u{85}",
        "-0x10",
        "0x",
        "inf",
        "nan",
        "NaN",
        "1_000",
        "1 2",
        "12px",
        "1e",
        "+-1",
        "1e999",
        "-1e999",
        "0.000001",
        "1e-7",
    ] {
        let literal = serde_json::to_string(text).unwrap();
        cases.push(literal.clone());
        cases.push(format!("[{literal}]"));
        cases.push(format!("[[{literal}]]"));
    }
    let mut source = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let expected = source.request(&json!({"id":"to-number","operation":"to_number","valuesJson":cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual: Vec<Value> = cases
        .iter()
        .map(|text| {
            let input = parse_lossless(text.as_str()).unwrap();
            match to_js_number(&input) {
                Ok(value) if value.is_nan() => json!({"kind":"nan"}),
                Ok(value) => json!({"bits":format!("{:016x}",value.to_bits())}),
                Err(message) => json!({"error":{"name":"TypeError","message":message}}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected["output"]["cases"]);
}
