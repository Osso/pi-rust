use pi_ai::api::openai_prompt_cache::{OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH, clamp_openai_prompt_cache_key};
use pi_js_compat::js_json::JsString;

#[test]
fn preserves_missing_and_short_keys() {
    assert_eq!(clamp_openai_prompt_cache_key(None), None).into();
    assert_eq!(clamp_openai_prompt_cache_key(Some(&"".into())), Some("".into())).into();
    assert_eq!(
        clamp_openai_prompt_cache_key(Some(&"prompt-cache-key".into())),
        Some("prompt-cache-key".into())
    );
}

#[test]
fn clamps_keys_longer_than_the_limit() {
    let exact: JsString = "a".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH).into();
    assert_eq!(clamp_openai_prompt_cache_key(Some(&exact)), Some(exact.clone()));

    let long: JsString = "a".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 1).into();
    assert_eq!(clamp_openai_prompt_cache_key(Some(&long)), Some(exact.clone()));
}

#[test]
fn counts_code_points_not_utf16_units_or_bytes() {
    let emoji: JsString = "🙈".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH).into();
    assert_eq!(clamp_openai_prompt_cache_key(Some(&emoji)), Some(emoji.clone()));

    let long_emoji: JsString = "🙈".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 1).into();
    assert_eq!(clamp_openai_prompt_cache_key(Some(&long_emoji)), Some(emoji.clone()));

    let accented: JsString = "é".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 5).into();
    assert_eq!(
        clamp_openai_prompt_cache_key(Some(&accented)),
        Some("é".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH).into())
    );
}
