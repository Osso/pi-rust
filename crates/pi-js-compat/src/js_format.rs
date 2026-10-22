//! `format!` for JavaScript strings, so template literals keep unmatched
//! UTF-16 units from their interpolated values.

use crate::js_json::JsString;

/// Counts `{}` placeholders; `js_format!` checks this against its arguments at compile time.
#[doc(hidden)]
pub const fn placeholder_count(template: &str) -> usize {
    let bytes = template.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'{' && bytes[index + 1] == b'}' {
            count += 1;
            index += 2;
        } else {
            index += 1;
        }
    }
    count
}

#[doc(hidden)]
pub fn format_template<const N: usize>(template: &str, arguments: [JsString; N]) -> JsString {
    let mut arguments = arguments.into_iter();
    let mut output = JsString::default();
    for (index, piece) in template.split("{}").enumerate() {
        if index > 0 {
            output.push_str(arguments.next().expect("placeholder count is checked at compile time"));
        }
        output.push_str(piece);
    }
    output
}

/// Builds a [`JsString`] from a template whose `{}` placeholders take values
/// convertible with `Into<JsString>`. There are no format specs or escapes.
#[macro_export]
macro_rules! js_format {
    ($template:literal $(, $argument:expr)* $(,)?) => {{
        const {
            assert!(
                $crate::js_format::placeholder_count($template) == <[&str]>::len(&[$(stringify!($argument)),*]),
                "js_format! placeholder and argument counts differ"
            )
        };
        $crate::js_format::format_template(
            $template,
            [$($crate::js_json::JsString::from($argument)),*],
        )
    }};
}

#[cfg(test)]
mod tests {
    use crate::js_json::JsString;

    #[test]
    fn interpolates_raw_units_in_order() {
        let lone = JsString::from_utf16(vec![0xd83d]);
        let formatted = js_format!("<{}|{}>", &lone, "x");
        assert_eq!(formatted.units(), [0x3c, 0xd83d, 0x7c, 0x78, 0x3e]);
    }

    #[test]
    fn formats_templates_without_placeholders() {
        assert_eq!(js_format!("plain"), "plain");
        assert_eq!(js_format!("{}", String::from("only")), "only");
    }
}
