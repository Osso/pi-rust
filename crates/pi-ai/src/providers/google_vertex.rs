//! Port of `packages/ai/src/providers/google-vertex.ts`.

use std::sync::Arc;

use pi_js_compat::js_json::JsString;

use crate::api::google_vertex::google_vertex_api;
use crate::auth::{ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthResult, ModelAuth, ProviderAuth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};
use crate::request::BoxFuture;

const VERTEX_ADC_PATH: &str = "~/.config/gcloud/application_default_credentials.json";

struct VertexAuth;

impl ApiKeyAuth for VertexAuth {
    fn name(&self) -> &str {
        "Google Cloud credentials"
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            if let Some(credential) = &input.credential {
                return Ok(credential
                    .key
                    .as_ref()
                    .filter(|key| !key.is_empty())
                    .map(|key| key_auth_result(key.clone(), "stored credential")));
            }
            if let Some(key) = read_env(&input, "GOOGLE_CLOUD_API_KEY")
                .await?
                .filter(|key| !key.is_empty())
            {
                return Ok(Some(key_auth_result(key, "GOOGLE_CLOUD_API_KEY")));
            }
            resolve_ambient_adc(&input).await
        })
    }
}

fn key_auth_result(key: JsString, source: &str) -> AuthResult {
    AuthResult {
        auth: ModelAuth {
            api_key: Some(key),
            ..Default::default()
        },
        env: None,
        source: Some(source.into()),
    }
}

async fn resolve_ambient_adc(input: &ApiKeyResolveInput) -> Result<Option<AuthResult>, AuthError> {
    let adc_path = read_env(input, "GOOGLE_APPLICATION_CREDENTIALS")
        .await?
        .unwrap_or_else(|| VERTEX_ADC_PATH.into());
    let has_credentials = input.ctx.file_exists(&adc_path).await;
    let project = match read_env(input, "GOOGLE_CLOUD_PROJECT").await? {
        Some(project) => Some(project),
        None => read_env(input, "GCLOUD_PROJECT").await?,
    };
    let location = read_env(input, "GOOGLE_CLOUD_LOCATION").await?;
    let has_project = project.is_some_and(|project| !project.is_empty());
    let has_location = location.is_some_and(|location| !location.is_empty());
    Ok((has_credentials && has_project && has_location).then(|| AuthResult {
        auth: ModelAuth::default(),
        env: None,
        source: Some("gcloud application default credentials".into()),
    }))
}

async fn read_env(input: &ApiKeyResolveInput, name: &str) -> Result<Option<JsString>, AuthError> {
    AuthError::check_aborted(&input.signal)?;
    let value = input.ctx.env(name).await;
    AuthError::check_aborted(&input.signal)?;
    Ok(value)
}

/// `googleVertexProvider()`.
pub fn google_vertex_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "google-vertex",
            name: "Google Vertex AI",
            base_url: None,
            auth: ProviderAuth::api_key(Arc::new(VertexAuth)),
        },
        single_api(google_vertex_api()),
    )
}
