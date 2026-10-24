//! Port of `packages/ai/src/providers/amazon-bedrock.ts`.

use std::sync::Arc;

use crate::auth::{ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthResult, ModelAuth, ProviderAuth};
use crate::bedrock_provider::bedrock_converse_stream_api;
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};
use crate::request::BoxFuture;

struct BedrockAuth;

impl ApiKeyAuth for BedrockAuth {
    fn name(&self) -> pi_js_compat::js_json::JsString {
        "AWS credentials".into()
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move { resolve(&input).await })
    }
}

async fn resolve(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    if let Some(key) = input.credential.as_ref().and_then(|credential| credential.key.as_ref())
        && !key.is_empty()
    {
        return Ok(Some(AuthResult {
            auth: ModelAuth {
                api_key: Some(key.clone()),
                ..Default::default()
            },
            env: None,
            source: Some("stored credential".into()),
        }));
    }
    let source = read_ambient_source(input).await?;
    Ok(source.map(|source| AuthResult {
        auth: ModelAuth::default(),
        env: None,
        source: Some(source.into()),
    }))
}

async fn read_ambient_source(input: &ApiKeyResolveInput) -> Result<Option<&'static str>, AuthError> {
    for name in ["AWS_BEARER_TOKEN_BEDROCK", "AWS_PROFILE"] {
        if env_present(input, name).await? {
            return Ok(Some(name));
        }
    }
    if env_present(input, "AWS_ACCESS_KEY_ID").await? && env_present(input, "AWS_SECRET_ACCESS_KEY").await? {
        return Ok(Some("AWS access keys"));
    }
    for (name, source) in [
        ("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "ECS task role"),
        ("AWS_CONTAINER_CREDENTIALS_FULL_URI", "ECS task role"),
        ("AWS_WEB_IDENTITY_TOKEN_FILE", "web identity token"),
    ] {
        if env_present(input, name).await? {
            return Ok(Some(source));
        }
    }
    Ok(None)
}

async fn env_present(input: &ApiKeyResolveInput, name: &str) -> Result<bool, AuthError> {
    Ok(input.ctx.env(name).await?.is_some_and(|value| !value.is_empty()))
}

/// `amazonBedrockProvider()`.
pub fn amazon_bedrock_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "amazon-bedrock",
            name: "Amazon Bedrock",
            base_url: None,
            auth: ProviderAuth::api_key(Arc::new(BedrockAuth)),
        },
        single_api(bedrock_converse_stream_api()),
    )
}
