use pi_js_compat::js_json::JsString;
pub const OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH: usize = 64;

pub fn clamp_openai_prompt_cache_key(key: Option<&JsString>) -> Option<JsString> {
    key.map(|key| {
        // Array.from(string) groups valid pairs and keeps unmatched units as entries.
        let end = char::decode_utf16(key.units().iter().copied())
            .take(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH)
            .map(|point| point.map_or(1, char::len_utf16))
            .sum();
        key.slice(0, end)
    })
}
