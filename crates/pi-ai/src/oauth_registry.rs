//! Frozen registry and successful OAuth-result helper. Provider flow revalidation is separate.
use super::{OAuthCredentials, OAuthLoginCallbacks};
use crate::{
    auth::{AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, OAuthAuth},
    request::{BoxFuture, CallbackError},
    types::Model,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

pub type OAuthProviderId = String;
pub type OAuthProvider = OAuthProviderId;
pub trait OAuthProviderInterface: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn uses_callback_server(&self) -> Option<bool> {
        None
    }
    fn login(&self, callbacks: Arc<dyn OAuthLoginCallbacks>)
    -> BoxFuture<'static, Result<OAuthCredentials, AuthError>>;
    fn refresh_token(&self, credentials: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>>;
    fn get_api_key(&self, credentials: &OAuthCredentials) -> Result<JsString, CallbackError>;
    fn modify_models(&self, models: Vec<Model>, _credentials: &OAuthCredentials) -> Result<Vec<Model>, CallbackError> {
        Ok(models)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthProviderInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OAuthApiKey {
    pub new_credentials: OAuthCredentials,
    pub api_key: JsString,
}

pub use crate::auth::oauth::device_code::*;
pub use crate::auth::oauth::github_copilot::{get_github_copilot_base_url, normalize_domain};
pub use crate::auth::oauth::openai_codex::{OPENAI_CODEX_BROWSER_LOGIN_METHOD, OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD};

fn builtin_provider(id: &str) -> Arc<dyn OAuthProviderInterface> {
    builtins()
        .iter()
        .find(|provider| provider.id() == id)
        .expect("frozen builtin ID")
        .clone()
}
pub fn anthropic_oauth_provider() -> Arc<dyn OAuthProviderInterface> {
    builtin_provider("anthropic")
}
pub fn github_copilot_oauth_provider() -> Arc<dyn OAuthProviderInterface> {
    builtin_provider("github-copilot")
}
pub fn openai_codex_oauth_provider() -> Arc<dyn OAuthProviderInterface> {
    builtin_provider("openai-codex")
}
pub fn openai_codex_gc_oauth_provider() -> Arc<dyn OAuthProviderInterface> {
    builtin_provider("openai-codex-gc")
}
pub fn openai_codex_team_oauth_provider() -> Arc<dyn OAuthProviderInterface> {
    builtin_provider("openai-codex-team")
}

pub async fn login_anthropic(callbacks: Arc<dyn OAuthLoginCallbacks>) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::anthropic::AnthropicOAuth::new()
        .login(Arc::new(Callbacks(callbacks)))
        .await
}
pub async fn refresh_anthropic_token(refresh_token: &JsString) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::anthropic::AnthropicOAuth::new()
        .refresh(OAuthCredentials::new(refresh_token, "", 0.0))
        .await
}
pub async fn login_github_copilot(callbacks: Arc<dyn OAuthLoginCallbacks>) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::github_copilot::GitHubCopilotOAuth::new()
        .login(Arc::new(Callbacks(callbacks)))
        .await
}
pub async fn refresh_github_copilot_token(
    refresh_token: &JsString,
    enterprise_domain: Option<&str>,
) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::github_copilot::refresh_token(
        &crate::auth::oauth::support::OAuthRuntime::system(),
        refresh_token,
        enterprise_domain,
    )
    .await
}
pub async fn login_openai_codex(
    callbacks: Arc<dyn OAuthLoginCallbacks>,
    originator: Option<JsString>,
) -> Result<OAuthCredentials, AuthError> {
    let runtime = crate::auth::oauth::support::OAuthRuntime::system();
    let host = crate::utils::provider_env::get_provider_env_value("PI_OAUTH_CALLBACK_HOST", None)
        .unwrap_or_else(|| "127.0.0.1".into());
    crate::auth::oauth::openai_codex::login_browser(
        &runtime,
        &host,
        Arc::new(Callbacks(callbacks)),
        &originator.unwrap_or_else(|| "pi".into()),
    )
    .await
}
pub async fn login_openai_codex_device_code(
    callbacks: Arc<dyn OAuthLoginCallbacks>,
) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::openai_codex::login_device_code(
        &crate::auth::oauth::support::OAuthRuntime::system(),
        Arc::new(Callbacks(callbacks)),
    )
    .await
}
pub async fn refresh_openai_codex_token(refresh_token: &JsString) -> Result<OAuthCredentials, AuthError> {
    crate::auth::oauth::openai_codex::OpenAICodexOAuth::new()
        .refresh(OAuthCredentials::new(refresh_token, "", 0.0))
        .await
}

