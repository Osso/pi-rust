use crate::shared_oracle_1 as oracle;
use pi_js_compat::{base64::buffer_from_base64, js_json::JsString};
use serde_json::json;

#[test]
fn buffer_base64_observes_utf16_units_instead_of_reencoded_utf8() {
    let mut oracle = oracle::TsOracle::spawn("raw-base64-oracle.mjs", "packages/tui/src");
    let text = "QQ😀QQ";
    let expected = oracle.request(&json!({"units":JsString::from(text).units()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let hex: String = buffer_from_base64(text)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(hex, expected["hex"].as_str().unwrap());
}

#[test]
fn buffer_base64_matches_node_for_raw_high_low_pairs_and_wide_low_bytes() {
    let mut oracle = oracle::TsOracle::spawn("raw-base64-oracle.mjs", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd841, 0xd841],
        vec![0xdc41, 0xdc41],
        vec![0xd841, 0xdc41],
        vec![0x141, 0x141],
        vec![81, 81, 0xd800, 81, 81],
        vec![0x151, 0x151, 0x13d, 0x13d],
        vec![0x120, 81, 0x109, 81, 0x10a, 61],
        vec![45, 95, 43, 47, 65, 81, 61, 61],
    ] {
        let expected = oracle.request(&json!({"units":units}));
        assert_eq!(expected["ok"], true, "{expected}");
        let text = JsString::from_utf16(units.clone());
        let hex: String = buffer_from_base64(&text)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hex, expected["hex"].as_str().unwrap(), "units={units:?}");
    }
}
