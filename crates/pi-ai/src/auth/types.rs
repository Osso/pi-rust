//! Port of `packages/ai/src/auth/types.ts`.
//!
//! Credentials are tagged by `type` on the wire. Extra OAuth keys are kept
//! verbatim in [`OAuthCredential::extra`] so round-trips do not drop
//! provider-specific fields.

use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::request::{BoxFuture, CallbackError};
use crate::types::{ProviderEnv, ProviderHeaders};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};

/// `ModelAuth`: request auth for a single model request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAuth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<ProviderHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<JsString>,
}

/// `ApiKeyCredential`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyCredential {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<ProviderEnv>,
}

/// `OAuthCredential`: required token fields plus provider-specific extra keys.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthCredential {
    pub refresh: JsString,
    pub access: JsString,
    /// Unix timestamp in milliseconds.
    pub expires: f64,
    /// Fields outside `refresh`/`access`/`expires`, preserved verbatim.
    pub extra: JsObject,
    /// Property insertion order, retained across typed token updates.
    pub key_order: Vec<JsString>,
}

impl OAuthCredential {
    pub fn new(refresh: impl Into<JsString>, access: impl Into<JsString>, expires: f64) -> Self {
        Self {
            refresh: refresh.into(),
            access: access.into(),
            expires,
            extra: JsObject::new(),
            key_order: ["type", "refresh", "access", "expires"].map(JsString::from).to_vec(),
        }
    }

    pub fn to_js_value(&self) -> JsValue {
        let mut object = JsObject::new();
        object.insert("type".into(), JsValue::String("oauth".into()));
        object.insert("refresh".into(), JsValue::String(self.refresh.clone()));
        object.insert("access".into(), JsValue::String(self.access.clone()));
        object.insert("expires".into(), JsValue::Number(self.expires));
        object.extend(self.extra.clone());
        let mut ordered = JsObject::new();
        for key in &self.key_order {
            if let Some(value) = object.shift_remove(key) {
                ordered.insert(key.clone(), value);
            }
        }
        ordered.extend(object);
        JsValue::Object(ordered)
    }

    pub fn from_js_value(value: &JsValue) -> Result<Self, String> {
        let mut object = value.as_object().cloned().ok_or("Expected OAuth object")?;
        let key_order = object.keys().cloned().collect();
        let refresh = object
            .shift_remove(&pi_js_compat::js_json::JsString::from("refresh"))
            .and_then(|v| v.as_string().cloned())
            .ok_or("Missing refresh")?;
        let access = object
            .shift_remove(&pi_js_compat::js_json::JsString::from("access"))
            .and_then(|v| v.as_string().cloned())
            .ok_or("Missing access")?;
        let expires = object
            .shift_remove(&pi_js_compat::js_json::JsString::from("expires"))
            .and_then(|v| v.as_f64())
            .ok_or("Missing expires")?;
        object.shift_remove(&pi_js_compat::js_json::JsString::from("type"));
        Ok(Self {
            refresh,
            access,
            expires,
            extra: object,
            key_order,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialType {
    ApiKey,
    Oauth,
}

impl CredentialType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Oauth => "oauth",
        }
    }
}

/// `Credential`: one type-tagged credential per provider.
#[derive(Debug, Clone, PartialEq)]
pub enum Credential {
    ApiKey(ApiKeyCredential),
    OAuth(OAuthCredential),
}

impl Credential {
    pub const fn credential_type(&self) -> CredentialType {
        match self {
            Self::ApiKey(_) => CredentialType::ApiKey,
            Self::OAuth(_) => CredentialType::Oauth,
        }
    }

    pub fn as_api_key(&self) -> Option<&ApiKeyCredential> {
        match self {
            Self::ApiKey(credential) => Some(credential),
            Self::OAuth(_) => None,
        }
    }

    pub fn as_oauth(&self) -> Option<&OAuthCredential> {
        match self {
            Self::ApiKey(_) => None,
            Self::OAuth(credential) => Some(credential),
        }
    }

