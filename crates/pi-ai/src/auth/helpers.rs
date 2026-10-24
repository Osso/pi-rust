//! Port of `packages/ai/src/auth/helpers.ts`.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::request::BoxFuture;
use pi_js_compat::js_json::JsString;

use super::types::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyLogin, ApiKeyResolveInput, AuthError, AuthInteraction, AuthPrompt,
    AuthPromptKind, AuthResult, ModelAuth, OAuthAuth, OAuthCredential,
};

/// A stored credential owns resolution, even when its key is empty.
/// Without a stored credential, the first nonempty env var resolves.
pub struct EnvApiKeyAuth {
    name: JsString,
    env_vars: Vec<String>,
}

/// `envApiKeyAuth(name, envVars)`.
pub fn env_api_key_auth(name: impl Into<JsString>, env_vars: &[&str]) -> EnvApiKeyAuth {
    EnvApiKeyAuth {
        name: name.into(),
        env_vars: env_vars.iter().map(|value| (*value).to_owned()).collect(),
    }
}

impl ApiKeyAuth for EnvApiKeyAuth {
    fn name(&self) -> JsString {
        self.name.clone()
    }

    fn login(&self) -> Option<ApiKeyLogin> {
        let name = self.name.clone();
        Some(Arc::new(move |interaction: Arc<dyn AuthInteraction>| {
            let name = name.clone();
            Box::pin(async move {
                let answer = interaction
                    .prompt(AuthPrompt {
                        signal: None,
                        kind: AuthPromptKind::Secret {
                            message: pi_js_compat::js_format!("Enter {}", name),
                            placeholder: None,
                        },
                    })
                    .await
                    .map_err(AuthError::from)?;
                Ok(ApiKeyCredential {
                    key: Some(answer),
                    env: None,
                })
            })
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let env_vars = self.env_vars.clone();
        Box::pin(async move {
            if let Some(credential) = input.credential {
                return Ok(credential.key.filter(|key| !key.is_empty()).map(|key| AuthResult {
                    auth: ModelAuth {
                        api_key: Some(key),
                        headers: None,
                        base_url: None,
                    },
                    env: None,
                    source: Some("stored credential".into()),
                }));
            }
            for env_var in env_vars {
                if let Some(value) = input.ctx.env(&env_var).await
                    && !value.is_empty()
                {
                    return Ok(Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(value),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some(env_var.into()),
                    }));
                }
            }
            Ok(None)
        })
    }
}

/// Loader for [`lazy_oauth`].
pub type OAuthLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn OAuthAuth>, AuthError>> + Send + Sync>;

pub struct LazyOAuthInput {
    pub name: JsString,
    pub load: OAuthLoader,
}

struct LazyOAuthInner {
    load: OAuthLoader,
    loaded: Mutex<Option<Result<Arc<dyn OAuthAuth>, AuthError>>>,
}

/// Wraps a dynamically loaded `OAuthAuth`, loading on first use.
pub struct LazyOAuth {
    name: JsString,
    inner: Arc<LazyOAuthInner>,
}

/// `lazyOAuth(input)`.
pub fn lazy_oauth(input: LazyOAuthInput) -> LazyOAuth {
    LazyOAuth {
        name: input.name,
        inner: Arc::new(LazyOAuthInner {
            load: input.load,
            loaded: Mutex::new(None),
        }),
    }
}

impl LazyOAuth {
    async fn implementation(inner: &LazyOAuthInner) -> Result<Arc<dyn OAuthAuth>, AuthError> {
        let mut loaded = inner.loaded.lock().await;
        if let Some(implementation) = loaded.as_ref() {
            return implementation.clone();
        }
        let implementation = (inner.load)().await;
        *loaded = Some(implementation.clone());
        implementation
    }
}

impl OAuthAuth for LazyOAuth {
    fn name(&self) -> JsString {
        self.name.clone()
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move { Self::implementation(&inner).await?.login(interaction).await })
    }

    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move { Self::implementation(&inner).await?.refresh(credential).await })
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move { Self::implementation(&inner).await?.to_auth(credential).await })
    }
}
