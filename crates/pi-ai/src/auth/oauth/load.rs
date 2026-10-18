//! Port of `packages/ai/src/auth/oauth/load.ts`.
//!
//! Rust links every ported flow into the binary, so the fallback branch
//! constructs the flow directly instead of dynamically importing a module.
//! Registered bundled loaders keep the standalone-binary override shape.

use std::sync::{Arc, OnceLock, RwLock};

use crate::auth::OAuthAuth;
use crate::auth::oauth::anthropic::AnthropicOAuth;
use crate::auth::oauth::github_copilot::GitHubCopilotOAuth;
use crate::auth::oauth::kimi_coding::KimiCodingOAuth;
use crate::auth::oauth::openai_codex::OpenAICodexOAuth;
use crate::auth::oauth::openrouter::OpenRouterOAuth;
use crate::auth::oauth::radius::{RadiusOAuthOptions, create_radius_oauth};
use crate::auth::oauth::xai::XaiOAuth;

type FlowLoader = Arc<dyn Fn() -> Arc<dyn OAuthAuth> + Send + Sync>;
type RadiusFlowLoader = Arc<dyn Fn(RadiusOAuthOptions) -> Arc<dyn OAuthAuth> + Send + Sync>;

/// `OAuthFlowLoaders`: statically bundled flow constructors.
#[derive(Clone)]
pub struct OAuthFlowLoaders {
    pub anthropic: FlowLoader,
    pub openai_codex: FlowLoader,
    pub github_copilot: FlowLoader,
    pub openrouter: FlowLoader,
    pub kimi_coding: FlowLoader,
    pub xai: FlowLoader,
    pub radius: RadiusFlowLoader,
}

static BUNDLED_LOADERS: OnceLock<RwLock<Option<OAuthFlowLoaders>>> = OnceLock::new();

fn bundled_loaders() -> &'static RwLock<Option<OAuthFlowLoaders>> {
    BUNDLED_LOADERS.get_or_init(|| RwLock::new(None))
}

pub fn register_bundled_oauth_flow_loaders(loaders: OAuthFlowLoaders) {
    *bundled_loaders().write().expect("OAuth loader registry poisoned") = Some(loaders);
}

fn registered() -> Option<OAuthFlowLoaders> {
    bundled_loaders()
        .read()
        .expect("OAuth loader registry poisoned")
        .clone()
}

pub fn load_anthropic_oauth() -> Arc<dyn OAuthAuth> {
    registered()
        .map(|loaders| (loaders.anthropic)())
        .unwrap_or_else(|| Arc::new(AnthropicOAuth::new()))
}

pub fn load_openai_codex_oauth() -> Arc<dyn OAuthAuth> {
    registered()
        .map(|loaders| (loaders.openai_codex)())
        .unwrap_or_else(|| Arc::new(OpenAICodexOAuth::new()))
}

pub fn load_github_copilot_oauth() -> Arc<dyn OAuthAuth> {
    match registered() {
        Some(loaders) => (loaders.github_copilot)(),
        None => Arc::new(GitHubCopilotOAuth::new()),
    }
}

pub fn load_openrouter_oauth() -> Arc<dyn OAuthAuth> {
    registered()
        .map(|loaders| (loaders.openrouter)())
        .unwrap_or_else(|| Arc::new(OpenRouterOAuth::new()))
}

pub fn load_kimi_coding_oauth() -> Arc<dyn OAuthAuth> {
    registered()
        .map(|loaders| (loaders.kimi_coding)())
        .unwrap_or_else(|| Arc::new(KimiCodingOAuth::new()))
}

pub fn load_xai_oauth() -> Arc<dyn OAuthAuth> {
    registered()
        .map(|loaders| (loaders.xai)())
        .unwrap_or_else(|| Arc::new(XaiOAuth::new()))
}

pub fn load_radius_oauth(options: RadiusOAuthOptions) -> Arc<dyn OAuthAuth> {
    match registered() {
        Some(loaders) => (loaders.radius)(options),
        None => Arc::new(create_radius_oauth(options)),
    }
}
