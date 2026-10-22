//! Port of `packages/ai/src/auth/types.ts`.
//!
//! Credentials are tagged by `type` on the wire. Extra OAuth keys are kept
//! verbatim in [`OAuthCredential::extra`] so round-trips do not drop
//! provider-specific fields.

use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::request::{BoxFuture, CallbackError};
use crate::types::{ProviderEnv, ProviderHeaders};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};

/// `ModelAuth`: request auth for a single model request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAuth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<ProviderHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// `ApiKeyCredential`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyCredential {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
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
}

impl OAuthCredential {
    pub fn new(refresh: impl Into<JsString>, access: impl Into<JsString>, expires: f64) -> Self {
        Self {
            refresh: refresh.into(),
            access: access.into(),
            expires,
            extra: JsObject::new(),
        }
    }

    /// Checked handoff to scalar-string-only request providers and HTTP builders.
    pub fn access_utf8(&self) -> Result<String, AuthError> {
        self.access
            .to_utf8()
            .map_err(|e| AuthError::failed(format!("Cannot hand OAuth access token to request provider: {e}")))
    }

    /// Node HTTP header validation: tab, printable ASCII and Latin-1 only.
    pub fn refresh_header_value(&self, header: &str) -> Result<String, AuthError> {
        let invalid = self
            .refresh
            .units()
            .iter()
            .any(|unit| !matches!(*unit, 9 | 32..=126 | 128..=255));
        if invalid {
            let mut error = CallbackError::new(format!("Invalid character in header content [\"{header}\"]"));
            error.name = "TypeError".into();
            error.code = Some("ERR_INVALID_CHAR".into());
            return Err(AuthError::Failed(error));
        }
        Ok(self.refresh.to_utf8().expect("validated header units are scalar"))
    }

    pub fn to_js_value(&self) -> JsValue {
        let mut object = JsObject::new();
        object.insert("type".into(), JsValue::String("oauth".into()));
        object.insert("refresh".into(), JsValue::String(self.refresh.clone()));
        object.insert("access".into(), JsValue::String(self.access.clone()));
        object.insert("expires".into(), JsValue::Number(self.expires));
        object.extend(self.extra.clone());
        JsValue::Object(object)
    }

    pub fn from_js_value(value: &JsValue) -> Result<Self, String> {
        let mut object = value.as_object().cloned().ok_or("Expected OAuth object")?;
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
        let mut object = Map::new();
        match self {
            Self::ApiKey(credential) => {
                object.insert("type".to_owned(), Value::String("api_key".to_owned()));
                if let Some(key) = &credential.key {
                    object.insert("key".to_owned(), Value::String(key.clone()));
                }
                if let Some(env) = &credential.env {
                    object.insert(
                        "env".to_owned(),
                        serde_json::to_value(env).map_err(serde::ser::Error::custom)?,
                    );
                }
            }
            Self::OAuth(credential) => {
                return credential
                    .to_js_value()
                    .to_serde()
                    .map_err(serde::ser::Error::custom)?
                    .serialize(serializer);
            }
        }
        object.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Credential {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut object = Map::<String, Value>::deserialize(deserializer)?;
        let tag = object
            .remove("type")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| D::Error::missing_field("type"))?;
        match tag.as_str() {
            "api_key" => Ok(Self::ApiKey(ApiKeyCredential {
                key: take_string(&mut object, "key"),
                env: take_value(&mut object, "env")
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(D::Error::custom)?,
            })),
            "oauth" => {
                let refresh = take_string(&mut object, "refresh").ok_or_else(|| D::Error::missing_field("refresh"))?;
                let access = take_string(&mut object, "access").ok_or_else(|| D::Error::missing_field("access"))?;
                let expires = take_value(&mut object, "expires")
                    .and_then(|value| value.as_f64())
                    .ok_or_else(|| D::Error::missing_field("expires"))?;
                Ok(Self::OAuth(OAuthCredential {
                    refresh: refresh.into(),
                    access: access.into(),
                    expires,
                    extra: object
                        .iter()
                        .map(|(key, value)| (key.into(), JsValue::from(value)))
                        .collect(),
                }))
            }
            other => Err(D::Error::custom(format!("unknown credential type {other:?}"))),
        }
    }
}

