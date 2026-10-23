//! Port of `packages/ai/src/auth/resolve.ts`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::abort::{AbortController, AbortSignal};

use crate::request::{BoxFuture, CallbackError, now_millis};
use crate::types::ProviderEnv;
use crate::utils::abort::{operation_signal, race_with_abort_signal};

use super::types::{
    ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthError, AuthModel, AuthResult, Credential,
    CredentialModifier, CredentialStore, ModelAuth, OAuthCredential, ProviderAuth,
};

pub const DEFAULT_OAUTH_MINIMUM_VALIDITY_MS: f64 = 5.0 * 60.0 * 1000.0;
pub const DEFAULT_OAUTH_REFRESH_TIMEOUT_MS: u64 = 15_000;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelsError {
    pub code: ModelsErrorCode,
    pub message: pi_js_compat::js_json::JsString,
}

impl ModelsError {
    pub fn new(code: ModelsErrorCode, message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn with_cause(
        code: ModelsErrorCode,
        message: impl Into<pi_js_compat::js_json::JsString>,
        cause: Option<&pi_js_compat::js_json::JsString>,
    ) -> Self {
        Self::new(code, with_cause_detail(&message.into(), cause))
    }
}

impl std::fmt::Display for ModelsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ModelsError {}

/// Failure from auth resolution: cancellation escapes unwrapped; provider and
/// store failures are [`ModelsError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Aborted,
    Models(ModelsError),
    /// Raw callback failure that TypeScript would propagate unwrapped.
    Callback(CallbackError),
}

impl ResolveError {
    pub fn message(&self) -> pi_js_compat::js_json::JsString {
        match self {
            Self::Aborted => "The operation was aborted".into(),
            Self::Models(error) => error.message.clone(),
            Self::Callback(error) => error.message.clone(),
        }
    }

    pub fn models(code: ModelsErrorCode, message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::Models(ModelsError::new(code, message))
    }

