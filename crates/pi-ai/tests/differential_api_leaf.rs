#[path = "differential_api_leaf/mod.rs"]
mod api_leaf_oracle;

#[test]
fn cloudflare_constants_and_prompt_cache_clamp_match_the_pinned_typescript_source() {
    api_leaf_oracle::run();
}