fn builtins() -> &'static Vec<Arc<dyn OAuthProviderInterface>> {
    static BUILTINS: OnceLock<Vec<Arc<dyn OAuthProviderInterface>>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        vec![
            Arc::new(Builtin {
                id: "anthropic",
                name: "Anthropic (Claude Pro/Max)",
                flow: Arc::new(crate::auth::oauth::anthropic::AnthropicOAuth::new()),
            }),
            Arc::new(Builtin {
                id: "github-copilot",
                name: "GitHub Copilot",
                flow: Arc::new(crate::auth::oauth::github_copilot::GitHubCopilotOAuth::new()),
            }),
            Arc::new(Builtin {
                id: "openai-codex",
                name: "ChatGPT Plus/Pro (Codex Subscription)",
                flow: Arc::new(crate::auth::oauth::openai_codex::OpenAICodexOAuth::new()),
            }),
            Arc::new(Builtin {
                id: "openai-codex-gc",
                name: "ChatGPT Plus/Pro (Codex Subscription, GlobalComix)",
                flow: Arc::new(crate::auth::oauth::openai_codex::OpenAICodexOAuth::new()),
            }),
            Arc::new(Builtin {
                id: "openai-codex-team",
                name: "ChatGPT (Codex Subscription, Team)",
                flow: Arc::new(crate::auth::oauth::openai_codex::OpenAICodexOAuth::new()),
            }),
        ]
    })
}
fn registry() -> &'static Mutex<Vec<Arc<dyn OAuthProviderInterface>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Arc<dyn OAuthProviderInterface>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(builtins().clone()))
}
pub fn get_oauth_provider(id: &str) -> Option<Arc<dyn OAuthProviderInterface>> {
    registry().lock().unwrap().iter().find(|p| p.id() == id).cloned()
}
pub fn register_oauth_provider(provider: Arc<dyn OAuthProviderInterface>) {
    let mut all = registry().lock().unwrap();
    if let Some(index) = all.iter().position(|p| p.id() == provider.id()) {
        all[index] = provider
    } else {
        all.push(provider)
    }
}
pub fn unregister_oauth_provider(id: &str) {
    if let Some(builtin) = builtins().iter().find(|p| p.id() == id) {
        register_oauth_provider(builtin.clone());
        return;
    }
    registry().lock().unwrap().retain(|p| p.id() != id);
}
pub fn reset_oauth_providers() {
    *registry().lock().unwrap() = builtins().clone();
}
pub fn get_oauth_providers() -> Vec<Arc<dyn OAuthProviderInterface>> {
    registry().lock().unwrap().clone()
}
pub fn get_oauth_provider_info_list() -> Vec<OAuthProviderInfo> {
    get_oauth_providers()
        .iter()
        .map(|p| OAuthProviderInfo {
            id: p.id().to_owned(),
            name: p.name().to_owned(),
            available: true,
        })
        .collect()
}
pub async fn refresh_oauth_token(
    provider_id: &str,
    credentials: OAuthCredentials,
) -> Result<OAuthCredentials, AuthError> {
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| AuthError::failed(format!("Unknown OAuth provider: {provider_id}")))?;
    provider.refresh_token(credentials).await
}
pub async fn get_oauth_api_key(
    provider_id: &str,
    credentials: &HashMap<String, OAuthCredentials>,
) -> Result<Option<OAuthApiKey>, AuthError> {
    let provider = get_oauth_provider(provider_id)
        .ok_or_else(|| AuthError::failed(format!("Unknown OAuth provider: {provider_id}")))?;
    let Some(mut credential) = credentials.get(provider_id).cloned() else {
        return Ok(None);
    };
    if now_millis() >= credential.expires {
        credential = match provider.refresh_token(credential).await {
            Ok(credential) => credential,
            Err(error) => {
                let error = error.into_callback();
                let message = error.error_or_string_message().map_err(AuthError::from)?;
                return Err(AuthError::from(CallbackError::from(
                    pi_js_compat::node_error::NativeNodeError::new(
                        "Error",
                        pi_js_compat::js_format!("Failed to refresh OAuth token for {}: {}", provider_id, message),
                    )
                    .with_cause(error.thrown_value()),
                )));
            }
        };
    }
    let api_key = provider.get_api_key(&credential).map_err(AuthError::from)?;
    Ok(Some(OAuthApiKey {
        new_credentials: credential,
        api_key,
    }))
}
fn now_millis() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}

