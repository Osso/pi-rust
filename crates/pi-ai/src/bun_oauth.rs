//! Port of `packages/ai/src/bun-oauth.ts`: registers the statically bundled flows.

use std::sync::Arc;

use crate::auth::oauth::anthropic::AnthropicOAuth;
use crate::auth::oauth::github_copilot::GitHubCopilotOAuth;
use crate::auth::oauth::kimi_coding::KimiCodingOAuth;
use crate::auth::oauth::load::{OAuthFlowLoaders, register_bundled_oauth_flow_loaders};
use crate::auth::oauth::openai_codex::OpenAICodexOAuth;
use crate::auth::oauth::openrouter::OpenRouterOAuth;
use crate::auth::oauth::radius::create_radius_oauth;
use crate::auth::oauth::xai::XaiOAuth;

/// Registers OAuth flows statically embedded in the standalone binary.
pub fn register_bun_oauth_flows() {
    register_bundled_oauth_flow_loaders(OAuthFlowLoaders {
        anthropic: Arc::new(|| Arc::new(AnthropicOAuth::new())),
        openai_codex: Arc::new(|| Arc::new(OpenAICodexOAuth::new())),
        github_copilot: Arc::new(|| Arc::new(GitHubCopilotOAuth::new())),
        openrouter: Arc::new(|| Arc::new(OpenRouterOAuth::new())),
        kimi_coding: Arc::new(|| Arc::new(KimiCodingOAuth::new())),
        xai: Arc::new(|| Arc::new(XaiOAuth::new())),
        radius: Arc::new(|options| Arc::new(create_radius_oauth(options))),
    });
}
