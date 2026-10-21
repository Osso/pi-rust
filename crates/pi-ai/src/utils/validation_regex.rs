//! TypeBox uses Unicode RegExp for pattern/patternProperties, default RegExp for conversion/additionalProperties.
use pi_js_compat::js_json::JsString;

pub(super) struct JsRegex {
    regex: regress::Regex,
    unicode: bool,
}

impl JsRegex {
    pub(super) fn new(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "")
    }

    pub(super) fn unicode(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "u")
    }

    pub(super) fn with_flags(pattern: impl Into<JsString>, flags: &str) -> Result<Self, regress::Error> {
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

    pub(super) fn is_match(&self, text: impl Into<JsString>) -> bool {
        let text = text.into();
        if self.unicode {
            self.regex.find_from_utf16(text.units(), 0).next().is_some()
        } else {
            self.regex.find_from_ucs2(text.units(), 0).next().is_some()
        }
    }
}