// Frozen legacy provider objects share native flow implementations through this callback bridge.
struct Builtin {
    id: &'static str,
    name: &'static str,
    flow: Arc<dyn OAuthAuth>,
}
impl OAuthProviderInterface for Builtin {
    fn id(&self) -> &str {
        self.id
    }
    fn name(&self) -> &str {
        self.name
    }
    fn uses_callback_server(&self) -> Option<bool> {
        if self.id == "github-copilot" { None } else { Some(true) }
    }
    fn login(
        &self,
        callbacks: Arc<dyn OAuthLoginCallbacks>,
    ) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        self.flow.login(Arc::new(Callbacks(callbacks)))
    }
    fn refresh_token(&self, credentials: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        if self.id == "github-copilot" {
            return Box::pin(async move {
                let domain = credentials
                    .extra
                    .get(&JsString::from("enterpriseUrl"))
                    .and_then(JsValue::as_string)
                    .map(JsString::to_utf8)
                    .transpose()
                    .map_err(|_| AuthError::failed("Invalid GitHub Enterprise URL/domain"))?;
                refresh_github_copilot_token(&credentials.refresh, domain.as_deref()).await
            });
        }
        self.flow.refresh(credentials)
    }
    fn get_api_key(&self, credentials: &OAuthCredentials) -> Result<JsString, CallbackError> {
        Ok(credentials.access.clone())
    }
    fn modify_models(&self, models: Vec<Model>, credentials: &OAuthCredentials) -> Result<Vec<Model>, CallbackError> {
        if self.id != "github-copilot" {
            return Ok(models);
        }
        use crate::auth::oauth::github_copilot::{get_base_url_from_token, normalize_domain};
        let domain = credentials
            .extra
            .get(&pi_js_compat::js_json::JsString::from("enterpriseUrl"))
            .and_then(JsValue::as_string)
            .and_then(normalize_domain);
        let base_url = get_base_url_from_token(&credentials.access).unwrap_or_else(|| match domain {
            Some(domain) => pi_js_compat::js_format!("https://copilot-api.{}", domain),
            None => "https://api.individual.githubcopilot.com".into(),
        });
        let available = credentials
            .extra
            .get(&pi_js_compat::js_json::JsString::from("availableModelIds"))
            .map(|ids| ids.as_array().expect("availableModelIds must be an array"));
        Ok(models
            .into_iter()
            .filter_map(|mut model| {
                if model.provider != "github-copilot" {
                    return Some(model);
                }
                if available.is_some_and(|ids| !ids.iter().any(|id| id.as_string() == Some(&JsString::from(&model.id))))
                {
                    return None;
                }
                model.base_url = base_url.clone();
                Some(model)
            })
            .collect())
    }
}
struct Callbacks(Arc<dyn OAuthLoginCallbacks>);
impl AuthInteraction for Callbacks {
    fn signal(&self) -> AbortSignal {
        self.0.signal().unwrap_or_default()
    }
    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            match prompt.kind {
                AuthPromptKind::Text { message, placeholder } | AuthPromptKind::Secret { message, placeholder } => {
                    let allow_empty =
                        (message == "GitHub Enterprise URL/domain (blank for github.com)").then_some(true);
                    self.0
                        .on_prompt(super::OAuthPrompt {
                            message,
                            placeholder,
                            allow_empty,
                        })
                        .await
                }
                AuthPromptKind::ManualCode { .. } => match self.0.on_manual_code_input() {
                    Some(input) => input.await,
                    None => std::future::pending().await,
                },
                AuthPromptKind::Select { message, options } => self
                    .0
                    .on_select(super::OAuthSelectPrompt {
                        message,
                        options: options
                            .into_iter()
                            .map(|o| super::OAuthSelectOption {
                                id: o.id,
                                label: o.label,
                            })
                            .collect(),
                    })
                    .await?
                    .filter(|method| !method.is_empty())
                    .ok_or_else(|| CallbackError::new("Login cancelled")),
            }
        })
    }
    fn notify(&self, event: AuthEvent) -> Result<(), CallbackError> {
        match event {
            AuthEvent::AuthUrl { url, instructions } => self.0.on_auth(super::OAuthAuthInfo { url, instructions }),
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                interval_seconds,
                expires_in_seconds,
            } => self.0.on_device_code(super::OAuthDeviceCodeInfo {
                user_code,
                verification_uri,
                interval_seconds,
                expires_in_seconds,
            }),
            AuthEvent::Progress { message } => self.0.on_progress(&message),
        }
    }
}
