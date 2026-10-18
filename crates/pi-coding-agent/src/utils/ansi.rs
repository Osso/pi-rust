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
