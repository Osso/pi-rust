#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::terminal_image::{KittyEncodeOptions, encode_kitty};
use serde_json::json;

#[test]
fn kitty_chunking_keeps_native_units_when_a_pair_crosses_the_source_boundary() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let text = "A".repeat(4095) + "😀Z";
    let expected = oracle.request(&json!({"operation":"kitty_encode","units":JsString::from(&text).units()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let result = encode_kitty(&text, &KittyEncodeOptions::default());
    let actual = serde_json::to_string(&result).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}
