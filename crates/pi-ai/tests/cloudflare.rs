use pi_ai::api::cloudflare::{
    CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL, CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL,
    CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL, CLOUDFLARE_WORKERS_AI_BASE_URL,
};

#[test]
fn exports_the_documented_cloudflare_endpoints() {
    assert_eq!(
        CLOUDFLARE_WORKERS_AI_BASE_URL,
        "https://api.cloudflare.com/client/v4/accounts/{CLOUDFLARE_ACCOUNT_ID}/ai/v1"
    );
    assert_eq!(
        CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL,
        "https://gateway.ai.cloudflare.com/v1/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}/compat"
    );
    assert_eq!(
        CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL,
        "https://gateway.ai.cloudflare.com/v1/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}/openai"
    );
    assert_eq!(
        CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL,
        "https://gateway.ai.cloudflare.com/v1/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}/anthropic"
    );
}
