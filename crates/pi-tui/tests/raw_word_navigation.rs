use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::word_navigation::{
    WordNavigationOptions, WordSegment, find_word_backward, find_word_forward, word_segments,
};
use serde_json::json;
use std::cell::RefCell;

#[test]
fn raw_word_segments_inside_pair_positions_and_callbacks_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from("abc") + JsString::from_utf16(vec![0xd800, 0x301]),
        JsString::from_utf16(vec![0xd800]) + "abc",
        JsString::from("😀word"),
        JsString::from("a") + JsString::from_utf16(vec![0xdc00, 0x301]) + "b",
    ] {
        let expected = oracle.request(&json!({"operation":"word_navigation","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let segments: Vec<_> = word_segments(&raw)
            .into_iter()
            .map(|segment| (segment.segment, segment.is_word_like))
            .collect();
        let positions: Vec<_> = (0..=raw.len())
            .map(|cursor| {
                (
                    find_word_backward(&raw, cursor, None),
                    find_word_forward(&raw, cursor, None),
                )
            })
            .collect();
        let seen = RefCell::new(Vec::new());
        let atomic = RefCell::new(Vec::new());
        let segment = |value: &JsString| {
            seen.borrow_mut().push(value.clone());
            vec![WordSegment {
                segment: value.clone(),
                is_word_like: true,
            }]
        };
        let is_atomic = |value: &JsString| {
            atomic.borrow_mut().push(value.clone());
            value.contains(JsString::from_utf16(vec![0xd800]))
        };
        let options = WordNavigationOptions {
            segment: Some(&segment),
            is_atomic_segment: Some(&is_atomic),
        };
        let custom = (
            find_word_backward(&raw, 1, Some(&options)),
            find_word_forward(&raw, 1, Some(&options)),
            seen.into_inner(),
            atomic.into_inner(),
        );
        let actual = serde_json::to_string(&(segments, positions, custom)).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}
