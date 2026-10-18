//! Credential printing (`cli/credential-print.ts`).

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_ai::auth::AuthResolutionOverrides;
use pi_ai::auth::types::{AuthOperationOptions, CredentialInfo, CredentialType};
use pi_ai::types::Model;
use tokio_util::sync::CancellationToken;

use super::args::Args;
use super::auth_command::{AuthCommandError, AuthCommandKind, get_auth_credential, validate_auth_command_args};
use crate::core::model_resolver::{ResolveCliModelOptions, resolve_cli_model};
use crate::core::model_runtime::ModelRuntime;

const DEFAULT_BEARER_TOKEN_MIN_EXPIRY_MS: f64 = 30.0 * 60_000.0;
const CUSTOM_MODEL_ID_WARNING: &str = "Using custom model id";

/// `Exclude<AuthCommandKind, "check">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialPrintKind {
    ApiKey,
    BearerToken,
}

impl CredentialPrintKind {
    const fn auth_command_kind(self) -> AuthCommandKind {
        match self {
            Self::ApiKey => AuthCommandKind::ApiKey,
            Self::BearerToken => AuthCommandKind::BearerToken,
        }
    }
}

/// Failure while resolving a credential for printing. `Command` mirrors the
/// source `AuthCommandError`; `Runtime` carries a non-command error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialPrintError {
    Command(AuthCommandError),
    Runtime(String),
}

impl CredentialPrintError {
    pub fn message(&self) -> &str {
        match self {
            Self::Command(error) => error.message(),
            Self::Runtime(message) => message,
        }
    }

    pub const fn is_command_error(&self) -> bool {
        matches!(self, Self::Command(_))
    }
}

impl Display for CredentialPrintError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl Error for CredentialPrintError {}

impl From<AuthCommandError> for CredentialPrintError {
    fn from(error: AuthCommandError) -> Self {
        Self::Command(error)
    }
}

fn command_error(message: impl Into<String>) -> CredentialPrintError {
    CredentialPrintError::Command(AuthCommandError::new(message))
}

/// One provider selected for credential printing.
struct CredentialProvider {
    id: String,
    model: Option<Model>,
}