    pub fn with_cause(
        code: ModelsErrorCode,
        message: impl Into<pi_js_compat::js_json::JsString>,
        cause: Option<&pi_js_compat::js_json::JsString>,
    ) -> Self {
        Self::Models(ModelsError::with_cause(code, message, cause))
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aborted => formatter.write_str("The operation was aborted"),
            Self::Models(error) => write!(formatter, "{error}"),
            Self::Callback(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Keep the underlying reason in the message: callers surface `message` only.
pub fn with_cause_detail(
    message: &pi_js_compat::js_json::JsString,
    cause: Option<&pi_js_compat::js_json::JsString>,
) -> pi_js_compat::js_json::JsString {
    let Some(cause) = cause else {
        return message.to_owned();
    };
    let detail = cause.trim();
    if detail.is_empty() || message.contains(&detail) {
        return message.to_owned();
    }
    pi_js_compat::js_json::JsString::join([message.clone(), ": ".into(), detail], "")
}

#[derive(Debug, Clone, Default)]
pub struct AuthResolutionOverrides {
    pub api_key: Option<pi_js_compat::js_json::JsString>,
    pub env: Option<ProviderEnv>,
    /// Require this much remaining OAuth-token validity.
    pub min_oauth_validity_ms: Option<f64>,
    pub signal: Option<AbortSignal>,
}

/// Frozen request-auth entrypoint: callers supply the actual requested model.
pub fn resolve_provider_auth(
    provider_id: &str,
    provider_auth: &ProviderAuth,
    model: AuthModel,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    overrides: AuthResolutionOverrides,
) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>> {
    resolve_auth_input(
        provider_id,
        provider_auth,
        Some(model),
        credentials,
        auth_context,
        overrides,
    )
}

/// Approved Q15 gap: provider-ID-only status wrappers have no request model.
/// This path must not be used for chat/image requests or invent a representative model.
pub fn resolve_provider_status_auth(
    provider_id: &str,
    provider_auth: &ProviderAuth,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    overrides: AuthResolutionOverrides,
) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>> {
    resolve_auth_input(provider_id, provider_auth, None, credentials, auth_context, overrides)
}

fn resolve_auth_input(
    provider_id: &str,
    provider_auth: &ProviderAuth,
    model: Option<AuthModel>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    overrides: AuthResolutionOverrides,
) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>> {
    let provider_id = provider_id.to_owned();
    let provider_auth = provider_auth.clone();
    let signal = operation_signal(overrides.signal.as_ref());
    let operation = resolve_with_signal(
        provider_id,
        provider_auth,
        model,
        credentials,
        auth_context,
        overrides,
        signal.clone(),
    );
    Box::pin(async move {
        race_with_abort_signal(operation, &signal)
            .await
            .map_err(|_| ResolveError::Aborted)?
    })
}

async fn resolve_with_signal(
    provider_id: String,
    provider_auth: ProviderAuth,
    model: Option<AuthModel>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    overrides: AuthResolutionOverrides,
    signal: AbortSignal,
) -> Result<Option<AuthResult>, ResolveError> {
    check_aborted(&signal)?;
    let request_context: Arc<dyn AuthContext> = match &overrides.env {
        Some(env) => Arc::new(OverlayAuthContext {
            base: auth_context,
            env: env.clone(),
        }),
        None => auth_context,
    };

    if let Some(api_key) = &overrides.api_key
        && let Some(api_key_auth) = &provider_auth.api_key
    {
        let credential = ApiKeyCredential {
            key: Some((api_key.clone()).into()),
            env: overrides.env.clone(),
        };
        return resolve_api_key(
            Arc::clone(api_key_auth),
            request_context,
            Some(credential),
            provider_id,
            model,
            signal,
        )
        .await;
    }

    let stored = read_credential(Arc::clone(&credentials), &provider_id, signal.clone()).await?;
    if let Some(stored) = stored {
        match &stored {
            Credential::OAuth(credential) if provider_auth.oauth.is_some() => {
                return resolve_stored_oauth(
                    credentials,
                    provider_id,
                    provider_auth,
                    credential.clone(),
                    signal,
                    overrides.min_oauth_validity_ms,
                )
                .await;
            }
            Credential::ApiKey(credential) if provider_auth.api_key.is_some() => {
                let credential = match &overrides.env {
                    Some(env) => {
                        let mut merged = credential.env.clone().unwrap_or_default();
                        merged.extend(env.clone());
                        ApiKeyCredential {
                            key: credential.key.clone(),
                            env: Some(merged),
                        }
                    }
                    None => credential.clone(),
                };
                return resolve_api_key(
                    Arc::clone(provider_auth.api_key.as_ref().expect("checked above")),
                    request_context,
                    Some(credential),
                    provider_id,
                    model,
                    signal,
                )
                .await;
            }
            _ => return Ok(None),
        }
    }

    match &provider_auth.api_key {
        Some(api_key_auth) => {
            resolve_api_key(
                Arc::clone(api_key_auth),
                request_context,
                None,
                provider_id,
                model,
                signal,
            )
            .await
        }
        None => Ok(None),
    }
}

async fn resolve_stored_oauth(
    credentials: Arc<dyn CredentialStore>,
    provider_id: String,
    provider_auth: ProviderAuth,
    stored: OAuthCredential,
    signal: AbortSignal,
    min_oauth_validity_ms: Option<f64>,
) -> Result<Option<AuthResult>, ResolveError> {
    let minimum_validity_ms = DEFAULT_OAUTH_MINIMUM_VALIDITY_MS.max(min_oauth_validity_ms.unwrap_or(0.0));
    let mut credential = stored;

    if expires_soon(&credential, minimum_validity_ms) {
        let oauth = Arc::clone(provider_auth.oauth.as_ref().expect("caller checked oauth auth"));
        let refresh_error: Arc<Mutex<Option<ModelsError>>> = Arc::new(Mutex::new(None));
        let modifier: CredentialModifier = {
            let provider_id = provider_id.clone();
            let signal = signal.clone();
            let refresh_error = Arc::clone(&refresh_error);
            Arc::new(move |current: Option<Credential>| {
                let oauth = Arc::clone(&oauth);
                let provider_id = provider_id.clone();
                let signal = signal.clone();
                let refresh_error = Arc::clone(&refresh_error);
                Box::pin(async move {
                    let Some(Credential::OAuth(current)) = current else {
                        return Ok(None);
                    };
                    if !expires_soon(&current, minimum_validity_ms) {
                        return Ok(None);
                    }
                    let controller = AbortController::new();
                    let refresh_signal = AbortSignal::any(&[signal.clone(), controller.signal()]);
                    let timer = spawn_timeout(controller, DEFAULT_OAUTH_REFRESH_TIMEOUT_MS);
                    let result =
                        race_with_abort_signal(oauth.refresh(current, refresh_signal.clone()), &refresh_signal).await;
                    timer.abort();
                    match result {
                        Ok(Ok(credential)) => Ok(Some(Credential::OAuth(credential))),
                        Ok(Err(AuthError::Aborted)) | Err(_) => {
                            if signal.aborted() {
                                return Err(AuthError::Aborted);
                            }
                            let models_error = ModelsError::with_cause(
                                ModelsErrorCode::Oauth,
                                format!("OAuth refresh failed for {provider_id}"),
                                Some(&"The operation was aborted".into()),
                            );
                            *refresh_error.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                Some(models_error.clone());
                            Err(AuthError::failed(models_error.message))
                        }
                        Ok(Err(AuthError::Failed(error))) => {
                            let models_error = ModelsError::with_cause(
                                ModelsErrorCode::Oauth,
                                format!("OAuth refresh failed for {provider_id}"),
                                Some(&error.message),
                            );
                            *refresh_error.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                Some(models_error.clone());
                            Err(AuthError::failed(models_error.message))
                        }
                    }
                })
            })
        };

        let post = credentials
            .modify(
                &provider_id,
                modifier,
                super::types::AuthOperationOptions {
                    signal: Some(signal.clone()),
                },
            )
            .await;
        let post = match post {
            Ok(post) => post,
            Err(AuthError::Aborted) if signal.aborted() => return Err(ResolveError::Aborted),
            Err(error) => {
                if let Some(models_error) = refresh_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    return Err(ResolveError::Models(models_error));
                }
                return Err(ResolveError::with_cause(
                    ModelsErrorCode::Auth,
                    format!("Credential store modify failed for {provider_id}"),
                    error.detail(),
                ));
            }
        };
        let Some(Credential::OAuth(post)) = post else {
            return Ok(None);
        };
        credential = post;
        if min_oauth_validity_ms.is_some() && expires_soon(&credential, minimum_validity_ms) {
            return Err(ResolveError::models(
                ModelsErrorCode::Oauth,
                format!("OAuth refresh returned a token that expires too soon for {provider_id}"),
            ));
        }
    }

    let auth = match oauth_to_auth(&provider_auth, credential).await {
        Ok(auth) => auth,
        Err(AuthError::Aborted) => return Err(ResolveError::Aborted),
        Err(AuthError::Failed(error)) => {
            return Err(ResolveError::with_cause(
                ModelsErrorCode::Oauth,
                format!("OAuth auth derivation failed for {provider_id}"),
                Some(&error.message),
            ));
        }
    };
    Ok(Some(AuthResult {
        auth,
        env: None,
        source: Some(("OAuth".to_owned()).into()),
    }))
}

async fn oauth_to_auth(provider_auth: &ProviderAuth, credential: OAuthCredential) -> Result<ModelAuth, AuthError> {
    match &provider_auth.oauth {
        Some(oauth) => oauth.to_auth(credential).await,
        None => Err(AuthError::failed("provider has no OAuth auth")),
    }
}

async fn resolve_api_key(
    api_key: Arc<dyn super::types::ApiKeyAuth>,
    ctx: Arc<dyn AuthContext>,
    credential: Option<ApiKeyCredential>,
    provider_id: String,
    model: Option<AuthModel>,
    signal: AbortSignal,
) -> Result<Option<AuthResult>, ResolveError> {
    let error_provider = match &model {
        Some(model) => model.provider().to_owned(),
        None => provider_id,
    };
    match api_key
        .resolve(ApiKeyResolveInput {
            model,
            ctx,
            credential,
            signal,
        })
        .await
    {
        Ok(result) => Ok(result),
        Err(AuthError::Aborted) => Err(ResolveError::Aborted),
        Err(AuthError::Failed(error)) => Err(ResolveError::with_cause(
            ModelsErrorCode::Auth,
            format!("API key auth failed for provider {error_provider}"),
            Some(&error.message),
        )),
    }
}

async fn read_credential(
    credentials: Arc<dyn CredentialStore>,
    provider_id: &str,
    signal: AbortSignal,
) -> Result<Option<Credential>, ResolveError> {
    match credentials
        .read(provider_id, super::types::AuthOperationOptions { signal: Some(signal) })
        .await
    {
        Ok(credential) => Ok(credential),
        Err(AuthError::Aborted) => Err(ResolveError::Aborted),
        Err(AuthError::Failed(error)) => Err(ResolveError::with_cause(
            ModelsErrorCode::Auth,
            format!("Credential store read failed for {provider_id}"),
            Some(&error.message),
        )),
    }
}

fn expires_soon(credential: &OAuthCredential, minimum_validity_ms: f64) -> bool {
    now_millis() as f64 + minimum_validity_ms >= credential.expires
}

fn spawn_timeout(token: AbortController, milliseconds: u64) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(milliseconds)).await;
        token.abort(None);
    })
}

fn check_aborted(signal: &AbortSignal) -> Result<(), ResolveError> {
    if signal.aborted() {
        Err(ResolveError::Aborted)
    } else {
        Ok(())
    }
}

struct OverlayAuthContext {
    base: Arc<dyn AuthContext>,
    env: ProviderEnv,
}

impl AuthContext for OverlayAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<pi_js_compat::js_json::JsString>> {
        let value = self.env.get(name).filter(|value| !value.is_empty()).cloned();
        let base = Arc::clone(&self.base);
        let name = name.to_owned();
        Box::pin(async move {
            match value {
                Some(value) => Some(value),
                None => base.env(&name).await,
            }
        })
    }

    fn file_exists(&self, path: &pi_js_compat::js_json::JsString) -> BoxFuture<'_, bool> {
        self.base.file_exists(path)
    }
}
