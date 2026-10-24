//! Port of `packages/ai/src/auth/resolve.ts`.

use std::sync::Arc;

use super::types::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthError, AuthModel, AuthResult, Credential,
    CredentialStore, OAuthAuth, OAuthCredential, ProviderAuth,
};
use crate::request::{BoxFuture, CallbackError, now_millis};
use crate::types::ProviderEnv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsErrorCode {
    ModelSource,
    ModelValidation,
    Provider,
    Stream,
    Auth,
    Oauth,
}
impl ModelsErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelSource => "model_source",
            Self::ModelValidation => "model_validation",
            Self::Provider => "provider",
            Self::Stream => "stream",
            Self::Auth => "auth",
            Self::Oauth => "oauth",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelsError {
    pub code: ModelsErrorCode,
    pub message: pi_js_compat::js_json::JsString,
    pub cause: Option<Box<pi_js_compat::abort::AbortReason>>,
}
impl ModelsError {
    pub fn new(code: ModelsErrorCode, message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }
    pub fn with_cause(
        code: ModelsErrorCode,
        message: impl Into<pi_js_compat::js_json::JsString>,
        cause: Option<Box<pi_js_compat::abort::AbortReason>>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            cause,
        }
    }
    pub fn into_callback(self) -> CallbackError {
        let mut error = pi_js_compat::node_error::NativeNodeError::new("ModelsError", self.message.clone())
            .with_property(
                "code",
                pi_js_compat::js_json::JsValue::String(self.code.as_str().into()),
            );
        if let Some(cause) = &self.cause {
            error = error.with_cause(cause.as_ref().clone());
        }
        CallbackError::from(error).with_original(Arc::new(self))
    }
}
impl std::fmt::Display for ModelsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}
impl std::error::Error for ModelsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.cause.as_deref() {
            Some(
                pi_js_compat::abort::AbortReason::Error(error) | pi_js_compat::abort::AbortReason::DomException(error),
            ) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthResolutionOverrides {
    pub api_key: Option<pi_js_compat::js_json::JsString>,
    pub env: Option<ProviderEnv>,
}

/// Resolve auth for the actual requested chat or image model.
pub fn resolve_provider_auth(
    provider_id: &str,
    provider_auth: &ProviderAuth,
    model: AuthModel,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    overrides: AuthResolutionOverrides,
) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>> {
    let provider_id = provider_id.to_owned();
    let provider_auth = provider_auth.clone();
    Box::pin(async move {
        let context = overlay_auth_context(auth_context, overrides.env.clone());
        if let (Some(key), Some(api_key)) = (overrides.api_key, provider_auth.api_key.as_ref()) {
            return resolve_api_key(
                Arc::clone(api_key),
                context,
                model,
                Some(ApiKeyCredential {
                    key: Some(key),
                    env: overrides.env,
                }),
            )
            .await;
        }
        let stored = read_credential(credentials.as_ref(), &provider_id).await?;
        if let Some(credential) = stored {
            return resolve_stored_credential(
                &provider_id,
                &provider_auth,
                model,
                credential,
                credentials,
                context,
                overrides.env,
            )
            .await;
        }
        match provider_auth.api_key {
            Some(api_key) => resolve_api_key(api_key, context, model, None).await,
            None => Ok(None),
        }
    })
}

async fn read_credential(
    credentials: &dyn CredentialStore,
    provider_id: &str,
) -> Result<Option<Credential>, ModelsError> {
    credentials
        .read(provider_id, Default::default())
        .await
        .map_err(|error| {
            wrap_auth_failure(
                ModelsErrorCode::Auth,
                format!("Credential store read failed for {provider_id}"),
                error,
            )
        })
}

async fn resolve_stored_credential(
    provider_id: &str,
    auth: &ProviderAuth,
    model: AuthModel,
    credential: Credential,
    credentials: Arc<dyn CredentialStore>,
    context: Arc<dyn AuthContext>,
    env: Option<ProviderEnv>,
) -> Result<Option<AuthResult>, ModelsError> {
    match credential {
        Credential::OAuth(credential) => match &auth.oauth {
            Some(oauth) => {
                resolve_stored_oauth(credentials, provider_id.to_owned(), Arc::clone(oauth), credential).await
            }
            None => Ok(None),
        },
        Credential::ApiKey(mut credential) => {
            if let Some(env) = env {
                credential.env.get_or_insert_with(Default::default).extend(env);
            }
            match &auth.api_key {
                Some(api_key) => resolve_api_key(Arc::clone(api_key), context, model, Some(credential)).await,
                None => Ok(None),
            }
        }
    }
}

struct OverlayAuthContext {
    base: Arc<dyn AuthContext>,
    env: ProviderEnv,
}
impl AuthContext for OverlayAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<pi_js_compat::js_json::JsString>> {
        let name = name.to_owned();
        Box::pin(async move {
            match self.env.get(&name).filter(|value| !value.is_empty()) {
                Some(value) => Some(value.clone()),
                None => self.base.env(&name).await,
            }
        })
    }
    fn file_exists(&self, path: &pi_js_compat::js_json::JsString) -> BoxFuture<'_, bool> {
        self.base.file_exists(path)
    }
}
fn overlay_auth_context(base: Arc<dyn AuthContext>, env: Option<ProviderEnv>) -> Arc<dyn AuthContext> {
    match env {
        Some(env) => Arc::new(OverlayAuthContext { base, env }),
        None => base,
    }
}

