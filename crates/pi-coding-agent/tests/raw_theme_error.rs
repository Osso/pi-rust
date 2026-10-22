use pi_coding_agent::modes::interactive::theme::ThemeError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
#[test]
fn theme_error_source_messages_retain_raw_fields_until_display() {
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let cases = [
            (ThemeError::Syntax { message: raw.clone() }, raw.clone()),
            (ThemeError::Io { message: raw.clone() }, raw.clone()),
            (
                ThemeError::Parse {
                    label: raw.clone(),
                    message: raw.clone(),
                },
                JsString::from("Failed to parse theme ") + &raw + ": " + &raw,
            ),
            (
                ThemeError::InvalidHexColor(raw.clone()),
                JsString::from("Invalid hex color: ") + &raw,
            ),
            (
                ThemeError::NotFound(raw.clone()),
                JsString::from("Theme not found: ") + &raw,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.message(), expected);
            assert_eq!(error.to_string(), encode_node_utf8(&expected));
        }
    }
}
