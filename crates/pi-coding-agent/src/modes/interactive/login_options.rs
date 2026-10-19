//! Provider login/logout option assembly (`interactive-mode.ts` helpers).

use pi_ai::auth::types::{AuthCheck, AuthType, CredentialType};
use pi_ai::models::Provider;
use pi_js_compat::locale_compare;

use crate::core::model_runtime::ModelRuntime;
use crate::modes::interactive::components::oauth_selector::{AuthSelectorProvider, format_auth_selector_provider_type};

/// One provider after merging its oauth/API-key options by id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginProviderCompletionOption {
    pub id: String,
    pub name: String,
    pub auth_types: Vec<AuthType>,
}

const AUTH_TYPE_ORDER: [AuthType; 2] = [AuthType::Oauth, AuthType::ApiKey];

fn auth_type_order(auth_type: AuthType) -> usize {
    AUTH_TYPE_ORDER
        .iter()
        .position(|candidate| *candidate == auth_type)
        .unwrap_or(AUTH_TYPE_ORDER.len())
}

fn includes_login_auth_type(requested: Option<AuthType>, provider_auth_type: AuthType) -> bool {
    requested.is_none_or(|requested| requested == provider_auth_type)
}

/// `createLoginOptionsForProvider`.
pub fn create_login_options_for_provider(
    provider: &dyn Provider,
    requested_auth_type: Option<AuthType>,
    status: Option<AuthCheck>,
) -> Vec<AuthSelectorProvider> {
    let base = AuthSelectorProvider {
        id: provider.id().to_owned(),
        name: provider.name().to_owned(),
        auth_type: AuthType::Oauth,
        method_name: None,
        status,
    };
    let auth = provider.auth();
    let mut options = Vec::new();
    if let Some(oauth) = &auth.oauth
        && includes_login_auth_type(requested_auth_type, AuthType::Oauth)
    {
        options.push(AuthSelectorProvider {
            auth_type: AuthType::Oauth,
            method_name: Some(oauth.name().to_owned()),
            ..base.clone()
        });
    }
    if let Some(api_key) = &auth.api_key
        && includes_login_auth_type(requested_auth_type, AuthType::ApiKey)
    {
        options.push(AuthSelectorProvider {
            auth_type: AuthType::ApiKey,
            method_name: Some(api_key.name().to_owned()),
            ..base
        });
    }
    options
}

/// `getLoginProviderOptions`: provider options sorted by display name.
pub fn get_login_provider_options(runtime: &ModelRuntime, auth_type: Option<AuthType>) -> Vec<AuthSelectorProvider> {
    let mut options: Vec<AuthSelectorProvider> = runtime
        .get_providers()
        .iter()
        .flat_map(|provider| {
            let auth_status = runtime.get_provider_auth_status(provider.id());
            let status = auth_status.configured.then(|| AuthCheck {
                auth_type: if runtime.is_using_oauth(provider.id()) {
                    AuthType::Oauth
                } else {
                    AuthType::ApiKey
                },
                source: auth_status
                    .label
                    .clone()
                    .or_else(|| auth_status.source.map(|source| source.as_str().to_owned())),
            });
            create_login_options_for_provider(provider.as_ref(), auth_type, status)
        })
        .collect();
    options.sort_by(|a, b| locale_compare(&a.name, &b.name));
    options
}

fn credential_auth_type(credential_type: CredentialType) -> AuthType {
    match credential_type {
        CredentialType::ApiKey => AuthType::ApiKey,
        CredentialType::Oauth => AuthType::Oauth,
    }
}

/// `getLogoutProviderOptions`.
pub async fn get_logout_provider_options(runtime: &ModelRuntime) -> Result<Vec<AuthSelectorProvider>, String> {
    let timeout = crate::main_runner::TimeoutSignal::new(15_000);
    let mut options: Vec<AuthSelectorProvider> = runtime
        .list_credentials(pi_ai::auth::types::AuthOperationOptions::new(Some(timeout.token())))
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|credential| {
            let auth_type = credential_auth_type(credential.credential_type);
            AuthSelectorProvider {
                name: runtime
                    .get_provider(&credential.provider_id)
                    .map(|provider| provider.name().to_owned())
                    .unwrap_or_else(|| credential.provider_id.clone()),
                id: credential.provider_id,
                auth_type,
                method_name: None,
                status: Some(AuthCheck {
                    auth_type,
                    source: Some("stored credential".to_owned()),
                }),
            }
        })
        .collect();
    options.sort_by(|a, b| locale_compare(&a.name, &b.name));
    Ok(options)
}

/// `getLoginProviderCompletionOptions`.
pub fn get_login_provider_completion_options(
    provider_options: &[AuthSelectorProvider],
) -> Vec<LoginProviderCompletionOption> {
    let mut by_id: Vec<LoginProviderCompletionOption> = Vec::new();
    for provider in provider_options {
        if let Some(existing) = by_id.iter_mut().find(|option| option.id == provider.id) {
            if !existing.auth_types.contains(&provider.auth_type) {
                existing.auth_types.push(provider.auth_type);
                existing.auth_types.sort_by_key(|auth_type| auth_type_order(*auth_type));
            }
            continue;
        }
        by_id.push(LoginProviderCompletionOption {
            id: provider.id.clone(),
            name: provider.name.clone(),
            auth_types: vec![provider.auth_type],
        });
    }
    by_id.sort_by(|a, b| locale_compare(&a.name, &b.name));
    by_id
}

/// `getLoginProviderSearchText`.
pub fn get_login_provider_search_text(provider: &LoginProviderCompletionOption) -> Vec<String> {
    let auth_types = provider
        .auth_types
        .iter()
        .map(|auth_type| {
            format!(
                "{} {}",
                auth_type.as_str(),
                format_auth_selector_provider_type(*auth_type)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    vec![format!("{} {} {}", provider.id, provider.name, auth_types)]
}

/// `formatLoginProviderCompletionDescription`.
pub fn format_login_provider_completion_description(provider: &LoginProviderCompletionOption) -> String {
    let auth_types = provider
        .auth_types
        .iter()
        .map(|auth_type| format_auth_selector_provider_type(*auth_type))
        .collect::<Vec<_>>()
        .join("/");
    if provider.name == provider.id {
        auth_types
    } else {
        format!("{} · {auth_types}", provider.name)
    }
}
