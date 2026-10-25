use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use pi_js_compat::js_json::{JsString, parse_lossless};
use serde_json::json;
#[test]
fn raw_visual_truncation_matches_frozen_source_including_zero_limit() {
    let mut oracle = oracle::TsOracle::spawn("raw-visual-truncate-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let text = raw.clone() + " first\nsecond " + &raw + "\nlast";
        for width in [0, 1, 8] {
            for padding in [0, 1] {
                for limit in [0, 1, 5] {
                    let expected =
                        oracle.request(&json!({"units":text.units(),"width":width,"padding":padding,"limit":limit}));
                    assert_eq!(expected["ok"], true, "{expected}");
                    let actual = truncate_to_visual_lines(&text, limit, width, padding);
                    let actual = serde_json::to_string(&(actual.visual_lines, actual.skipped_count)).unwrap();
                    assert_eq!(
                        parse_lossless(actual).unwrap(),
                        parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                        "raw={raw:?} width={width} padding={padding} limit={limit}"
                    );
                }
            }
        }
    }
}
