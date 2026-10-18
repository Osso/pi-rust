//! Port of `packages/ai/src/providers/amazon-bedrock.ts`.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::auth::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthError, AuthEvent, AuthInfoLink, AuthInteraction,
    AuthPrompt, AuthPromptKind, AuthResult, AuthSelectOption, ModelAuth, ProviderAuth,
};
use crate::bedrock_provider::bedrock_converse_stream_api;
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};
use crate::request::BoxFuture;

const AUTH_NAME: &str = "AWS credentials or bearer token";

/// `bedrockAuth`.
struct BedrockAuth;

impl ApiKeyAuth for BedrockAuth {
    fn name(&self) -> &str {
        AUTH_NAME
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        Some(Box::pin(async move { login(&interaction).await }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move { resolve(&input).await })
    }
}

async fn login(interaction: &Arc<dyn AuthInteraction>) -> Result<ApiKeyCredential, AuthError> {
    let signal = interaction.signal();
    check_aborted(&signal)?;
    let method = interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Select {
                message: "Select Amazon Bedrock authentication method:".to_owned(),
                options: vec![
                    AuthSelectOption {
                        id: "bearer-token".to_owned(),
                        label: "Bearer token".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "aws-profile".to_owned(),
                        label: "AWS profile".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "credential-chain".to_owned(),
                        label: "Existing AWS credential chain".to_owned(),
                        description: None,
                    },
                ],
            },
        })
        .await
        .map_err(AuthError::Failed)?;
    check_aborted(&signal)?;

    if method == "bearer-token" {
        let key = interaction
            .prompt(AuthPrompt {
                signal: None,
                kind: AuthPromptKind::Secret {
                    message: "Enter Amazon Bedrock bearer token".to_owned(),
                    placeholder: None,
                },
            })
            .await
            .map_err(AuthError::Failed)?;
        return Ok(ApiKeyCredential {
            key: Some(key),
            env: None,
        });
    }

    interaction.notify(AuthEvent::Info {
        message: "Amazon Bedrock supports AWS profiles, IAM credentials, and role-based credentials.".to_owned(),
        links: Some(vec![AuthInfoLink {
            url: "https://docs.aws.amazon.com/sdkref/latest/guide/standardized-credentials.html".to_owned(),
            label: Some("AWS credential provider chain".to_owned()),
        }]),
    });

    if method == "aws-profile" {
        let profile = interaction
            .prompt(AuthPrompt {
                signal: None,
                kind: AuthPromptKind::Text {
                    message: "Enter AWS profile name".to_owned(),
                    placeholder: None,
                },
            })
            .await
            .map_err(AuthError::Failed)?;
        return Ok(ApiKeyCredential {
            key: None,
            env: Some([("AWS_PROFILE".to_owned(), profile)].into_iter().collect()),
        });
    }

    if method != "credential-chain" {
        return Err(AuthError::failed(format!(
            "Unknown Amazon Bedrock auth method: {method}"
        )));
    }

    interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Text {
                message: "Configure AWS credentials, then press Enter to continue".to_owned(),
                placeholder: None,
            },
        })
        .await
        .map_err(AuthError::Failed)?;
    Ok(ApiKeyCredential { key: None, env: None })
}

async fn resolve(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    if input
        .credential
        .as_ref()
        .and_then(|credential| credential.key.as_ref())
        .is_some()
    {
        return Ok(Some(AuthResult {
            auth: ModelAuth::default(),
            env: input.credential.as_ref().and_then(|credential| credential.env.clone()),
            source: Some("stored credential".to_owned()),
        }));
    }

    let credential_env = input.credential.as_ref().and_then(|credential| credential.env.as_ref());
    let stored_profile = credential_env
        .and_then(|env| env.get("AWS_PROFILE"))
        .filter(|value| !value.is_empty())
        .cloned();

    if env_value(&input.ctx, "AWS_BEARER_TOKEN_BEDROCK", &input.signal)
        .await?
        .is_some()
    {
        return Ok(Some(AuthResult {
            auth: ModelAuth::default(),
            env: None,
            source: Some("AWS_BEARER_TOKEN_BEDROCK".to_owned()),
        }));
    }

    if stored_profile.is_some() || env_value(&input.ctx, "AWS_PROFILE", &input.signal).await?.is_some() {
        return Ok(Some(AuthResult {
            auth: ModelAuth::default(),
            env: credential_env.cloned(),
            source: Some(if stored_profile.is_some() {
                "stored credential".to_owned()
            } else {
                "AWS_PROFILE".to_owned()
            }),
        }));
    }

    let access_key = env_value(&input.ctx, "AWS_ACCESS_KEY_ID", &input.signal).await?;
    if access_key.is_some()
        && env_value(&input.ctx, "AWS_SECRET_ACCESS_KEY", &input.signal)
            .await?
            .is_some()
    {
        return Ok(Some(AuthResult {
            auth: ModelAuth::default(),
            env: None,
            source: Some("AWS access keys".to_owned()),
        }));
    }

    for (name, source) in [
        ("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "ECS task role"),
        ("AWS_CONTAINER_CREDENTIALS_FULL_URI", "ECS task role"),
        ("AWS_WEB_IDENTITY_TOKEN_FILE", "web identity token"),
    ] {
        if env_value(&input.ctx, name, &input.signal).await?.is_some() {
            return Ok(Some(AuthResult {
                auth: ModelAuth::default(),
                env: None,
                source: Some(source.to_owned()),
            }));
        }
    }

    Ok(None)
}

async fn env_value(
    ctx: &Arc<dyn AuthContext>,
    name: &str,
    signal: &CancellationToken,
) -> Result<Option<String>, AuthError> {
    check_aborted(signal)?;
    let value = ctx.env(name).await;
    check_aborted(signal)?;
    Ok(value.filter(|value| !value.is_empty()))
}

fn check_aborted(signal: &CancellationToken) -> Result<(), AuthError> {
    if signal.is_cancelled() {
        Err(AuthError::Aborted)
    } else {
        Ok(())
    }
}

/// `amazonBedrockProvider()`.
pub fn amazon_bedrock_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "amazon-bedrock",
            name: "Amazon Bedrock",
            base_url: None,
            auth: ProviderAuth::api_key(Arc::new(BedrockAuth)),
            filter_models: None,
        },
        single_api(bedrock_converse_stream_api()),
    )
}
