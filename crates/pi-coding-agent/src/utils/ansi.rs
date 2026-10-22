use std::sync::OnceLock;

use regex::Regex;

fn ansi_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        // Valid string terminator sequences are BEL, ESC\, and 0x9c.
        // OSC sequences only: ESC ] ... ST (non-greedy until the first ST).
        // CSI and related: ESC/C1, optional intermediates, optional params then a final byte.
        Regex::new(
            r"(?:\x1B\][\s\S]*?(?:\x07|\x1B\\|\x{9C}))|[\x1B\x{9B}][\[\]()#;?]*(?:[0-9]{1,4}(?:[;:][0-9]{0,4})*)?[0-9A-PR-TZcf-nq-uy=><~]",
        )
        .expect("the ANSI compatibility pattern must compile")
    })
}

/// Strip ANSI escape sequences using the same pattern as the pinned `ansi-regex` port.
pub fn strip_ansi(value: &str) -> String {
    if !value.contains('\u{1b}') && !value.contains('\u{9b}') {
        return value.to_owned();
    }

    ansi_regex().replace_all(value, "").into_owned()
}

/// Strip the same ANSI ranges without decoding or replacing source content.
pub fn strip_ansi_js(value: &pi_js_compat::js_json::JsString) -> pi_js_compat::js_json::JsString {
    use pi_js_compat::js_json::JsString;
    // ANSI syntax tests only ASCII/C1 units; OSC payloads accept every unit.
    // One scalar per unit gives the shared regex a view, never a content authority.
    const SURROGATE_MATCH_UNIT: char = '\u{e000}';
    let view: String = value
        .units()
        .iter()
        .map(|unit| char::from_u32(u32::from(*unit)).unwrap_or(SURROGATE_MATCH_UNIT))
        .collect();
    let mut offsets: Vec<usize> = view.char_indices().map(|(byte, _)| byte).collect();
    offsets.push(view.len());
    let mut output = JsString::default();
    let mut previous = 0;
    for matched in ansi_regex().find_iter(&view) {
        let start = offsets.binary_search(&matched.start()).expect("match starts on a unit");
        let end = offsets.binary_search(&matched.end()).expect("match ends on a unit");
        output.push_str(value.slice(previous, start));
        previous = end;
    }
    output.push_str(value.slice(previous, value.len()));
    output
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;
    #[test]
    fn ansi_removal_preserves_distinct_surrogates_and_pairs() {
        let content = JsString::from_utf16(vec![0xd800, 0xd801, 0xd83d, 0xde00, 0xfffd]);
        let decorated = JsString::from("\x1b[31m") + &content + "\x1b[0m";
        assert_eq!(strip_ansi_js(&decorated), content);
        let osc = JsString::from("\x1b]0;") + JsString::from_utf16(vec![0xd800]) + "\x07" + &content;
        assert_eq!(strip_ansi_js(&osc), content);
    }
}
