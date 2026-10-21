//! ECMAScript's default (non-Unicode) RegExp operates on individual UTF-16 units.
use pi_js_compat::js_json::JsString;

pub(super) struct JsRegex(regress::Regex);

impl JsRegex {
    pub(super) fn new(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        let pattern = pattern.into();
        regress::Regex::from_unicode(
            pattern.units().iter().copied().map(u32::from),
            regress::Flags::default(),
        )
        .map(Self)
    }

    pub(super) fn is_match(&self, text: impl Into<JsString>) -> bool {
        self.0.find_from_ucs2(text.into().units(), 0).next().is_some()
    }
}
