use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::text::Text;
use pi_tui::tui::Component;
use serde_json::json;
use std::rc::Rc;

#[test]
fn text_native_lines_cache_and_background_callback_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0x61, 0xd800, 0x20, 0xdc00, 0x62, 10, 0xd83d, 0xde00],
    ] {
        for width in 1..=8 {
            for padding_x in [0, 1] {
                for padding_y in [0, 1] {
                    for background in [false, true] {
                        let response = oracle.request(&json!({"operation":"text", "units":units, "width":width, "paddingX":padding_x, "paddingY":padding_y, "background":background}));
                        assert_eq!(response["ok"], true, "{response}");
                        let mut text = Text::new(JsString::from_utf16(units.clone()), padding_x, padding_y);
                        if background {
                            text.set_custom_bg_fn(Some(Rc::new(|value| {
                                JsString::from("\x1b[44m") + value + "\x1b[0m"
                            })));
                        }
                        let actual = [text.render(width), text.render(width)];
                        let actual = parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap();
                        let expected = parse_lossless(response["json"].as_str().unwrap()).unwrap();
                        assert_eq!(
                            actual, expected,
                            "units={units:?} width={width} padding={padding_x},{padding_y} bg={background}"
                        );
                    }
                }
            }
        }
    }
}
