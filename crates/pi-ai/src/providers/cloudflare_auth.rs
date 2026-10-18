//! Port of `packages/ai/src/providers/cloudflare-auth.ts`.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::auth::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthError, AuthInteraction, AuthPrompt,
    AuthPromptKind, AuthResult, ModelAuth, ProviderAuth,
};
use crate::request::BoxFuture;
use crate::types::{ProviderEnv, ProviderHeaders};

const CLOUDFLARE_API_KEY: &str = "CLOUDFLARE_API_KEY";
const CLOUDFLARE_ACCOUNT_ID: &str = "CLOUDFLARE_ACCOUNT_ID";
const CLOUDFLARE_GATEWAY_ID: &str = "CLOUDFLARE_GATEWAY_ID";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudflareAuthKind {
    WorkersAi,
    AiGateway,
}

struct ResolvedCloudflareEnv {
    api_key: String,
    env: ProviderEnv,
    source: String,
}

/// Per-field merge: prefer the credential value, fall back to ambient env.
async fn resolve_value(
    name: &str,
    ctx: &Arc<dyn AuthContext>,
    credential: Option<&ApiKeyCredential>,
    signal: &CancellationToken,
) -> Result<Option<String>, AuthError> {
    let from_credential = credential.and_then(|credential| {
        if name == CLOUDFLARE_API_KEY {
            credential.key.clone()
        } else {
            credential.env.as_ref().and_then(|env| env.get(name).cloned())
        }
    });
    if from_credential.is_some() {
        return Ok(from_credential);
    }
    check_aborted(signal)?;
    let value = ctx.env(name).await;
    check_aborted(signal)?;
    Ok(value)
}

async fn resolve_cloudflare_env(
    kind: CloudflareAuthKind,
    ctx: &Arc<dyn AuthContext>,
    credential: Option<&ApiKeyCredential>,
    signal: &CancellationToken,
) -> Result<Option<ResolvedCloudflareEnv>, AuthError> {
    let api_key = resolve_value(CLOUDFLARE_API_KEY, ctx, credential, signal).await?;
    let account_id = resolve_value(CLOUDFLARE_ACCOUNT_ID, ctx, credential, signal).await?;
    let gateway_id = if kind == CloudflareAuthKind::AiGateway {
        resolve_value(CLOUDFLARE_GATEWAY_ID, ctx, credential, signal).await?
    } else {
        None
    };
    let (Some(api_key), Some(account_id)) = (api_key, account_id) else {
        return Ok(None);
    };
    if kind == CloudflareAuthKind::AiGateway && gateway_id.is_none() {
        return Ok(None);
    }
    let mut env = ProviderEnv::from([(CLOUDFLARE_ACCOUNT_ID.to_owned(), account_id)]);
    if let Some(gateway_id) = gateway_id {
        env.insert(CLOUDFLARE_GATEWAY_ID.to_owned(), gateway_id);
    }
    Ok(Some(ResolvedCloudflareEnv {
        api_key,
        env,
        source: if credential.is_some() {
            "stored credential".to_owned()
        } else {
            CLOUDFLARE_API_KEY.to_owned()
        },
    }))
}

fn check_aborted(signal: &CancellationToken) -> Result<(), AuthError> {
    if signal.is_cancelled() {
        Err(AuthError::Aborted)
    } else {
        Ok(())
    }
}

async fn prompt(interaction: &Arc<dyn AuthInteraction>, kind: AuthPromptKind) -> Result<String, AuthError> {
    interaction
        .prompt(AuthPrompt { signal: None, kind })
        .await
        .map_err(AuthError::Failed)
}

fn secret(message: &str) -> AuthPromptKind {
    AuthPromptKind::Secret {
        message: message.to_owned(),
        placeholder: None,
    }
}

fn text(message: &str) -> AuthPromptKind {
    AuthPromptKind::Text {
        message: message.to_owned(),
        placeholder: None,
    }
}

/// `cloudflareWorkersAIAuth()`.
pub struct CloudflareWorkersAiAuth;

impl ApiKeyAuth for CloudflareWorkersAiAuth {
    fn name(&self) -> &str {
        "Cloudflare API key"
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        Some(Box::pin(async move {
            let key = prompt(&interaction, secret("Enter Cloudflare API key")).await?;
            let account_id = prompt(&interaction, text("Enter Cloudflare account ID")).await?;
            Ok(ApiKeyCredential {
                key: Some(key),
                env: Some(ProviderEnv::from([(CLOUDFLARE_ACCOUNT_ID.to_owned(), account_id)])),
            })
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            let resolved = resolve_cloudflare_env(
                CloudflareAuthKind::WorkersAi,
                &input.ctx,
                input.credential.as_ref(),
                &input.signal,
            )
            .await?;
            Ok(resolved.map(|resolved| AuthResult {
                auth: ModelAuth {
                    api_key: Some(resolved.api_key),
                    headers: None,
                    base_url: None,
                },
                env: Some(resolved.env),
                source: Some(resolved.source),
            }))
        })
    }
}

/// `cloudflareAIGatewayAuth()`.
pub struct CloudflareAiGatewayAuth;

impl ApiKeyAuth for CloudflareAiGatewayAuth {
    fn name(&self) -> &str {
        "Cloudflare API key"
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        Some(Box::pin(async move {
            let key = prompt(&interaction, secret("Enter Cloudflare API key")).await?;
            let account_id = prompt(&interaction, text("Enter Cloudflare account ID")).await?;
            let gateway_id = prompt(&interaction, text("Enter Cloudflare AI Gateway ID")).await?;
            Ok(ApiKeyCredential {
                key: Some(key),
                env: Some(ProviderEnv::from([
                    (CLOUDFLARE_ACCOUNT_ID.to_owned(), account_id),
                    (CLOUDFLARE_GATEWAY_ID.to_owned(), gateway_id),
                ])),
            })
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            let resolved = resolve_cloudflare_env(
                CloudflareAuthKind::AiGateway,
                &input.ctx,
                input.credential.as_ref(),
                &input.signal,
            )
            .await?;
            Ok(resolved.map(|resolved| {
                let mut headers = ProviderHeaders::new();
                headers.insert(
                    "cf-aig-authorization".to_owned(),
                    Some(format!("Bearer {}", resolved.api_key)),
                );
                headers.insert("Authorization".to_owned(), None);
                headers.insert("x-api-key".to_owned(), None);
                AuthResult {
                    auth: ModelAuth {
                        api_key: None,
                        headers: Some(headers),
                        base_url: None,
                    },
                    env: Some(resolved.env),
                    source: Some(resolved.source),
                }
            }))
        })
    }
}

/// Shared auth constructor for tests.
pub fn workers_ai_auth() -> ProviderAuth {
    ProviderAuth::api_key(Arc::new(CloudflareWorkersAiAuth))
}

pub fn ai_gateway_auth() -> ProviderAuth {
    ProviderAuth::api_key(Arc::new(CloudflareAiGatewayAuth))
}
