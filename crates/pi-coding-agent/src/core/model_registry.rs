//! Port of `packages/coding-agent/src/core/model-registry.ts`.

use pi_ai::auth::resolve::{AuthResolutionOverrides, ResolveError};
use pi_ai::auth::types::AuthResult;
use pi_ai::models::{ModelsRefreshOptions, ModelsRefreshResult, Provider};
use pi_ai::request::StreamOptions;
use pi_ai::types::{AssistantMessage, Context, Model};
use pi_ai::types::{ProviderEnv, ProviderHeaders};
use pi_ai::utils::event_stream::StreamFailure;
use pi_js_compat::js_json::JsString;

use crate::core::model_runtime::{ModelRuntime, ModelRuntimeError};
use crate::core::provider_composer::{AuthStatus, ProviderConfigInput};

pub use crate::core::provider_composer::clear_api_key_cache;

/// Resolved request auth exposed to extensions.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedRequestAuth {
    Ok {
        api_key: Option<JsString>,
        headers: Option<ProviderHeaders>,
        base_url: Option<JsString>,
        env: Option<ProviderEnv>,
    },
    Err {
        error: JsString,
    },
}

impl ResolvedRequestAuth {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }

    pub fn error(&self) -> Option<&JsString> {
        match self {
            Self::Ok { .. } => None,
            Self::Err { error } => Some(error),
        }
    }
}

fn format_request_auth_error(model: &Model, error: &ModelRuntimeError) -> JsString {
    let message = error.message();
    if message == "authHeader requires a resolved API key" {
        format!("No API key found for \"{}\"", model.provider).into()
    } else {
        message
    }
}

/// Synchronous compatibility facade exposed to extensions.
#[derive(Clone)]
pub struct ModelRegistry {
    runtime: ModelRuntime,
}

impl ModelRegistry {
    pub fn new(runtime: ModelRuntime) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &ModelRuntime {
        &self.runtime
    }

    pub async fn refresh(&self, options: ModelsRefreshOptions) -> Result<ModelsRefreshResult, ModelRuntimeError> {
        self.runtime.refresh(options).await
    }

    pub fn get_error(&self) -> Option<JsString> {
        self.runtime.get_error()
    }

    pub fn get_all(&self) -> Vec<Model> {
        self.runtime.get_models(None)
    }

    pub fn get_available(&self) -> Vec<Model> {
        self.runtime.get_available_snapshot()
    }

    pub fn find(&self, provider: impl Into<JsString>, model_id: impl Into<JsString>) -> Option<Model> {
        self.runtime.get_model(provider, model_id)
    }

    pub fn has_configured_auth(&self, model: &Model) -> bool {
        self.runtime.has_configured_auth(&model.provider)
    }

    pub async fn get_api_key_and_headers(&self, model: &Model) -> ResolvedRequestAuth {
        let resolution = self
            .runtime
            .get_auth_for_model(model, AuthResolutionOverrides::default())
            .await;
        match resolution {
            Err(error) => ResolvedRequestAuth::Err {
                error: format_request_auth_error(model, &ModelRuntimeError::Resolve(error)),
            },
            Ok(None) => match self.runtime.get_compatibility_request_config(model) {
                Ok(compatibility) => {
                    if compatibility.auth_header {
                        ResolvedRequestAuth::Err {
                            error: format!("No API key found for \"{}\"", model.provider).into(),
                        }
                    } else {
                        ResolvedRequestAuth::Ok {
                            api_key: None,
                            headers: compatibility.headers,
                            base_url: None,
                            env: None,
                        }
                    }
                }
                Err(error) => ResolvedRequestAuth::Err { error: error.message() },
            },
            Ok(Some(resolution)) => ResolvedRequestAuth::Ok {
                api_key: resolution.auth.api_key,
                headers: resolution.auth.headers,
                base_url: resolution.auth.base_url,
                env: resolution.env,
            },
        }
    }

    pub fn get_provider_auth_status(&self, provider: &str) -> AuthStatus {
        self.runtime.get_provider_auth_status(provider)
    }

    pub fn get_provider(&self, provider: &str) -> Option<std::sync::Arc<dyn Provider>> {
        self.runtime.get_provider(provider)
    }

    pub async fn complete(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessage, StreamFailure> {
        self.runtime.complete(model, context, options).await
    }

    pub fn get_provider_display_name(&self, provider: &str) -> String {
        self.runtime
            .get_provider(provider)
            .map(|provider| provider.name().to_owned())
            .unwrap_or_else(|| provider.to_owned())
    }

    pub async fn get_provider_auth(&self, provider: &str) -> Result<Option<AuthResult>, ResolveError> {
        self.runtime
            .get_auth(provider, AuthResolutionOverrides::default())
            .await
    }

    pub async fn get_api_key_for_provider(&self, provider: &str) -> Result<Option<JsString>, ResolveError> {
        Ok(self
            .get_provider_auth(provider)
            .await?
            .and_then(|resolution| resolution.auth.api_key))
    }

    pub async fn resolve_provider_auth(&self, provider: &str) -> Result<Option<AuthResult>, ResolveError> {
        self.get_provider_auth(provider).await
    }

    pub fn is_using_oauth(&self, model: &Model) -> bool {
        self.runtime.is_using_oauth(&model.provider)
    }

    pub fn register_native_provider(&self, provider: std::sync::Arc<dyn Provider>) {
        self.runtime.register_native_provider(provider);
    }

    pub fn register_provider(&self, provider_name: &str, config: ProviderConfigInput) -> Result<(), ModelRuntimeError> {
        self.runtime.register_provider(provider_name, config)
    }

    pub fn unregister_provider(&self, provider_name: &str) {
        self.runtime.unregister_provider(provider_name);
    }

    pub fn get_registered_provider_config(&self, provider_name: &str) -> Option<ProviderConfigInput> {
        self.runtime.get_registered_provider_config(provider_name)
    }

    pub fn get_registered_native_provider(&self, provider_name: &str) -> Option<std::sync::Arc<dyn Provider>> {
        self.runtime.get_registered_native_provider(provider_name)
    }

    pub fn get_registered_provider_ids(&self) -> Vec<String> {
        self.runtime.get_registered_provider_ids()
    }
}