fn take_string(object: &mut Map<String, Value>, key: &str) -> Option<String> {
    match object.remove(key) {
        Some(Value::String(value)) => Some(value),
        _ => None,
    }
}

fn take_value(object: &mut Map<String, Value>, key: &str) -> Option<Value> {
    object.remove(key)
}

/// Non-secret credential metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInfo {
    pub provider_id: String,
    #[serde(rename = "type")]
    pub credential_type: CredentialType,
}

/// Optional cancellation for public auth and credential operations.
#[derive(Debug, Clone, Default)]
pub struct AuthOperationOptions {
    pub signal: Option<CancellationToken>,
}

impl AuthOperationOptions {
    pub fn new(signal: Option<CancellationToken>) -> Self {
        Self { signal }
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
    pub source: Option<String>,
}

/// Side-effect-free availability check result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCheck {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub auth_type: AuthType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    ApiKey,
    Oauth,
}

impl AuthType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Oauth => "oauth",
        }
    }
}

/// Environment access for auth resolution.
pub trait AuthContext: Send + Sync {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>>;
    /// Supports a leading `~`.
    fn file_exists(&self, path: &str) -> BoxFuture<'_, bool>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSelectOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

/// Prompt shown to the user during login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    /// Cancels a pending prompt when an out-of-band event resolves the step.
    pub signal: Option<CancellationToken>,
    pub kind: AuthPromptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthPromptKind {
    Text {
        message: String,
        placeholder: Option<String>,
    },
    Secret {
        message: String,
        placeholder: Option<String>,
    },
    Select {
        message: String,
        options: Vec<AuthSelectOption>,
    },
    ManualCode {
        message: String,
        placeholder: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthInfoLink {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthEvent {
    Info {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        links: Option<Vec<AuthInfoLink>>,
    },
    AuthUrl {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
    },
    DeviceCode {
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "verificationUri")]
        verification_uri: String,
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
        message: String,
    },
}

/// Login interaction callbacks serving both api-key and OAuth flows.
pub trait AuthInteraction: Send + Sync {
    /// Aborts the whole login flow.
    fn signal(&self) -> CancellationToken;
    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, CallbackError>>;
    fn notify(&self, event: AuthEvent);
}

/// Failure from auth callbacks and credential stores. Aborts are distinct so
/// callers never wrap cancellation as a provider/auth error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Aborted,
    Failed(CallbackError),
}

impl AuthError {
    pub fn failed(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::Failed(CallbackError::new(message))
    }

    pub fn detail(&self) -> Option<&pi_js_compat::js_json::JsString> {
        match self {
            Self::Aborted => None,
            Self::Failed(error) => Some(&error.message),
        }
    }
}

impl From<CallbackError> for AuthError {
    fn from(error: CallbackError) -> Self {
        Self::Failed(error)
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aborted => formatter.write_str("The operation was aborted"),
            Self::Failed(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Inputs to `ApiKeyAuth::resolve`.
pub struct ApiKeyResolveInput {
    pub ctx: Arc<dyn AuthContext>,
    pub credential: Option<ApiKeyCredential>,
    pub signal: CancellationToken,
}

/// Inputs to `ApiKeyAuth::check`.
pub struct ApiKeyCheckInput {
    pub ctx: Arc<dyn AuthContext>,
    pub credential: Option<ApiKeyCredential>,
    pub signal: CancellationToken,
}

/// Api-key auth: stored key/provider env plus ambient sources.
pub trait ApiKeyAuth: Send + Sync {
    fn name(&self) -> &str;

    /// Interactive setup; `None` means ambient-only.
    fn login(
        &self,
        _interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        None
    }

    /// Optional side-effect-free availability check.
    fn check(&self, _input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
        None
    }

    /// Whether [`ApiKeyAuth::login`] is implemented. Ambient-only providers
    /// report `false`; callers use this instead of invoking `login`.
    fn supports_login(&self) -> bool {
        true
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>>;
}

/// OAuth auth. `Models` owns the locked refresh pattern.
pub trait OAuthAuth: Send + Sync {
    fn name(&self) -> &str;

    fn is_subscription(&self) -> bool {
        false
    }

    fn login_label(&self) -> Option<&str> {
        None
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>>;

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>>;

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
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>>;

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>>;

    /// Serialized read-modify-write; `Ok(None)` leaves the entry unchanged.
    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>>;

    fn delete(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>>;
}