/// `resolveCredentialForPrint`.
pub async fn resolve_credential_for_print(
    args: &Args,
    model_runtime: &ModelRuntime,
    kind: CredentialPrintKind,
    min_expiry_ms: Option<f64>,
    signal: Option<CancellationToken>,
) -> Result<String, CredentialPrintError> {
    let target = validate_auth_command_args(args, kind.auth_command_kind())?;
    let credential_types = credential_types(model_runtime, signal.clone()).await?;
    let providers = select_providers(model_runtime, &target, &credential_types)?;
    let credentials = collect_credentials(
        model_runtime,
        kind,
        min_expiry_ms,
        signal,
        providers.iter(),
        &credential_types,
    )
    .await;

    if credentials.len() == 1 {
        return Ok(credentials[0].1.clone());
    }
    if credentials.is_empty() {
        return Err(empty_credentials_error(
            &target,
            providers.first(),
            kind,
            &credential_types,
        ));
    }
    Err(command_error(format!(
        "Multiple configured providers matched ({}). Specify --provider.",
        credentials
            .iter()
            .map(|(provider_id, _)| provider_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

async fn credential_types(
    model_runtime: &ModelRuntime,
    signal: Option<CancellationToken>,
) -> Result<Vec<CredentialInfo>, CredentialPrintError> {
    model_runtime
        .list_credentials(AuthOperationOptions::new(signal))
        .await
        .map_err(|error| CredentialPrintError::Runtime(error.to_string()))
}

fn credential_type(credential_types: &[CredentialInfo], provider_id: &str) -> Option<CredentialType> {
    credential_types
        .iter()
        .find(|credential| credential.provider_id == provider_id)
        .map(|credential| credential.credential_type)
}

fn select_providers(
    model_runtime: &ModelRuntime,
    target: &super::auth_command::AuthCommandTarget,
    credential_types: &[CredentialInfo],
) -> Result<Vec<CredentialProvider>, CredentialPrintError> {
    if let Some(cli_provider) = target.provider() {
        return Ok(vec![explicit_provider(model_runtime, target, cli_provider)?]);
    }

    let cli_model = target.model().unwrap_or_default();
    let providers = matching_providers(model_runtime, cli_model, credential_types);
    if providers.is_empty() {
        return Err(command_error(format!(
            "Model \"{cli_model}\" not found. Use --list-models to see available models."
        )));
    }
    Ok(providers)
}

fn explicit_provider(
    model_runtime: &ModelRuntime,
    target: &super::auth_command::AuthCommandTarget,
    cli_provider: &str,
) -> Result<CredentialProvider, CredentialPrintError> {
    let provider = model_runtime.get_provider(cli_provider).ok_or_else(|| {
        command_error(format!(
            "Unknown provider \"{cli_provider}\". Use --list-models to see available providers."
        ))
    })?;
    let provider_id = provider.id().to_owned();
    let Some(cli_model) = target.model() else {
        return Ok(CredentialProvider {
            id: provider_id,
            model: None,
        });
    };

    let resolved = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_provider: Some(provider_id.clone()),
            cli_model: Some(cli_model.to_owned()),
            cli_thinking: None,
        },
        model_runtime,
    );
    match resolved.model {
        Some(model) if resolved.error.is_none() => Ok(CredentialProvider {
            id: provider_id,
            model: Some(model),
        }),
        _ => Err(command_error(resolved.error.unwrap_or_else(|| {
            "Unable to resolve the requested provider/model".to_owned()
        }))),
    }
}

fn matching_providers(
    model_runtime: &ModelRuntime,
    cli_model: &str,
    credential_types: &[CredentialInfo],
) -> Vec<CredentialProvider> {
    model_runtime
        .get_providers()
        .into_iter()
        .filter(|provider| credential_type(credential_types, provider.id()).is_some())
        .filter_map(|provider| {
            let resolved = resolve_cli_model(
                &ResolveCliModelOptions {
                    cli_provider: Some(provider.id().to_owned()),
                    cli_model: Some(cli_model.to_owned()),
                    cli_thinking: None,
                },
                model_runtime,
            );
            if resolved.error.is_some() {
                return None;
            }
            if resolved
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains(CUSTOM_MODEL_ID_WARNING))
            {
                return None;
            }
            let model = resolved.model?;
            Some(CredentialProvider {
                id: provider.id().to_owned(),
                model: Some(model),
            })
        })
        .collect()
}

async fn collect_credentials(
    model_runtime: &ModelRuntime,
    kind: CredentialPrintKind,
    min_expiry_ms: Option<f64>,
    signal: Option<CancellationToken>,
    providers: impl Iterator<Item = &CredentialProvider>,
    credential_types: &[CredentialInfo],
) -> Vec<(String, String)> {
    let mut credentials = Vec::new();
    for provider in providers {
        let credential_type = credential_type(credential_types, &provider.id);
        if !credential_matches_kind(kind, credential_type) {
            continue;
        }
        let overrides = AuthResolutionOverrides {
            min_oauth_validity_ms: (kind == CredentialPrintKind::BearerToken)
                .then_some(min_expiry_ms.unwrap_or(DEFAULT_BEARER_TOKEN_MIN_EXPIRY_MS)),
            signal: signal.clone(),
            ..Default::default()
        };
        let auth = match &provider.model {
            Some(model) => model_runtime.get_auth_for_model(model, overrides).await,
            None => model_runtime.get_auth(&provider.id, overrides).await,
        };
        let value = get_auth_credential(auth.ok().flatten().as_ref());
        if let Some(value) = value {
            credentials.push((provider.id.clone(), value));
        }
    }
    credentials
}

fn credential_matches_kind(kind: CredentialPrintKind, credential_type: Option<CredentialType>) -> bool {
    match kind {
        CredentialPrintKind::ApiKey => credential_type != Some(CredentialType::Oauth),
        CredentialPrintKind::BearerToken => credential_type == Some(CredentialType::Oauth),
    }
}

fn empty_credentials_error(
    target: &super::auth_command::AuthCommandTarget,
    provider: Option<&CredentialProvider>,
    kind: CredentialPrintKind,
    credential_types: &[CredentialInfo],
) -> CredentialPrintError {
    let provider_id = provider.map(|provider| provider.id.as_str());
    let credential_type = provider_id.and_then(|provider_id| credential_type(credential_types, provider_id));

    if target.provider().is_some()
        && kind == CredentialPrintKind::ApiKey
        && credential_type == Some(CredentialType::Oauth)
    {
        return command_error(format!(
            "Provider \"{}\" is configured with OAuth, not an API key",
            provider_id.unwrap_or_default()
        ));
    }
    if target.provider().is_some()
        && kind == CredentialPrintKind::BearerToken
        && credential_type != Some(CredentialType::Oauth)
    {
        return command_error(format!(
            "Provider \"{}\" is not configured with an OAuth bearer token",
            provider_id.unwrap_or_default()
        ));
    }

    command_error(match kind {
        CredentialPrintKind::ApiKey => "No usable API key is configured",
        CredentialPrintKind::BearerToken => "No usable OAuth bearer token is configured",
    })
}
