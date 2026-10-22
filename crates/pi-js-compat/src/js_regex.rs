//! ECMAScript RegExp over raw UTF-16 strings (Unicode and legacy UCS-2 modes).
use crate::js_json::JsString;

#[derive(Clone, Debug)]
pub struct JsRegex {
    regex: regress::Regex,
    unicode: bool,
}

impl JsRegex {
    pub fn new(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "")
    }

    pub fn unicode(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "u")
    }

    pub fn with_flags(pattern: impl Into<JsString>, flags: &str) -> Result<Self, regress::Error> {
        let pattern = pattern.into();
        let unicode = flags.contains('u');
        let points: Vec<u32> = if unicode {
            char::decode_utf16(pattern.units().iter().copied())
                .map(|point| {
                    point
                        .map(u32::from)
                        .unwrap_or_else(|error| u32::from(error.unpaired_surrogate()))
                })
                .collect()
        } else {
            pattern.units().iter().copied().map(u32::from).collect()
        };
        let regex = regress::Regex::from_unicode(points.into_iter(), flags)?;
        Ok(Self { regex, unicode })
    }

    pub fn is_match(&self, text: impl Into<JsString>) -> bool {
        let text = text.into();
        if self.unicode {
            self.regex.find_from_utf16(text.units(), 0).next().is_some()
        } else {
            self.regex.find_from_ucs2(text.units(), 0).next().is_some()
        }
    }
}
