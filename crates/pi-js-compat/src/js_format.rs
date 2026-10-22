//! `format!` for JavaScript strings, so template literals keep unmatched
//! UTF-16 units from their interpolated values.

use crate::js_json::JsString;

/// Counts `{}` placeholders; `js_format!` checks this against its arguments at compile time.
/// Any other brace is rejected, so `format!`-style named captures such as `{path}` cannot
/// silently survive as literal text.
#[doc(hidden)]
pub const fn placeholder_count(template: &str) -> usize {
    let bytes = template.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{' && index + 1 < bytes.len() && bytes[index + 1] == b'}' {
            count += 1;
            index += 2;
        } else if bytes[index] == b'{' || bytes[index] == b'}' {
            panic!("js_format! templates only support `{{}}` placeholders");
        } else {
            index += 1;
        }
    }
    count
}

/// A value `js_format!` can interpolate without taking ownership of it.
pub trait FormatArgument {
    fn to_js_string(&self) -> JsString;
}

impl FormatArgument for JsString {
    fn to_js_string(&self) -> JsString {
        self.clone()
    }
}

impl FormatArgument for str {
    fn to_js_string(&self) -> JsString {
        self.into()
    }
}

impl FormatArgument for String {
    fn to_js_string(&self) -> JsString {
        self.into()
    }
}

impl FormatArgument for char {
    fn to_js_string(&self) -> JsString {
        (*self).into()
    }
}

/// Integer counts and ids interpolate as JavaScript `${n}` does for safe integers: plain decimal.
macro_rules! integer_format_argument {
    ($($integer:ty),*) => {$(
        impl FormatArgument for $integer {
            fn to_js_string(&self) -> JsString {
                self.to_string().into()
            }
        }
    )*};
}

integer_format_argument!(u8, i32, i64, u32, u64, usize);

impl<T: FormatArgument + ?Sized> FormatArgument for &T {
    fn to_js_string(&self) -> JsString {
        (**self).to_js_string()
    }
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

/// Builds a [`JsString`] from a template whose `{}` placeholders take
/// [`FormatArgument`] values by reference. There are no format specs or escapes.
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
            [$($crate::js_format::FormatArgument::to_js_string(&$argument)),*],
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
    fn borrows_field_arguments() {
        struct Row {
            path: JsString,
            id: String,
        }
        let row = Row {
            path: JsString::from_utf16(vec![0xdc00]),
            id: "7".to_owned(),
        };
        let formatted = js_format!("{}#{}", row.path, row.id);
        assert_eq!(formatted.units(), [0xdc00, 0x23, 0x37]);
        assert_eq!(row.id, "7");
    }

    #[test]
    fn interpolates_integers_as_javascript_template_numbers() {
        let count: usize = 3;
        assert_eq!(js_format!("{} of {} ({})", count, 10u32, -7i64), "3 of 10 (-7)");
        assert_eq!(js_format!("{}", 0u64), "0");
        let output_pad: u8 = 2;
        assert_eq!(js_format!("pad {}", output_pad), "pad 2");
    }

    #[test]
    #[should_panic(expected = "only support `{}` placeholders")]
    fn rejects_named_captures() {
        crate::js_format::placeholder_count("{path}.tmp");
    }

    #[test]
    #[should_panic(expected = "only support `{}` placeholders")]
    fn rejects_unpaired_closing_brace() {
        crate::js_format::placeholder_count("x}");
    }

    #[test]
    fn formats_templates_without_placeholders() {
        assert_eq!(js_format!("plain"), "plain");
        assert_eq!(js_format!("{}", String::from("only")), "only");
    }
}
