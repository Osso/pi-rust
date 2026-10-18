pub const OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH: usize = 64;

pub fn clamp_openai_prompt_cache_key(key: Option<&str>) -> Option<&str> {
    let key = key?;
    match key.char_indices().nth(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH) {
        Some((byte_index, _)) => Some(&key[..byte_index]),
        None => Some(key),
    }
}
