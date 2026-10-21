//! Provider readiness and credential helpers (`cli/auth-check.ts`).

use std::sync::Arc;

use pi_ai::auth::AuthResolutionOverrides;
use pi_ai::auth::types::{AuthOperationOptions, AuthType, Credential, CredentialStore};

use super::args::Args;
use super::auth_command::{AuthCommandError, AuthCommandKind, get_auth_credential, validate_auth_command_args};
use crate::core::model_resolver::{ResolveCliModelOptions, resolve_cli_model};
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime, ModelRuntimeError};
use crate::core::models_store::InMemoryCodingAgentModelsStore;

/// `AuthCheckStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCheckStatus {
    Ready,
    NotReady,
    Invalid,
}

impl AuthCheckStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotReady => "not_ready",
            Self::Invalid => "invalid",
        }
    }
}

/// `AuthCheckReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCheckReason {
    ProviderNotFound,
    CredentialsNotConfigured,
    CredentialNotAvailable,
    InvalidState,
}

impl AuthCheckReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderNotFound => "provider_not_found",
            Self::CredentialsNotConfigured => "credentials_not_configured",
            Self::CredentialNotAvailable => "credential_not_available",
            Self::InvalidState => "invalid_state",
        }
    }
}

/// `AuthCheckResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCheckResult {
    pub status: AuthCheckStatus,
    pub provider: String,
    pub reason: Option<AuthCheckReason>,
    pub auth_type: Option<AuthType>,
}

/// `checkProviderAuth` options (`refresh` defaults to false).
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckProviderAuthOptions {
    pub refresh: bool,
}

/// `checkProviderAuth`.
pub async fn check_provider_auth(
    args: &Args,
    model_runtime: &ModelRuntime,
    options: CheckProviderAuthOptions,
) -> Result<AuthCheckResult, AuthCommandError> {
    let target = validate_auth_command_args(args, AuthCommandKind::Check)?;
    let mut provider = target.provider().map(str::to_owned);

    if let Some(cli_model) = target.model() {
        let resolved = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_provider: provider.clone(),
                cli_model: Some(cli_model.to_owned()),
                cli_thinking: None,
            },
            model_runtime,
        );
        if resolved.error.is_some() || resolved.model.is_none() {
            return Err(AuthCommandError::new(
                resolved
                    .error
                    .unwrap_or_else(|| format!("Unable to resolve model \"{cli_model}\"")),
            ));
        }
        provider = resolved.model.map(|model| model.provider);
    }

    let Some(provider) = provider else {
        return Err(AuthCommandError::new("Unable to resolve an auth provider"));
    };

    if model_runtime.get_error().is_some() {
        return Ok(invalid(&provider));
    }
    if model_runtime.get_provider(&provider).is_none() {
        return Ok(not_ready(&provider, AuthCheckReason::ProviderNotFound));
    }

    match check_provider(&provider, model_runtime, options.refresh).await {
        Ok(result) => Ok(result),
        Err(()) => Ok(invalid(&provider)),
    }
}

async fn check_provider(provider: &str, model_runtime: &ModelRuntime, refresh: bool) -> Result<AuthCheckResult, ()> {
    let auth = model_runtime
        .check_auth(provider, AuthOperationOptions::default())
        .await
        .map_err(|_| ())?;
    let Some(auth) = auth else {
        return Ok(not_ready(provider, AuthCheckReason::CredentialsNotConfigured));
    };
    if refresh && !refreshed_auth_available(provider, model_runtime).await {
        return Ok(not_ready(provider, AuthCheckReason::CredentialsNotConfigured));
    }
    Ok(ready(provider, auth.auth_type))
}

async fn refreshed_auth_available(provider: &str, model_runtime: &ModelRuntime) -> bool {
    matches!(
        model_runtime
            .get_auth(provider, AuthResolutionOverrides::default())
            .await,
        Ok(Some(_))
    )
}

fn ready(provider: &str, auth_type: AuthType) -> AuthCheckResult {
    AuthCheckResult {
        status: AuthCheckStatus::Ready,
        provider: provider.to_owned(),
        reason: None,
        auth_type: Some(auth_type),
    }
}

fn not_ready(provider: &str, reason: AuthCheckReason) -> AuthCheckResult {
    AuthCheckResult {
        status: AuthCheckStatus::NotReady,
        provider: provider.to_owned(),
        reason: Some(reason),
        auth_type: None,
    }
}

fn invalid(provider: &str) -> AuthCheckResult {
    AuthCheckResult {
        status: AuthCheckStatus::Invalid,
        provider: provider.to_owned(),
        reason: Some(AuthCheckReason::InvalidState),
        auth_type: None,
    }
}

/// `getProviderCredential`.
pub async fn get_provider_credential(
    provider_id: &str,
    model_runtime: &ModelRuntime,
    credentials: Arc<dyn CredentialStore>,
    options: CheckProviderAuthOptions,
) -> Result<Option<String>, String> {
    let credential = credentials
        .read(provider_id, AuthOperationOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    if !options.refresh
        && let Some(Credential::OAuth(oauth)) = credential.as_ref()
    {
        return Ok(Some(oauth.access_utf8().map_err(|error| error.to_string())?));
    }

    let auth = model_runtime
        .get_auth(provider_id, AuthResolutionOverrides::default())
        .await
        .map_err(|error| error.to_string())?;
    Ok(get_auth_credential(auth.as_ref()))
}

/// `createAuthCheckModelRuntime`.
pub async fn create_auth_check_model_runtime(
    credentials: Arc<dyn CredentialStore>,
) -> Result<ModelRuntime, ModelRuntimeError> {
    ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(credentials),
        models_store: Some(Arc::new(InMemoryCodingAgentModelsStore::new())),
        allow_model_network: false,
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
}
