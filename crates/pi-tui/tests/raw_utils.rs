use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::utils::{
    extract_segments, normalize_terminal_output, slice_by_column, truncate_to_width, visible_width, wrap_text_with_ansi,
};
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Segments {
    before: JsString,
    before_width: usize,
    after: JsString,
    after_width: usize,
}

#[derive(Serialize)]
struct Output {
    width: usize,
    wrap: Vec<JsString>,
    truncate: JsString,
    slice: JsString,
    segments: Segments,
    normalize: JsString,
}

#[test]
fn raw_ansi_width_wrap_slice_and_truncate_match_native_source_strings() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let mut corpus = vec![
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0x61, 0xd800, 0x62, 0xdc00, 0x63],
        vec![0xd800, 0x301, 0x61, 0xdc00, 0x0e33],
        vec![0xd800, 0xd83d, 0xde00, 0xdc00],
    ];
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0x20, 0xdc00, 0xd83d, 0xde00, 0x62]);
    corpus.push((JsString::from("\x1b[31m") + &raw + "\x1b[0m").units().to_vec());
    corpus.push(
        (JsString::from("\x1b]8;;https://host/") + &raw + "\x07" + &raw + "\x1b]8;;\x07")
            .units()
            .to_vec(),
    );
    for units in corpus {
        let text = JsString::from_utf16(units.clone());
        for width in 0..=5 {
            for start in 0..=3 {
                let response =
                    oracle.request(&json!({"operation":"utils", "units": units, "width":width, "start":start}));
                assert_eq!(response["ok"], true, "{response}");
                let expected = parse_lossless(response["json"].as_str().unwrap()).unwrap();
                let segments = extract_segments(&text, start, start + 1, width, true);
                let actual = Output {
                    width: visible_width(&text),
                    wrap: wrap_text_with_ansi(&text, width),
                    truncate: truncate_to_width(&text, width),
                    slice: slice_by_column(&text, start, width, true),
                    segments: Segments {
                        before: segments.before,
                        before_width: segments.before_width,
                        after: segments.after,
                        after_width: segments.after_width,
                    },
                    normalize: normalize_terminal_output(&text),
                };
                let actual = parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap();
                assert_eq!(actual, expected, "units={units:?} width={width} start={start}");
            }
        }
    }
}
