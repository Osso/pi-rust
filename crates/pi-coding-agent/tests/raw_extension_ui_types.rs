use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{TerminalInputHandler, TerminalInputResult, WorkingIndicatorOptions};
use pi_js_compat::js_json::{JsString, parse_lossless};

#[test]
fn terminal_input_callback_and_working_frames_preserve_native_units() {
    let handler: TerminalInputHandler = Arc::new(|data| {
        Some(TerminalInputResult {
            consume: Some(false),
            data: Some(data.clone()),
        })
    });
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        assert_eq!(handler(&raw).unwrap().data, Some(raw.clone()));
        let options = WorkingIndicatorOptions {
            frames: Some(vec![raw.clone()]),
            interval_ms: Some(80.0),
        };
        let json = serde_json::to_string(&options).unwrap();
        let value = parse_lossless(&json).unwrap();
        assert_eq!(value["frames"][0].as_string(), Some(&raw));
    }
}
