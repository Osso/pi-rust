//! Port of `packages/ai/src/env-api-keys.ts`.

use std::sync::{Arc, LazyLock, Mutex};

use crate::types::ProviderEnv;
use crate::utils::provider_env::resolve_provider_env_value;

pub const ANTHROPIC_AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
pub const ANTHROPIC_OAUTH_TOKEN_ENV: &str = "ANTHROPIC_OAUTH_TOKEN";
pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

const GOOGLE_APPLICATION_CREDENTIALS: &str = "GOOGLE_APPLICATION_CREDENTIALS";
const VERTEX_ADC_DEFAULT_PATH: &str = ".config/gcloud/application_default_credentials.json";

/// Application Default Credentials probe with the TypeScript module's cache.
///
/// `fileExists` and `home` are injectable so tests never touch process-global
/// state. An explicit scoped `GOOGLE_APPLICATION_CREDENTIALS` value bypasses the
/// cache, matching the source.
pub struct VertexAdc {
    file_exists: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    home: Option<String>,
    cached: Mutex<Option<bool>>,
}

impl VertexAdc {
    pub fn with_sources(file_exists: impl Fn(&str) -> bool + Send + Sync + 'static, home: Option<&str>) -> Self {
        Self {
            file_exists: Arc::new(file_exists),
            home: home.map(str::to_owned),
            cached: Mutex::new(None),
        }
    }

    pub fn default_sources() -> Self {
        Self::with_sources(
            |path| std::fs::metadata(path).is_ok(),
            std::env::var("HOME").ok().as_deref(),
        )
    }

    /// `hasVertexAdcCredentials(env)`.
    pub fn has_credentials(&self, env: Option<&ProviderEnv>, process_env: impl Fn(&str) -> Option<String>) -> bool {
        if let Some(path) = env
            .and_then(|env| env.get(GOOGLE_APPLICATION_CREDENTIALS))
            .filter(|path| !path.is_empty())
        {
            return (self.file_exists)(path);
        }

        let mut cached = self.cached.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = *cached {
            return value;
        }

        let value = match process_env(GOOGLE_APPLICATION_CREDENTIALS).filter(|path| !path.is_empty()) {
            Some(path) => (self.file_exists)(&path),
            None => self
                .home
                .as_ref()
                .is_some_and(|home| (self.file_exists)(&format!("{home}/{VERTEX_ADC_DEFAULT_PATH}"))),
        };
        *cached = Some(value);
        value
    }
}

static DEFAULT_VERTEX_ADC: LazyLock<VertexAdc> = LazyLock::new(VertexAdc::default_sources);

fn api_key_env_vars(provider: &str) -> Option<Vec<&'static str>> {
    if provider == "github-copilot" {
        return Some(vec!["COPILOT_GITHUB_TOKEN"]);
    }
    if provider == "anthropic" {
        return Some(vec![
            ANTHROPIC_AUTH_TOKEN_ENV,
            ANTHROPIC_OAUTH_TOKEN_ENV,
            ANTHROPIC_API_KEY_ENV,
        ]);
    }

    let env_var = match provider {
        "ant-ling" => "ANT_LING_API_KEY",
        "qwen-token-plan" | "qwen-token-plan-individual" => "QWEN_TOKEN_PLAN_API_KEY",
        "qwen-token-plan-cn" => "QWEN_TOKEN_PLAN_CN_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "nvidia" => "NVIDIA_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "google" => "GEMINI_API_KEY",
        "google-vertex" => "GOOGLE_CLOUD_API_KEY",
        "groq" => "GROQ_API_KEY",
        "cerebras" => "CEREBRAS_API_KEY",
        "xai" => "XAI_API_KEY",
        "radius" => "RADIUS_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "vercel-ai-gateway" => "AI_GATEWAY_API_KEY",
        "zai" => "ZAI_API_KEY",
        "zai-coding-cn" => "ZAI_CODING_CN_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "minimax" => "MINIMAX_API_KEY",
        "minimax-cn" => "MINIMAX_CN_API_KEY",
        "moonshotai" | "moonshotai-cn" => "MOONSHOT_API_KEY",
        "huggingface" => "HF_TOKEN",
        "fireworks" => "FIREWORKS_API_KEY",
        "together" => "TOGETHER_API_KEY",
        "baseten" => "BASETEN_API_KEY",
        "opencode" | "opencode-go" => "OPENCODE_API_KEY",
        "kimi-coding" => "KIMI_API_KEY",
        "cloudflare-workers-ai" | "cloudflare-ai-gateway" => "CLOUDFLARE_API_KEY",
        "xiaomi" => "XIAOMI_API_KEY",
        "xiaomi-token-plan-cn" => "XIAOMI_TOKEN_PLAN_CN_API_KEY",
        "xiaomi-token-plan-ams" => "XIAOMI_TOKEN_PLAN_AMS_API_KEY",
        "xiaomi-token-plan-sgp" => "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
        _ => return None,
    };
    Some(vec![env_var])
}

