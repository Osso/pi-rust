//! Port of `packages/ai/src/providers/google-vertex.ts`.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::api::google_vertex::google_vertex_api;
use crate::auth::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthError, AuthEvent, AuthInfoLink, AuthInteraction,
    AuthPrompt, AuthPromptKind, AuthResult, AuthSelectOption, ModelAuth, ProviderAuth,
};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};
use crate::request::BoxFuture;
use crate::types::ProviderEnv;

const VERTEX_ADC_PATH: &str = "~/.config/gcloud/application_default_credentials.json";

/// `vertexAuth`.
struct VertexAuth;

impl ApiKeyAuth for VertexAuth {
    fn name(&self) -> &str {
        "Google Cloud credentials"
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
                message: "Select Google Vertex AI authentication method:".to_owned(),
                options: vec![
                    AuthSelectOption {
                        id: "api-key".to_owned(),
                        label: "Google Cloud API key".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "adc".to_owned(),
                        label: "Application Default Credentials".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "service-account".to_owned(),
                        label: "Service account credentials file".to_owned(),
                        description: None,
                    },
                ],
            },
        })
        .await
        .map_err(AuthError::Failed)?;
    check_aborted(&signal)?;

    if method == "api-key" {
        let key = interaction
            .prompt(AuthPrompt {
                signal: None,
                kind: AuthPromptKind::Secret {
                    message: "Enter Google Cloud API key".to_owned(),
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

    if method != "adc" && method != "service-account" {
        return Err(AuthError::failed(format!(
            "Unknown Google Vertex AI auth method: {method}"
        )));
    }

    interaction.notify(AuthEvent::Info {
        message: if method == "adc" {
            "Run `gcloud auth application-default login`, then provide the project and location.".to_owned()
        } else {
            "Provide a service account credentials file, project, and location.".to_owned()
        },
        links: Some(vec![AuthInfoLink {
            url: "https://cloud.google.com/docs/authentication/provide-credentials-adc".to_owned(),
            label: Some("Application Default Credentials".to_owned()),
        }]),
    });

    let project = prompt_text(interaction, "Enter Google Cloud project ID").await?;
    let location = prompt_text(interaction, "Enter Google Cloud location").await?;
    let credentials_path = if method == "service-account" {
        Some(prompt_text(interaction, "Enter service account credentials file path").await?)
    } else {
        None
    };

    let mut env = std::collections::BTreeMap::from([
        ("GOOGLE_CLOUD_PROJECT".to_owned(), project),
        ("GOOGLE_CLOUD_LOCATION".to_owned(), location),
    ]);
    if let Some(path) = credentials_path {
        env.insert("GOOGLE_APPLICATION_CREDENTIALS".to_owned(), path);
    }
    Ok(ApiKeyCredential {
        key: None,
        env: Some(env),
    })
}

async fn prompt_text(interaction: &Arc<dyn AuthInteraction>, message: &str) -> Result<String, AuthError> {
    interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Text {
                message: message.to_owned(),
                placeholder: None,
            },
        })
        .await
        .map_err(AuthError::Failed)
}

async fn resolve(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    if let Some(result) = api_key_result(input).await? {
        return Ok(Some(result));
    }
    adc_result(input).await
}

async fn api_key_result(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    let stored_key = input.credential.as_ref().and_then(|credential| credential.key.clone());
    let stored_key_present = stored_key.is_some();
    let key = match stored_key {
        Some(key) => Some(key),
        None => env_value(&input.ctx, "GOOGLE_CLOUD_API_KEY", &input.signal).await?,
    };
    let Some(key) = key.filter(|key| !key.is_empty()) else {
        return Ok(None);
    };
    let source = if stored_key_present {
        "stored credential"
    } else {
        "GOOGLE_CLOUD_API_KEY"
    };
    Ok(Some(AuthResult {
        auth: ModelAuth {
            api_key: Some(key),
            headers: None,
            base_url: None,
        },
        env: None,
        source: Some(source.to_owned()),
    }))
}

async fn adc_result(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    let credential_env = input.credential.as_ref().and_then(|credential| credential.env.as_ref());
    let has_credentials = has_adc_credentials(input, credential_env).await?;
    let project = project_id(input, credential_env).await?;
    let location = location_id(input, credential_env).await?;
    let missing_project = project.as_ref().is_none_or(|project| project.is_empty());
    let missing_location = location.as_ref().is_none_or(|location| location.is_empty());
    if !has_credentials || missing_project || missing_location {
        return Ok(None);
    }
    let source = if input.credential.is_some() {
        "stored credential"
    } else {
        "gcloud application default credentials"
    };
    Ok(Some(AuthResult {
        auth: ModelAuth::default(),
        env: credential_env.cloned(),
        source: Some(source.to_owned()),
    }))
}

async fn has_adc_credentials(
    input: &ApiKeyResolveInput,
    credential_env: Option<&ProviderEnv>,
) -> Result<bool, AuthError> {
    let stored_path = credential_env
        .and_then(|env| env.get("GOOGLE_APPLICATION_CREDENTIALS"))
        .cloned();
    let adc_path = match stored_path {
        Some(path) => Some(path),
        None => env_value(&input.ctx, "GOOGLE_APPLICATION_CREDENTIALS", &input.signal).await?,
    };
    check_aborted(&input.signal)?;
    let exists = input
        .ctx
        .file_exists(adc_path.as_deref().unwrap_or(VERTEX_ADC_PATH))
        .await;
    check_aborted(&input.signal)?;
    Ok(exists)
}

async fn project_id(
    input: &ApiKeyResolveInput,
    credential_env: Option<&ProviderEnv>,
) -> Result<Option<String>, AuthError> {
    if let Some(project) = credential_env.and_then(|env| env.get("GOOGLE_CLOUD_PROJECT")) {
        return Ok(Some(project.clone()));
    }
    if let Some(project) = env_value(&input.ctx, "GOOGLE_CLOUD_PROJECT", &input.signal).await? {
        return Ok(Some(project));
    }
    env_value(&input.ctx, "GCLOUD_PROJECT", &input.signal).await
}

async fn location_id(
    input: &ApiKeyResolveInput,
    credential_env: Option<&ProviderEnv>,
) -> Result<Option<String>, AuthError> {
    if let Some(location) = credential_env.and_then(|env| env.get("GOOGLE_CLOUD_LOCATION")) {
        return Ok(Some(location.clone()));
    }
    env_value(&input.ctx, "GOOGLE_CLOUD_LOCATION", &input.signal).await
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

/// `googleVertexProvider()`.
pub fn google_vertex_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "google-vertex",
            name: "Google Vertex AI",
            base_url: None,
            auth: ProviderAuth::api_key(Arc::new(VertexAuth)),
            filter_models: None,
        },
        single_api(google_vertex_api()),
    )
}
