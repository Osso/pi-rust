//! Native loaders for frozen `packages/ai/src/utils/oauth/load.ts`.
//! Rust statically links the three frozen flow modules; no override registry.

use super::{anthropic::AnthropicOAuth, github_copilot::GitHubCopilotOAuth, openai_codex::OpenAICodexOAuth};
use crate::auth::OAuthAuth;
use std::sync::Arc;

pub fn load_anthropic_oauth() -> Arc<dyn OAuthAuth> {
    Arc::new(AnthropicOAuth::new())
}

pub fn load_openai_codex_oauth() -> Arc<dyn OAuthAuth> {
    Arc::new(OpenAICodexOAuth::new())
}

pub fn load_github_copilot_oauth() -> Arc<dyn OAuthAuth> {
    Arc::new(GitHubCopilotOAuth::new())
}
