//! Frozen utils/oauth/load.ts and providers/openrouter-images.ts public contracts.
use crate::shared_oracle as oracle;

use pi_ai::auth::OAuthCredential;
use serde_json::json;

#[test]
fn openrouter_image_provider_auth_matches_real_frozen_factory() {
    let mut source = oracle::TsOracle::spawn("oauth-load-contract-oracle.mts", "packages/ai/src/utils/oauth/load.ts");
    let expected = source.request(&json!({"operation":"images"}));
    let provider = pi_ai::providers::openrouter_images::openrouter_images_provider();
    let auth = provider.auth();
    let mut auth_keys = Vec::new();
    if auth.api_key.is_some() {
        auth_keys.push("apiKey");
    }
    if auth.oauth.is_some() {
        auth_keys.push("oauth");
    }
    let actual = json!({"id":provider.id(),"name":provider.name(),"authKeys":auth_keys,
        "apiKeyName":auth.api_key.as_ref().unwrap().name(),"oauth":auth.oauth.as_ref().map(|flow|flow.name())});
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn frozen_three_loaders_return_current_auth_records() {
    use pi_ai::auth::oauth::load::{load_anthropic_oauth, load_github_copilot_oauth, load_openai_codex_oauth};
    let mut source = oracle::TsOracle::spawn("oauth-load-contract-oracle.mts", "packages/ai/src/utils/oauth/load.ts");
    let expected = source.request(&json!({"operation":"loaders"}));
    let flows = [
        ("loadAnthropicOAuth", load_anthropic_oauth()),
        ("loadGitHubCopilotOAuth", load_github_copilot_oauth()),
        ("loadOpenAICodexOAuth", load_openai_codex_oauth()),
    ];
    let mut actual = Vec::new();
    for (name, flow) in flows {
        let result = flow
            .to_auth(OAuthCredential::new("r", "plain-token", 10.0))
            .await
            .unwrap();
        actual.push(
            json!({"name":name,"authName":flow.name(),"authKeys":["login","name","refresh","toAuth"],"result":result}),
        );
    }
    assert_eq!(json!(actual), expected);
}