    pub fn into_oauth(self) -> Option<OAuthCredential> {
        match self {
            Self::ApiKey(_) => None,
            Self::OAuth(credential) => Some(credential),
        }
    }
}

impl Serialize for Credential {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            Self::ApiKey(credential) => {
                let mut object = JsObject::new();
                object.insert("type".into(), JsValue::String("api_key".into()));
                if let Some(key) = &credential.key {
                    object.insert("key".into(), JsValue::String(key.clone()));
                }
                if let Some(env) = &credential.env {
                    object.insert(
                        "env".into(),
                        pi_js_compat::js_json::to_value(env).map_err(serde::ser::Error::custom)?,
                    );
                }
                JsValue::Object(object)
            }
            Self::OAuth(credential) => credential.to_js_value(),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Credential {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("Expected credential object"))?;
        let tag = object
            .get(&JsString::from("type"))
            .and_then(JsValue::as_string)
            .ok_or_else(|| D::Error::missing_field("type"))?;
        if tag == "oauth" {
            return OAuthCredential::from_js_value(&value)
                .map(Self::OAuth)
                .map_err(D::Error::custom);
        }
        if tag == "api_key" {
            let key = object.get(&JsString::from("key")).and_then(JsValue::as_string).cloned();
            let env = object
                .get(&JsString::from("env"))
                .map(pi_js_compat::js_json::from_value::<Option<ProviderEnv>>)
                .transpose()
                .map_err(D::Error::custom)?
                .flatten();
            return Ok(Self::ApiKey(ApiKeyCredential { key, env }));
        }
        Err(D::Error::custom("unknown credential type"))
    }
}

/// Result of resolving auth for a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResult {
    pub auth: ModelAuth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<ProviderEnv>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<JsString>,
}

/// Environment access for auth resolution.
pub trait AuthContext: Send + Sync {
    fn env(&self, name: &str) -> BoxFuture<'_, Result<Option<JsString>, AuthError>>;
    /// Supports a leading `~`.
    fn file_exists(&self, path: &JsString) -> BoxFuture<'_, Result<bool, AuthError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSelectOption {
    pub id: JsString,
    pub label: JsString,
    pub description: Option<JsString>,
}

/// Prompt shown to the user during login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    /// Cancels a pending prompt when an out-of-band event resolves the step.
    pub signal: Option<AbortSignal>,
    pub kind: AuthPromptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthPromptKind {
    Text {
        message: JsString,
        placeholder: Option<JsString>,
    },
    Secret {
        message: JsString,
        placeholder: Option<JsString>,
    },
    Select {
        message: JsString,
        options: Vec<AuthSelectOption>,
    },
    ManualCode {
        message: JsString,
        placeholder: Option<JsString>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthEvent {
    AuthUrl {
        url: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        instructions: Option<JsString>,
    },
    DeviceCode {
        #[serde(rename = "userCode")]
        user_code: JsString,
        #[serde(rename = "verificationUri")]
        verification_uri: JsString,
        #[serde(
            rename = "intervalSeconds",
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        interval_seconds: Option<f64>,
        #[serde(
            rename = "expiresInSeconds",
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        expires_in_seconds: Option<f64>,
    },
    Progress {
        message: JsString,
    },
}

impl<'de> Deserialize<'de> for AuthEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let field = |name: &str| -> Result<JsString, D::Error> {
            value
                .get(name)
                .and_then(JsValue::as_string)
                .cloned()
                .ok_or_else(|| D::Error::custom(format!("Missing auth event string: {name}")))
        };
        let optional = |name: &str| -> Result<Option<JsString>, D::Error> {
            value
                .get(name)
                .map(pi_js_compat::js_json::from_value::<Option<JsString>>)
                .transpose()
                .map(Option::flatten)
                .map_err(D::Error::custom)
        };
        let tag = field("type")?;
        if tag == "auth_url" {
            return Ok(Self::AuthUrl {
                url: field("url")?,
                instructions: optional("instructions")?,
            });
        }
        if tag == "progress" {
            return Ok(Self::Progress {
                message: field("message")?,
            });
        }
        if tag == "device_code" {
            return Ok(Self::DeviceCode {
                user_code: field("userCode")?,
                verification_uri: field("verificationUri")?,
                interval_seconds: value
                    .get("intervalSeconds")
                    .map(pi_js_compat::js_json::from_value::<Option<f64>>)
                    .transpose()
                    .map_err(D::Error::custom)?
                    .flatten(),
                expires_in_seconds: value
                    .get("expiresInSeconds")
                    .map(pi_js_compat::js_json::from_value::<Option<f64>>)
                    .transpose()
                    .map_err(D::Error::custom)?
                    .flatten(),
            });
        }
        Err(D::Error::custom("unknown auth event type"))
    }
}

