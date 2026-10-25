use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::terminal_colors::{is_osc11_background_color_response, parse_osc11_background_color};
use serde_json::json;

#[test]
fn native_osc11_regex_framing_keeps_source_final_newline_behavior() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for text in [
        "\x1b]11;#abcdef\x07\n",
        "\x1b]11;\u{feff}#aabbcc\u{00a0}\x07",
        "\x1b]11;RGB:ff/00/aa\x1b\\",
    ] {
        let expected = oracle.request(&json!({"operation":"terminal_colors","units":JsString::from(text).units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let color = parse_osc11_background_color(text).map(|color| (color.r, color.g, color.b));
        let actual = serde_json::to_string(&(is_osc11_background_color_response(text), color)).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "text={text:?}"
        );
    }
}

#[test]
fn osc11_framing_accepts_native_payload_without_sanitizing_surrogates() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [vec![0xd800], vec![0xdc00], vec![0xd83d, 0xde00]] {
        let text = JsString::from("\x1b]11;") + JsString::from_utf16(units) + "\x07";
        let expected = oracle.request(&json!({"operation":"terminal_colors","units":text.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let color = parse_osc11_background_color(&text).map(|color| (color.r, color.g, color.b));
        let actual = serde_json::to_string(&(is_osc11_background_color_response(&text), color)).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap()
        );
    }
}
