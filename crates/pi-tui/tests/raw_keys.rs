use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::keys::{decode_printable_key, is_key_release, is_key_repeat, parse_key};
use serde_json::json;

#[test]
fn kitty_and_modify_other_keys_decode_surrogate_codepoints_as_native_strings() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for input in [
        "\x1b[55296u",
        "\x1b[56320u",
        "\x1b[128512u",
        "\x1b[97:55296;2u",
        "\x1b[27;1;55296~",
        "\x1b[27;1;56320~",
    ] {
        let expected = oracle.request(&json!({"operation":"keys","units":JsString::from(input).units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = serde_json::to_string(&(
            decode_printable_key(input),
            parse_key(input),
            is_key_release(input),
            is_key_repeat(input),
        ))
        .unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "input={input:?}"
        );
    }
}

#[test]
fn native_key_input_recognition_and_identifier_templates_keep_units() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for (input, key) in [
        (JsString::from_utf16(vec![0xd800]), JsString::from_utf16(vec![0xdc00])),
        (JsString::from("k"), JsString::from("K")),
        (JsString::from_utf16(vec![0xd800]) + ":3u", JsString::from("a")),
        (JsString::from_utf16(vec![0xdc00]) + ":2u", JsString::from("b")),
    ] {
        let expected = oracle.request(&json!({"operation":"key_identity","units":input.units(),"key":key.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = serde_json::to_string(&(
            pi_tui::keys::matches_key(&input, &key),
            pi_tui::keys::Key::ctrl(&key),
            pi_tui::keys::Key::alt(&key),
            is_key_release(&input),
            is_key_repeat(&input),
        ))
        .unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap()
        );
    }
}