async fn resolve_stored_oauth(
    credentials: Arc<dyn CredentialStore>,
    provider_id: String,
    oauth: Arc<dyn OAuthAuth>,
    mut credential: OAuthCredential,
) -> Result<Option<AuthResult>, ModelsError> {
    if now_millis() as f64 >= credential.expires {
        let post = credentials
            .modify(
                &provider_id,
                oauth_refresh_modifier(provider_id.clone(), Arc::clone(&oauth)),
                Default::default(),
            )
            .await
            .map_err(|error| preserve_modify_error(&provider_id, error))?;
        let Some(Credential::OAuth(post)) = post else {
            return Ok(None);
        };
        credential = post;
    }
    let auth = oauth.to_auth(credential).await.map_err(|error| {
        wrap_auth_failure(
            ModelsErrorCode::Oauth,
            format!("OAuth auth derivation failed for {provider_id}"),
            error,
        )
    })?;
    Ok(Some(AuthResult {
        auth,
        env: None,
        source: Some("OAuth".into()),
    }))
}
fn oauth_refresh_modifier(provider_id: String, oauth: Arc<dyn OAuthAuth>) -> super::types::CredentialModifier {
    Arc::new(move |current| Box::pin(refresh_locked_oauth(provider_id.clone(), Arc::clone(&oauth), current)))
}
async fn refresh_locked_oauth(
    provider_id: String,
    oauth: Arc<dyn OAuthAuth>,
    current: Option<Credential>,
) -> Result<Option<Credential>, AuthError> {
    let Some(Credential::OAuth(current)) = current else {
        return Ok(None);
    };
    if (now_millis() as f64) < current.expires {
        return Ok(None);
    }
    oauth
        .refresh(current, Default::default())
        .await
        .map(|credential| Some(Credential::OAuth(credential)))
        .map_err(|error| {
            AuthError::from(
                wrap_auth_failure(
                    ModelsErrorCode::Oauth,
                    format!("OAuth refresh failed for {provider_id}"),
                    error,
                )
                .into_callback(),
            )
        })
}
fn preserve_modify_error(provider_id: &str, error: AuthError) -> ModelsError {
    let callback = error.callback();
    if let Some(existing) = callback
        .original
        .as_deref()
        .and_then(|error| error.downcast_ref::<ModelsError>())
    {
        return existing.clone();
    }
    wrap_auth_failure(
        ModelsErrorCode::Auth,
        format!("Credential store modify failed for {provider_id}"),
        error,
    )
}
async fn resolve_api_key(
    api_key: Arc<dyn ApiKeyAuth>,
    context: Arc<dyn AuthContext>,
    model: AuthModel,
    credential: Option<ApiKeyCredential>,
) -> Result<Option<AuthResult>, ModelsError> {
    let provider = model.provider().to_owned();
    api_key
        .resolve(ApiKeyResolveInput {
            model: Some(model),
            ctx: context,
            credential,
            signal: Default::default(),
        })
        .await
        .map_err(|error| {
            wrap_auth_failure(
                ModelsErrorCode::Auth,
                format!("API key auth failed for provider {provider}"),
                error,
            )
        })
}
fn wrap_auth_failure(code: ModelsErrorCode, message: String, error: AuthError) -> ModelsError {
    ModelsError::with_cause(code, message, Some(Box::new(error.thrown_value())))
}
