//! One-line tool argument summaries for agent activity displays.

use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::string::is_js_whitespace_unit;

const DETAIL_ARGUMENT_KEYS: [&str; 10] = [
    "command",
    "code",
    "path",
    "file_path",
    "pattern",
    "query",
    "url",
    "prompt",
    "message",
    "agentId",
];

pub const MAX_ACTIVITY_DETAIL_LENGTH: usize = 160;

pub fn summarize_tool_arguments(args: &JsValue) -> Option<JsString> {
    for key in DETAIL_ARGUMENT_KEYS {
        let Some(value) = args.get(key).and_then(JsValue::as_string) else {
            continue;
        };
        let single_line = collapse_whitespace(value);
        if single_line.is_empty() {
            continue;
        }
        return Some(if single_line.len() > MAX_ACTIVITY_DETAIL_LENGTH {
            single_line.slice(0, MAX_ACTIVITY_DETAIL_LENGTH - 1) + "…"
        } else {
            single_line
        });
    }
    None
}

fn collapse_whitespace(value: &JsString) -> JsString {
    let mut units = Vec::with_capacity(value.len());
    let mut pending_space = false;
    for unit in value.units() {
        if is_js_whitespace_unit(*unit) {
            pending_space = !units.is_empty();
        } else {
            if pending_space {
                units.push(u16::from(b' '));
                pending_space = false;
            }
            units.push(*unit);
        }
    }
    JsString::from_utf16(units)
}
