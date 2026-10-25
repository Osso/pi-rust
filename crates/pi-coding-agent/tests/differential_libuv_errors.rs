use crate::shared_oracle as oracle;

use pi_js_compat::node_fs::node_file_read_error_record;
use serde_json::{Value, json};

#[test]
fn all_runtime_libuv_errors_and_unknown_codes_keep_node_names_and_descriptions() {
    let mut oracle = oracle::TsOracle::spawn("libuv-errors-oracle.mjs", "packages/coding-agent/src/utils/mime.ts");
    let expected: Value = oracle.request(&json!({"codes": [-126, 9999]}));
    let errors = expected["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 85, "runtime table changed: {expected}");
    let mut mismatches = Vec::new();
    for entry in errors {
        let errno = entry[0].as_i64().unwrap() as i32;
        compare_error(-errno, &entry[1][0], &entry[1][1], &mut mismatches);
    }
    for probe in expected["probes"].as_array().unwrap() {
        let code = probe["code"].as_i64().unwrap() as i32;
        compare_error(-code, &probe["name"], &probe["description"], &mut mismatches);
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn compare_error(input: i32, name: &Value, description: &Value, mismatches: &mut Vec<String>) {
    let actual = node_file_read_error_record(&std::io::Error::from_raw_os_error(input));
    let name = name.as_str().unwrap();
    let description = description.as_str().unwrap();
    let message = format!("{name}: {description}, read");
    if actual.code != name || actual.message != message.as_str() || actual.errno != Some(-input) {
        mismatches.push(format!("input {input}: {actual:?}; expected {message:?}"));
    }
}
