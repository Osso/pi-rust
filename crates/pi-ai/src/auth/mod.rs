//! Auth contracts and runtime from `packages/ai/src/auth/`.

pub mod context;
pub mod credential_store;
pub mod helpers;
pub mod oauth;
pub mod resolve;
pub mod types;

pub use context::{DefaultProviderAuthContext, default_provider_auth_context};
pub use credential_store::InMemoryCredentialStore;
pub use helpers::{EnvApiKeyAuth, LazyOAuth, LazyOAuthInput, OAuthLoader, env_api_key_auth, lazy_oauth};
pub use resolve::{AuthResolutionOverrides, ModelsError, ModelsErrorCode, resolve_provider_auth};
pub use types::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyLogin, ApiKeyResolveInput, AuthContext, AuthError, AuthEvent, AuthInteraction,
    AuthModel, AuthPrompt, AuthPromptKind, AuthResult, AuthSelectOption, Credential, CredentialModifier,
    CredentialStore, CredentialType, ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth,
};
