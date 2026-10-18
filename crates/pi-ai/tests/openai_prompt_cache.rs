use pi_ai::api::openai_prompt_cache::{OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH, clamp_openai_prompt_cache_key};

#[test]
fn preserves_missing_and_short_keys() {
    assert_eq!(clamp_openai_prompt_cache_key(None), None);
    assert_eq!(clamp_openai_prompt_cache_key(Some("")), Some(""));
    assert_eq!(
        clamp_openai_prompt_cache_key(Some("prompt-cache-key")),
        Some("prompt-cache-key")
    );
}

#[test]
fn clamps_keys_longer_than_the_limit() {
    let exact = "a".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH);
    assert_eq!(clamp_openai_prompt_cache_key(Some(&exact)), Some(exact.as_str()));

    let long = "a".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 1);
    assert_eq!(clamp_openai_prompt_cache_key(Some(&long)), Some(exact.as_str()));
}

#[test]
fn counts_code_points_not_utf16_units_or_bytes() {
    let emoji = "🙈".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH);
    assert_eq!(clamp_openai_prompt_cache_key(Some(&emoji)), Some(emoji.as_str()));

    let long_emoji = "🙈".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 1);
    assert_eq!(clamp_openai_prompt_cache_key(Some(&long_emoji)), Some(emoji.as_str()));

    let accented = "é".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 5);
    assert_eq!(
        clamp_openai_prompt_cache_key(Some(&accented)),
        Some("é".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH).as_str())
    );
}
