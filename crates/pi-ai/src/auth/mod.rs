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
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthContext, AuthError, AuthEvent,
    AuthInfoLink, AuthInteraction, AuthModel, AuthOperationOptions, AuthPrompt, AuthPromptKind, AuthResult,
    AuthSelectOption, AuthType, Credential, CredentialInfo, CredentialModifier, CredentialStore, CredentialType,
    ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth,
};