/// Find configured API-key environment variables for `provider`.
pub fn find_env_keys(provider: &str, env: Option<&ProviderEnv>) -> Option<Vec<String>> {
    find_env_keys_with(provider, env, |name| std::env::var(name).ok())
}

/// Testable [`find_env_keys`] with an injected process environment.
pub fn find_env_keys_with(
    provider: &str,
    env: Option<&ProviderEnv>,
    process_env: impl Fn(&str) -> Option<String>,
) -> Option<Vec<String>> {
    let env_vars = api_key_env_vars(provider)?;
    let found: Vec<String> = env_vars
        .iter()
        .filter(|name| resolve_provider_env_value(name, env, &process_env).is_some())
        .map(|name| (*name).to_owned())
        .collect();
    (!found.is_empty()).then_some(found)
}

/// Get an API key for `provider` from known environment variables.
pub fn get_env_api_key(provider: &str, env: Option<&ProviderEnv>) -> Option<String> {
    get_env_api_key_with(provider, env, |name| std::env::var(name).ok(), &DEFAULT_VERTEX_ADC)
}

/// Testable [`get_env_api_key`] with injected process environment, file
/// existence, and ADC cache.
pub fn get_env_api_key_with(
    provider: &str,
    env: Option<&ProviderEnv>,
    process_env: impl Fn(&str) -> Option<String>,
    vertex_adc: &VertexAdc,
) -> Option<String> {
    if let Some(env_keys) = find_env_keys_with(provider, env, &process_env) {
        let api_key_env = if provider == "anthropic" {
            env_keys.iter().find(|key| *key != ANTHROPIC_AUTH_TOKEN_ENV)
        } else {
            env_keys.first()
        };
        if let Some(name) = api_key_env
            && let Some(value) = resolve_provider_env_value(name, env, &process_env)
        {
            return Some(value);
        }
    }

    if provider == "google-vertex" && has_vertex_credentials(env, &process_env, vertex_adc) {
        return Some("<authenticated>".to_owned());
    }

    if provider == "amazon-bedrock" && has_bedrock_credentials(env, &process_env) {
        return Some("<authenticated>".to_owned());
    }

    None
}

fn has_vertex_credentials(
    env: Option<&ProviderEnv>,
    process_env: impl Fn(&str) -> Option<String>,
    vertex_adc: &VertexAdc,
) -> bool {
    if !vertex_adc.has_credentials(env, &process_env) {
        return false;
    }
    let has_project = resolve_provider_env_value("GOOGLE_CLOUD_PROJECT", env, &process_env).is_some()
        || resolve_provider_env_value("GCLOUD_PROJECT", env, &process_env).is_some();
    let has_location = resolve_provider_env_value("GOOGLE_CLOUD_LOCATION", env, &process_env).is_some();
    has_project && has_location
}

fn has_bedrock_credentials(env: Option<&ProviderEnv>, process_env: impl Fn(&str) -> Option<String>) -> bool {
    let present = |name: &str| resolve_provider_env_value(name, env, &process_env).is_some();
    present("AWS_PROFILE")
        || (present("AWS_ACCESS_KEY_ID") && present("AWS_SECRET_ACCESS_KEY"))
        || present("AWS_BEARER_TOKEN_BEDROCK")
        || present("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        || present("AWS_CONTAINER_CREDENTIALS_FULL_URI")
        || present("AWS_WEB_IDENTITY_TOKEN_FILE")
}
