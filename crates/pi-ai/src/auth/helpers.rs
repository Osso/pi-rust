//! Port of `packages/ai/src/auth/helpers.ts`.

use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;
use tokio::sync::Mutex;

use crate::request::BoxFuture;

use super::types::{
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthError, AuthInteraction,
    AuthPrompt, AuthPromptKind, AuthResult, ModelAuth, OAuthAuth, OAuthCredential,
};

/// A stored credential owns resolution, even when its key is empty.
/// Without a stored credential, the first nonempty env var resolves.
pub struct EnvApiKeyAuth {
    name: String,
    env_vars: Vec<String>,
}

/// `envApiKeyAuth(name, envVars)`.
pub fn env_api_key_auth(name: impl Into<String>, env_vars: &[&str]) -> EnvApiKeyAuth {
    EnvApiKeyAuth {
        name: name.into(),
        env_vars: env_vars.iter().map(|value| (*value).to_owned()).collect(),
    }
}

impl ApiKeyAuth for EnvApiKeyAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        let name = self.name.clone();
        Some(Box::pin(async move {
            let signal = interaction.signal();
            check_aborted(&signal)?;
            let answer = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Secret {
                        message: pi_js_compat::js_format!("Enter {}", name),
                        placeholder: None,
                    },
                })
                .await
                .map_err(AuthError::Failed)?;
            check_aborted(&signal)?;
            Ok(ApiKeyCredential {
                key: Some(answer),
                env: None,
            })
        }))
    }

    fn check(&self, _input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
        None
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let env_vars = self.env_vars.clone();
        Box::pin(async move {
            check_aborted(&input.signal)?;
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
                let value = input.ctx.env(&env_var).await;
                check_aborted(&input.signal)?;
                if let Some(value) = value
                    && !value.is_empty()
                {
                    return Ok(Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(value),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some((env_var).into()),
                    }));
                }
            }
            Ok(None)
        })
    }
}

fn check_aborted(signal: &AbortSignal) -> Result<(), AuthError> {
    if signal.aborted() {
        Err(AuthError::Aborted)
    } else {
        Ok(())
    }
}

/// Loader for [`lazy_oauth`].
pub type OAuthLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn OAuthAuth>, AuthError>> + Send + Sync>;

pub struct LazyOAuthInput {
    pub name: String,
    pub is_subscription: Option<bool>,
    pub login_label: Option<String>,
    pub load: OAuthLoader,
}

struct LazyOAuthInner {
    load: OAuthLoader,
    loaded: Mutex<Option<Result<Arc<dyn OAuthAuth>, AuthError>>>,
}

/// Wraps a dynamically loaded `OAuthAuth`, loading on first use.
pub struct LazyOAuth {
    name: String,
    is_subscription: bool,
    login_label: Option<String>,
    inner: Arc<LazyOAuthInner>,
}

/// `lazyOAuth(input)`.
pub fn lazy_oauth(input: LazyOAuthInput) -> LazyOAuth {
    LazyOAuth {
        name: input.name,
        is_subscription: input.is_subscription.unwrap_or(false),
        login_label: input.login_label,
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
    fn name(&self) -> &str {
        &self.name
    }

    fn is_subscription(&self) -> bool {
        self.is_subscription
    }

    fn login_label(&self) -> Option<&str> {
        self.login_label.as_deref()
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let implementation = Self::implementation(&inner).await?;
            implementation.login(interaction).await
        })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: AbortSignal,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let implementation = Self::implementation(&inner).await?;
            implementation.refresh(credential, signal).await
        })
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let implementation = Self::implementation(&inner).await?;
            implementation.to_auth(credential).await
        })
    }
}