/// Login interaction callbacks serving both api-key and OAuth flows.
pub trait AuthInteraction: Send + Sync {
    /// Aborts the whole login flow.
    fn signal(&self) -> AbortSignal;
    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>>;
    fn notify(&self, event: AuthEvent) -> Result<(), CallbackError>;
}

/// Failure from auth callbacks and credential stores.
/// Cancellation and callback failure retain the exact thrown value.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthError {
    Aborted(Box<CallbackError>),
    Failed(Box<CallbackError>),
}

impl AuthError {
    pub fn failed(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::from(CallbackError::new(message))
    }

    pub fn from_abort_reason(reason: pi_js_compat::abort::AbortReason) -> Self {
        Self::Aborted(Box::new(CallbackError::from_rejection(reason)))
    }

    pub fn check_aborted(signal: &AbortSignal) -> Result<(), Self> {
        match signal.reason() {
            Some(reason) => Err(Self::from_abort_reason(reason)),
            None => Ok(()),
        }
    }

    pub fn callback(&self) -> &CallbackError {
        match self {
            Self::Aborted(error) | Self::Failed(error) => error,
        }
    }

    pub fn into_callback(self) -> CallbackError {
        match self {
            Self::Aborted(error) | Self::Failed(error) => *error,
        }
    }

    pub fn thrown_value(&self) -> pi_js_compat::abort::AbortReason {
        self.callback().thrown_value()
    }

    pub fn detail(&self) -> Option<&pi_js_compat::js_json::JsString> {
        Some(&self.callback().message)
    }
}

impl From<CallbackError> for AuthError {
    fn from(error: CallbackError) -> Self {
        Self::Failed(Box::new(error))
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.callback())
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.callback())
    }
}

/// Model shape received by frozen auth resolution: chat or image generation.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthModel {
    Chat(crate::types::Model),
    Images(crate::types::ImagesModel),
}

impl AuthModel {
    pub fn provider(&self) -> &str {
        match self {
            Self::Chat(model) => &model.provider,
            Self::Images(model) => &model.provider,
        }
    }
}

/// Inputs to `ApiKeyAuth::resolve`.
pub struct ApiKeyResolveInput {
    pub model: AuthModel,
    pub ctx: Arc<dyn AuthContext>,
    pub credential: Option<ApiKeyCredential>,
}

/// Optional login function, present only for interactive auth methods.
pub type ApiKeyLogin =
    Arc<dyn Fn(Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<ApiKeyCredential, AuthError>> + Send + Sync>;

/// Api-key auth: stored key/provider env plus ambient sources.
pub trait ApiKeyAuth: Send + Sync {
    fn name(&self) -> JsString;

    /// Interactive setup; `None` means ambient-only.
    fn login(&self) -> Option<ApiKeyLogin> {
        None
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>>;
}

/// OAuth auth. `Models` owns the locked refresh pattern.
pub trait OAuthAuth: Send + Sync {
    fn name(&self) -> JsString;

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>>;

    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>>;

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>>;
}

/// Provider auth. At least one method is expected by callers.
#[derive(Clone, Default)]
pub struct ProviderAuth {
    pub api_key: Option<Arc<dyn ApiKeyAuth>>,
    pub oauth: Option<Arc<dyn OAuthAuth>>,
}

impl ProviderAuth {
    pub fn api_key(api_key: Arc<dyn ApiKeyAuth>) -> Self {
        Self {
            api_key: Some(api_key),
            oauth: None,
        }
    }

    pub fn oauth(oauth: Arc<dyn OAuthAuth>) -> Self {
        Self {
            api_key: None,
            oauth: Some(oauth),
        }
    }
}

/// Callback used by [`CredentialStore::modify`]; sees the current credential.
pub type CredentialModifier =
    Arc<dyn Fn(Option<Credential>) -> BoxFuture<'static, Result<Option<Credential>, AuthError>> + Send + Sync>;

/// App-owned credential storage, keyed by provider id.
pub trait CredentialStore: Send + Sync {
    fn read(&self, provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>>;

    /// Serialized read-modify-write; `Ok(None)` leaves the entry unchanged.
    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>>;

    fn delete(&self, provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>>;
}
