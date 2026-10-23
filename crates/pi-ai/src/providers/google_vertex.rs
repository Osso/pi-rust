//! Port of `packages/ai/src/providers/google-vertex.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;

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
                message: "Select Google Vertex AI authentication method:".into(),
                options: vec![
                    AuthSelectOption {
                        id: "api-key".into(),
                        label: "Google Cloud API key".into(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "adc".into(),
                        label: "Application Default Credentials".into(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: "service-account".into(),
                        label: "Service account credentials file".into(),
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
                    message: "Enter Google Cloud API key".into(),
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
        return Err(AuthError::failed(js_format!(
            "Unknown Google Vertex AI auth method: {}",
            method
        )));
    }

    interaction.notify(AuthEvent::Info {
        message: if method == "adc" {
            "Run `gcloud auth application-default login`, then provide the project and location.".into()
        } else {
            "Provide a service account credentials file, project, and location.".into()
        },
        links: Some(vec![AuthInfoLink {
            url: "https://cloud.google.com/docs/authentication/provide-credentials-adc".into(),
            label: Some("Application Default Credentials".into()),
        }]),
    });

    let project = prompt_text(interaction, "Enter Google Cloud project ID").await?;
    let location = prompt_text(interaction, "Enter Google Cloud location").await?;
    let credentials_path = if method == "service-account" {
        Some(prompt_text(interaction, "Enter service account credentials file path").await?)
    } else {
        None
    };

    let mut env = indexmap::IndexMap::from([
        ("GOOGLE_CLOUD_PROJECT".into(), project),
        ("GOOGLE_CLOUD_LOCATION".into(), location),
    ]);
    if let Some(path) = credentials_path {
        env.insert("GOOGLE_APPLICATION_CREDENTIALS".into(), path);
    }
    Ok(ApiKeyCredential {
        key: None,
        env: Some(env),
    })
}

async fn prompt_text(interaction: &Arc<dyn AuthInteraction>, message: &str) -> Result<JsString, AuthError> {
    interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Text {
                message: message.into(),
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
        source: Some(source.into()),
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
        source: Some(source.into()),
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
        .file_exists(&adc_path.unwrap_or_else(|| VERTEX_ADC_PATH.into()))
        .await;
    check_aborted(&input.signal)?;
    Ok(exists)
}

async fn project_id(
    input: &ApiKeyResolveInput,
    credential_env: Option<&ProviderEnv>,
) -> Result<Option<JsString>, AuthError> {
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
) -> Result<Option<JsString>, AuthError> {
    if let Some(location) = credential_env.and_then(|env| env.get("GOOGLE_CLOUD_LOCATION")) {
        return Ok(Some(location.clone()));
    }
    env_value(&input.ctx, "GOOGLE_CLOUD_LOCATION", &input.signal).await
}

async fn env_value(
    ctx: &Arc<dyn AuthContext>,
    name: &str,
    signal: &AbortSignal,
) -> Result<Option<JsString>, AuthError> {
    check_aborted(signal)?;
    let value = ctx.env(name).await;
    check_aborted(signal)?;
    Ok(value.filter(|value| !value.is_empty()))
}

fn check_aborted(signal: &AbortSignal) -> Result<(), AuthError> {
    if signal.aborted() {
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
