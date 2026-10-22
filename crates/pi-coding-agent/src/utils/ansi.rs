use std::sync::OnceLock;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::JsRegex;

fn ansi_regex() -> &'static JsRegex {
    static REGEX: OnceLock<JsRegex> = OnceLock::new();
    REGEX.get_or_init(|| {
        // OSC payloads accept every UTF-16 unit until BEL, ESC\\ or C1 ST.
        JsRegex::new(
            r"(?:\x1B\][\s\S]*?(?:\x07|\x1B\\|\u009C))|[\x1B\u009B][\[\]()#;?]*(?:[0-9]{1,4}(?:[;:][0-9]{0,4})*)?[0-9A-PR-TZcf-nq-uy=><~]",
        )
        .expect("the ANSI compatibility pattern must compile")
    })
}

/// Strip ANSI escape sequences without projecting the remaining JS string.
pub fn strip_ansi(value: &JsString) -> JsString {
    if !value.contains('\u{1b}') && !value.contains('\u{9b}') {
        return value.clone();
    }
    ansi_regex().replace_all(value, "")
}

#[cfg(test)]
mod lossless_tests {
    use super::*;

    #[test]
    fn ansi_removal_preserves_distinct_surrogates_and_pairs() {
        let content = JsString::from_utf16(vec![0xd800, 0xd801, 0xd83d, 0xde00, 0xfffd]);
        let decorated = JsString::from("\x1b[31m") + &content + "\x1b[0m";
        assert_eq!(strip_ansi(&decorated), content);
        let osc = JsString::from("\x1b]0;") + JsString::from_utf16(vec![0xd800]) + "\x07" + &content;
        assert_eq!(strip_ansi(&osc), content);
    }
}
