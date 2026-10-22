use pi_js_compat::js_json::JsString;
pub const OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH: usize = 64;

pub fn clamp_openai_prompt_cache_key(key: Option<&JsString>) -> Option<JsString> {
    key.map(|key| key.slice(0, key.len().min(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH)))
}
