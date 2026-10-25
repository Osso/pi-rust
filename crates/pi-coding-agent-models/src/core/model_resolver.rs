//! Port of `packages/coding-agent/src/core/model-resolver.ts`.
//!
//! Model reference resolution: exact/canonical/fuzzy matching, `:thinking`
//! suffix parsing, scope globs, CLI flag resolution, and initial/restore
//! selection. Console warnings go to stderr; `findInitialModel` returns the
//! CLI error instead of calling `process.exit`.

use indexmap::IndexMap;
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::models::models_are_equal;
use pi_ai::types::Model;
use pi_js_compat::minimatch::{MinimatchOptions, minimatch};
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

use crate::cli::args::is_valid_thinking_level;
use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::model_registry::ModelRegistry;
use crate::utils::chalk::Chalk;

/// Default model IDs for each known provider.
pub const DEFAULT_MODEL_PER_PROVIDER: &[(&str, &str)] = &[
    ("amazon-bedrock", "us.anthropic.claude-opus-4-6-v1"),
    ("ant-ling", "Ring-2.6-1T"),
    ("openai-codex", "gpt-5.5"),
    ("openai-codex-gc", "gpt-5.5"),
    ("openai-codex-team", "gpt-5.5"),
    ("anthropic", "claude-opus-4-8"),
    ("openai", "gpt-5.5"),
    ("nvidia", "nvidia/nemotron-3-super-120b-a12b"),
    ("deepseek", "deepseek-v4-pro"),
    ("google", "gemini-3.1-pro-preview"),
    ("google-vertex", "gemini-3.1-pro-preview"),
    ("github-copilot", "gpt-5.4"),
    ("openrouter", "moonshotai/kimi-k2.6"),
    ("vercel-ai-gateway", "zai/glm-5.1"),
    ("xai", "grok-4.20-0309-reasoning"),
    ("groq", "openai/gpt-oss-120b"),
    ("cerebras", "zai-glm-4.7"),
    ("zai", "glm-5.1"),
    ("zai-coding-cn", "glm-5.1"),
    ("mistral", "devstral-medium-latest"),
    ("minimax", "MiniMax-M2.7"),
    ("minimax-cn", "MiniMax-M2.7"),
    ("moonshotai", "kimi-k2.6"),
    ("moonshotai-cn", "kimi-k2.6"),
    ("huggingface", "moonshotai/Kimi-K2.6"),
    ("fireworks", "accounts/fireworks/models/kimi-k2p6"),
    ("together", "moonshotai/Kimi-K2.6"),
    ("opencode", "kimi-k2.6"),
    ("opencode-go", "kimi-k2.6"),
    ("kimi-coding", "kimi-for-coding"),
    ("cloudflare-workers-ai", "@cf/moonshotai/kimi-k2.6"),
    ("cloudflare-ai-gateway", "workers-ai/@cf/moonshotai/kimi-k2.6"),
    ("xiaomi", "mimo-v2.5-pro"),
    ("xiaomi-token-plan-cn", "mimo-v2.5-pro"),
    ("xiaomi-token-plan-ams", "mimo-v2.5-pro"),
    ("xiaomi-token-plan-sgp", "mimo-v2.5-pro"),
];

/// `defaultModelPerProvider[provider]`.
pub fn default_model_for_provider(provider: &str) -> Option<&'static str> {
    DEFAULT_MODEL_PER_PROVIDER
        .iter()
        .find(|(id, _)| *id == provider)
        .map(|(_, model)| *model)
}

/// One scoped model plus its optional thinking level.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedModel {
    pub model: Model,
    /// Set when the pattern carried an explicit `:level` suffix.
    pub thinking_level: Option<ThinkingLevel>,
}

/// Result of [`parse_model_pattern`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedModelResult {
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub warning: Option<JsString>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ParseModelPatternOptions {
    /// Defaults to true; CLI `--model` parsing sets it false.
    pub allow_invalid_thinking_level_fallback: Option<bool>,
}

/// A structured scope warning (`ModelScopeDiagnostic`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelScopeDiagnostic {
    pub diagnostic_type: &'static str,
    pub message: JsString,
    pub pattern: JsString,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolveModelScopeResult {
    pub scoped_models: Vec<ScopedModel>,
    pub diagnostics: Vec<ModelScopeDiagnostic>,
}

/// Borrowed structural surface used by the frozen resolver; no defaults or state.
/// ModelRegistry is the only production implementation; tests provide exact input lists.
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

pub trait ModelResolverRegistry: Send + Sync {
    fn get_all(&self) -> Vec<Model>;
    fn get_available(&self) -> Result<Vec<Model>, NativeNodeError>;
    fn find(&self, provider: &str, model_id: &str) -> Option<Model>;
    fn has_configured_auth(&self, model: &Model) -> Result<bool, NativeNodeError>;
}
impl ModelResolverRegistry for ModelRegistry {
    fn get_all(&self) -> Vec<Model> {
        ModelRegistry::get_all(self)
    }
    fn get_available(&self) -> Result<Vec<Model>, NativeNodeError> {
        ModelRegistry::get_available(self)
    }
    fn find(&self, provider: &str, model_id: &str) -> Option<Model> {
        ModelRegistry::find(self, provider, model_id)
    }
    fn has_configured_auth(&self, model: &Model) -> Result<bool, NativeNodeError> {
        ModelRegistry::has_configured_auth(self, model)
    }
}

/// `isAlias`: no `-YYYYMMDD` suffix, or an explicit `-latest` alias.
fn is_alias(id: &str) -> bool {
    if id.ends_with("-latest") {
        return true;
    }
    !has_date_suffix(id)
}

fn has_date_suffix(id: &str) -> bool {
    let Some((_, suffix)) = id.rsplit_once('-') else {
        return false;
    };
    suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_thinking_level(value: &JsString) -> Option<ThinkingLevel> {
    let scalar = match value.to_utf8() {
        Ok(scalar) => scalar,
        Err(_) => return None,
    };
    let value = scalar.as_str();
    if !is_valid_thinking_level(value) {
        return None;
    }
    Some(match value {
        "off" => ThinkingLevel::Off,
        "minimal" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        "xhigh" => ThinkingLevel::XHigh,
        "max" => ThinkingLevel::Max,
        "ultra" => ThinkingLevel::Ultra,
        _ => return None,
    })
}

/// Finds an exact model reference: canonical `provider/model`, explicit
/// `provider/model` split, then a bare model id. Ambiguous matches are rejected.
pub fn find_exact_model_reference_match(model_reference: &JsString, available_models: &[Model]) -> Option<Model> {
    let trimmed = model_reference.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_lowercase();

    let canonical: Vec<&Model> = available_models
        .iter()
        .filter(|model| js_format!("{}/{}", model.provider, model.id).to_lowercase() == normalized)
        .collect();
    match canonical.len() {
        1 => return Some(canonical[0].clone()),
        0 => {}
        _ => return None,
    }

    if let Some(slash_index) = trimmed.find_from('/', 0) {
        let provider = trimmed.slice(0, slash_index).trim();
        let model_id = trimmed.slice(slash_index + 1, trimmed.len()).trim();
        if !provider.is_empty() && !model_id.is_empty() {
            let provider_matches: Vec<&Model> = available_models
                .iter()
                .filter(|model| {
                    JsString::from(&model.provider).to_lowercase() == provider.to_lowercase()
                        && JsString::from(&model.id).to_lowercase() == model_id.to_lowercase()
                })
                .collect();
            match provider_matches.len() {
                1 => return Some(provider_matches[0].clone()),
                0 => {}
                _ => return None,
            }
        }
    }

    let id_matches: Vec<&Model> = available_models
        .iter()
        .filter(|model| JsString::from(&model.id).to_lowercase() == normalized)
        .collect();
    (id_matches.len() == 1).then(|| id_matches[0].clone())
}

/// Exact match first, then partial id/name matching with alias preference.
fn try_match_model(model_pattern: &JsString, available_models: &[Model]) -> Option<Model> {
    if let Some(exact) = find_exact_model_reference_match(model_pattern, available_models) {
        return Some(exact);
    }
    let needle = model_pattern.to_lowercase();
    let matches: Vec<&Model> = available_models
        .iter()
        .filter(|model| {
            JsString::from(&model.id).to_lowercase().contains(&needle)
                || JsString::from(&model.name).to_lowercase().contains(&needle)
        })
        .collect();
    if matches.is_empty() {
        return None;
    }
    let aliases: Vec<&Model> = matches.iter().copied().filter(|model| is_alias(&model.id)).collect();
    let mut candidates = if aliases.is_empty() {
        matches.into_iter().filter(|model| !is_alias(&model.id)).collect()
    } else {
        aliases
    };
    candidates.sort_by(|left, right| pi_js_compat::locale_compare(&right.id, &left.id));
    candidates.first().map(|model| (*model).clone())
}

fn build_fallback_model(provider: &str, model_id: &str, available_models: &[Model]) -> Option<Model> {
    let provider_models: Vec<&Model> = available_models
        .iter()
        .filter(|model| model.provider == provider)
        .collect();
    if provider_models.is_empty() {
        return None;
    }
    let base = default_model_for_provider(provider)
        .and_then(|default_id| provider_models.iter().find(|model| model.id == default_id).copied())
        .unwrap_or(provider_models[0]);
    let mut fallback = base.clone();
    fallback.id = model_id.to_owned();
    fallback.name = model_id.into();
    Some(fallback)
}

/// Parses `pattern[:thinking]`, tolerating colons inside model ids.
pub fn parse_model_pattern(
    pattern: &JsString,
    available_models: &[Model],
    options: ParseModelPatternOptions,
) -> ParsedModelResult {
    if let Some(model) = try_match_model(pattern, available_models) {
        return ParsedModelResult {
            model: Some(model),
            thinking_level: None,
            warning: None,
        };
    }
    let Some(last_colon_index) = pattern.units().iter().rposition(|unit| *unit == b':' as u16) else {
        return ParsedModelResult {
            model: None,
            thinking_level: None,
            warning: None,
        };
    };
    let prefix = pattern.slice(0, last_colon_index);
    let suffix = pattern.slice(last_colon_index + 1, pattern.len());

    if let Some(level) = parse_thinking_level(&suffix) {
        let result = parse_model_pattern(&prefix, available_models, options);
        if result.model.is_some() {
            return ParsedModelResult {
                model: result.model,
                thinking_level: if result.warning.is_some() { None } else { Some(level) },
                warning: result.warning,
            };
        }
        return result;
    }

    if !options.allow_invalid_thinking_level_fallback.unwrap_or(true) {
        return ParsedModelResult {
            model: None,
            thinking_level: None,
            warning: None,
        };
    }
    let result = parse_model_pattern(&prefix, available_models, options);
    if result.model.is_some() {
        return ParsedModelResult {
            model: result.model,
            thinking_level: None,
            warning: Some(js_format!(
                "Invalid thinking level \"{}\" in pattern \"{}\". Using default instead.",
                suffix,
                pattern
            )),
        };
    }
    result
}

fn contains_glob_characters(pattern: &JsString) -> bool {
    ['*', '?', '['].iter().any(|unit| pattern.contains(*unit))
}

fn glob_matches(path: impl Into<JsString>, pattern: &JsString) -> bool {
    minimatch(
        path,
        pattern,
        &MinimatchOptions {
            nocase: true,
            ..Default::default()
        },
    )
}

/// Resolves scope patterns against a fixed model list.
pub fn resolve_model_scope_from_models(patterns: &[JsString], models: &[Model]) -> ResolveModelScopeResult {
    let mut scoped_models: Vec<ScopedModel> = Vec::new();
    let mut diagnostics: Vec<ModelScopeDiagnostic> = Vec::new();

    for pattern in patterns {
        if contains_glob_characters(pattern) {
            let (glob_pattern, thinking_level) = split_glob_thinking_suffix(pattern);
            let matching: Vec<&Model> = models
                .iter()
                .filter(|model| {
                    glob_matches(js_format!("{}/{}", model.provider, model.id), &glob_pattern)
                        || glob_matches(&model.id, &glob_pattern)
                })
                .collect();
            if matching.is_empty() {
                diagnostics.push(no_match_diagnostic(pattern));
                continue;
            }
            for model in matching {
                push_unique(&mut scoped_models, model.clone(), thinking_level);
            }
            continue;
        }

        let parsed = parse_model_pattern(pattern, models, ParseModelPatternOptions::default());
        if let Some(warning) = parsed.warning {
            diagnostics.push(ModelScopeDiagnostic {
                diagnostic_type: "warning",
                message: warning,
                pattern: pattern.clone(),
            });
        }
        match parsed.model {
            Some(model) => push_unique(&mut scoped_models, model, parsed.thinking_level),
            None => diagnostics.push(no_match_diagnostic(pattern)),
        }
    }

    ResolveModelScopeResult {
        scoped_models,
        diagnostics,
    }
}

fn split_glob_thinking_suffix(pattern: &JsString) -> (JsString, Option<ThinkingLevel>) {
    let Some(colon_index) = pattern.units().iter().rposition(|unit| *unit == b':' as u16) else {
        return (pattern.clone(), None);
    };
    let suffix = pattern.slice(colon_index + 1, pattern.len());
    match parse_thinking_level(&suffix) {
        Some(level) => (pattern.slice(0, colon_index), Some(level)),
        None => (pattern.clone(), None),
    }
}

fn no_match_diagnostic(pattern: &JsString) -> ModelScopeDiagnostic {
    ModelScopeDiagnostic {
        diagnostic_type: "warning",
        message: js_format!("No models match pattern \"{}\"", pattern),
        pattern: pattern.to_owned(),
    }
}

fn push_unique(scoped_models: &mut Vec<ScopedModel>, model: Model, thinking_level: Option<ThinkingLevel>) {
    if scoped_models
        .iter()
        .any(|scoped| models_are_equal(Some(&scoped.model), Some(&model)))
    {
        return;
    }
    scoped_models.push(ScopedModel { model, thinking_level });
}

/// Resolves scope patterns against the registry's available models.
pub async fn resolve_model_scope_with_diagnostics(
    patterns: &[JsString],
    registry: &dyn ModelResolverRegistry,
) -> Result<ResolveModelScopeResult, NativeNodeError> {
    let models = registry.get_available()?;
    Ok(resolve_model_scope_from_models(patterns, &models))
}

/// Resolves scope patterns and prints each diagnostic like the CLI.
pub async fn resolve_model_scope(
    patterns: &[JsString],
    registry: &dyn ModelResolverRegistry,
) -> Result<Vec<ScopedModel>, NativeNodeError> {
    let result = resolve_model_scope_with_diagnostics(patterns, registry).await?;
    for diagnostic in &result.diagnostics {
        let message = Chalk::stdout().yellow(js_format!("Warning: {}", diagnostic.message));
        eprintln!("{}", encode_node_utf8(&message));
    }
    Ok(result.scoped_models)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolveCliModelResult {
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub warning: Option<JsString>,
    /// Set when `model` is `None` because resolution failed.
    pub error: Option<JsString>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolveCliModelOptions {
    pub cli_provider: Option<String>,
    pub cli_model: Option<JsString>,
    pub cli_thinking: Option<ThinkingLevel>,
}

/// Resolves a single model from CLI flags (`--provider`, `--model`, `--thinking`).
pub fn resolve_cli_model(
    options: &ResolveCliModelOptions,
    registry: &dyn ModelResolverRegistry,
) -> Result<ResolveCliModelResult, NativeNodeError> {
    let Some(cli_model) = options.cli_model.as_ref().filter(|model| !model.is_empty()) else {
        return Ok(ResolveCliModelResult::default());
    };
    let available_models = registry.get_all();
    if available_models.is_empty() {
        return Ok(ResolveCliModelResult {
            error: Some("No models available. Check your installation or add models to models.json.".into()),
            ..Default::default()
        });
    }

    let mut provider_map = IndexMap::new();
    for model in &available_models {
        provider_map.insert(JsString::from(&model.provider).to_lowercase(), model.provider.clone());
    }
    let cli_provider = options.cli_provider.as_deref().filter(|provider| !provider.is_empty());
    let mut provider = cli_provider.and_then(|requested| find_provider(&provider_map, requested));
    if let Some(requested) = cli_provider.filter(|_| provider.is_none()) {
        return Ok(ResolveCliModelResult {
            error: Some(
                format!("Unknown provider \"{requested}\". Use --list-models to see available providers/models.")
                    .into(),
            ),
            ..Default::default()
        });
    }

    let mut pattern = cli_model.to_owned();
    let mut inferred_provider = false;
    if provider.is_none()
        && let Some(slash_index) = cli_model.find('/')
    {
        let maybe_provider = &cli_model.slice(0, slash_index);
        if let Some(canonical) = find_provider(&provider_map, maybe_provider) {
            provider = Some(canonical);
            pattern = cli_model.slice(slash_index + 1, cli_model.len());
            inferred_provider = true;
        }
    }

    if provider.is_none() {
        let lower = cli_model.to_lowercase();
        if let Some(exact) = available_models.iter().find(|model| {
            JsString::from(&model.id).to_lowercase() == lower
                || js_format!("{}/{}", model.provider, model.id).to_lowercase() == lower
        }) {
            return Ok(ResolveCliModelResult {
                model: Some(exact.clone()),
                ..Default::default()
            });
        }
    }

    if cli_provider.is_some()
        && let Some(provider) = &provider
    {
        let prefix = js_format!("{}/", provider);
        if cli_model.to_lowercase().starts_with(prefix.to_lowercase()) {
            pattern = cli_model.slice(prefix.len(), cli_model.len());
        }
    }

    let candidates: Vec<Model> = match &provider {
        Some(provider) => available_models
            .iter()
            .filter(|model| &model.provider == provider)
            .cloned()
            .collect(),
        None => available_models.clone(),
    };
    let parsed = parse_model_pattern(
        &pattern,
        &candidates,
        ParseModelPatternOptions {
            allow_invalid_thinking_level_fallback: Some(false),
        },
    );
    let warning = parsed.warning.clone();
    if let Some(model) = parsed.model {
        if inferred_provider {
            let raw_exact_matches: Vec<&Model> = available_models
                .iter()
                .filter(|candidate| {
                    JsString::from(&candidate.id).to_lowercase() == cli_model.to_lowercase()
                        && !models_are_equal(Some(candidate), Some(&model))
                })
                .collect();
            if !raw_exact_matches.is_empty() && !registry.has_configured_auth(&model)? {
                let mut authenticated = Vec::new();
                for candidate in &raw_exact_matches {
                    if registry.has_configured_auth(candidate)? {
                        authenticated.push(candidate);
                    }
                }
                if authenticated.len() == 1 {
                    return Ok(ResolveCliModelResult {
                        model: Some((**authenticated[0]).clone()),
                        ..Default::default()
                    });
                }
            }
        }
        return Ok(ResolveCliModelResult {
            model: Some(model),
            thinking_level: parsed.thinking_level,
            warning,
            error: None,
        });
    }

    if inferred_provider {
        let lower = cli_model.to_lowercase();
        if let Some(exact) = available_models.iter().find(|candidate| {
            JsString::from(&candidate.id).to_lowercase() == lower
                || js_format!("{}/{}", candidate.provider, candidate.id).to_lowercase() == lower
        }) {
            return Ok(ResolveCliModelResult {
                model: Some(exact.clone()),
                ..Default::default()
            });
        }
        let fallback = parse_model_pattern(
            cli_model,
            &available_models,
            ParseModelPatternOptions {
                allow_invalid_thinking_level_fallback: Some(false),
            },
        );
        if let Some(model) = fallback.model {
            return Ok(ResolveCliModelResult {
                model: Some(model),
                thinking_level: fallback.thinking_level,
                warning: fallback.warning,
                error: None,
            });
        }
    }

    if let Some(provider) = &provider {
        let (fallback_pattern, fallback_thinking) = split_fallback_thinking(&pattern, options.cli_thinking.is_none());
        if let Some(fallback_model) = build_fallback_model(
            provider,
            &match fallback_pattern.to_utf8() {
                Ok(id) => id,
                Err(_) => {
                    return Ok(ResolveCliModelResult {
                        error: Some("Model ID contains unpaired UTF-16 units".into()),
                        ..Default::default()
                    });
                }
            },
            &available_models,
        ) {
            let requested_thinking = options.cli_thinking.or(fallback_thinking);
            let mut model = fallback_model;
            if requested_thinking.is_some_and(|level| level != ThinkingLevel::Off) {
                model.reasoning = true;
            }
            let fallback_warning = match &warning {
                Some(warning) => js_format!(
                    "{} Model \"{}\" not found for provider \"{}\". Using custom model id.",
                    warning,
                    fallback_pattern,
                    provider
                ),
                None => js_format!(
                    "Model \"{}\" not found for provider \"{}\". Using custom model id.",
                    fallback_pattern,
                    provider
                ),
            };
            return Ok(ResolveCliModelResult {
                model: Some(model),
                thinking_level: fallback_thinking,
                warning: Some(fallback_warning),
                error: None,
            });
        }
    }

    let display = match &provider {
        Some(provider) => js_format!("{}/{}", provider, pattern),
        None => cli_model.to_owned(),
    };
    Ok(ResolveCliModelResult {
        model: None,
        thinking_level: None,
        warning,
        error: Some(js_format!(
            "Model \"{}\" not found. Use --list-models to see available models.",
            display
        )),
    })
}

fn find_provider(provider_map: &IndexMap<JsString, String>, requested: impl Into<JsString>) -> Option<String> {
    provider_map.get(&requested.into().to_lowercase()).cloned()
}

fn split_fallback_thinking(pattern: &JsString, allow_suffix: bool) -> (JsString, Option<ThinkingLevel>) {
    if !allow_suffix {
        return (pattern.clone(), None);
    }
    let Some(last_colon) = pattern.units().iter().rposition(|unit| *unit == b':' as u16) else {
        return (pattern.clone(), None);
    };
    let suffix = pattern.slice(last_colon + 1, pattern.len());
    match parse_thinking_level(&suffix) {
        Some(level) => (pattern.slice(0, last_colon), Some(level)),
        None => (pattern.clone(), None),
    }
}

#[derive(Debug, Clone, Default)]
pub struct FindInitialModelOptions {
    pub cli_provider: Option<String>,
    pub cli_model: Option<JsString>,
    pub scoped_models: Vec<ScopedModel>,
    pub is_continuing: bool,
    pub default_provider: Option<String>,
    pub default_model_id: Option<String>,
    pub default_thinking_level: Option<ThinkingLevel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitialModelResult {
    pub model: Option<Model>,
    pub thinking_level: ThinkingLevel,
    pub fallback_message: Option<JsString>,
}

/// Picks the initial model: CLI flags, scoped models, saved default, first available.
///
/// The TypeScript source calls `process.exit(1)` on a CLI resolution error;
/// Rust returns that message as `Err` so the CLI owns process termination.
pub async fn find_initial_model(
    options: &FindInitialModelOptions,
    registry: &dyn ModelResolverRegistry,
) -> Result<InitialModelResult, NativeNodeError> {
    if let (Some(cli_provider), Some(cli_model)) = (
        options.cli_provider.as_ref().filter(|provider| !provider.is_empty()),
        options.cli_model.as_ref().filter(|model| !model.is_empty()),
    ) {
        let resolved = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_provider: Some(cli_provider.clone()),
                cli_model: Some(cli_model.clone()),
                cli_thinking: None,
            },
            registry,
        )?;
        if let Some(error) = resolved.error {
            return Err(NativeNodeError::from_constructor(NativeErrorClass::Error, error));
        }
        if let Some(model) = resolved.model {
            return Ok(InitialModelResult {
                model: Some(model),
                thinking_level: DEFAULT_THINKING_LEVEL,
                fallback_message: None,
            });
        }
    }

    if let Some(first) = options.scoped_models.first()
        && !options.is_continuing
    {
        return Ok(InitialModelResult {
            model: Some(first.model.clone()),
            thinking_level: first
                .thinking_level
                .or(options.default_thinking_level)
                .unwrap_or(DEFAULT_THINKING_LEVEL),
            fallback_message: None,
        });
    }

    if let (Some(provider), Some(model_id)) = (&options.default_provider, &options.default_model_id)
        && let Some(found) = registry.find(provider, model_id)
        && registry.has_configured_auth(&found)?
    {
        return Ok(InitialModelResult {
            model: Some(found),
            thinking_level: options.default_thinking_level.unwrap_or(DEFAULT_THINKING_LEVEL),
            fallback_message: None,
        });
    }

    let available = registry.get_available()?;
    if let Some(model) = preferred_available_model(&available) {
        return Ok(InitialModelResult {
            model: Some(model),
            thinking_level: DEFAULT_THINKING_LEVEL,
            fallback_message: None,
        });
    }

    Ok(InitialModelResult {
        model: None,
        thinking_level: DEFAULT_THINKING_LEVEL,
        fallback_message: None,
    })
}

fn preferred_available_model(available: &[Model]) -> Option<Model> {
    for (provider, default_id) in DEFAULT_MODEL_PER_PROVIDER {
        if let Some(found) = available
            .iter()
            .find(|model| model.provider == *provider && model.id == *default_id)
        {
            return Some(found.clone());
        }
    }
    available.first().cloned()
}

fn print_dimmed_model(prefix: &str, provider: &str, id: &str) {
    let message = Chalk::stdout().dim(format!("{prefix}: {provider}/{id}"));
    println!("{}", encode_node_utf8(&message));
}

/// Restores a model from session state, falling back to an available model.
pub async fn restore_model_from_session(
    saved_provider: &str,
    saved_model_id: &str,
    current_model: Option<&Model>,
    should_print_messages: bool,
    registry: &dyn ModelResolverRegistry,
) -> Result<(Option<Model>, Option<JsString>), NativeNodeError> {
    let restored = registry.find(saved_provider, saved_model_id);
    let has_configured_auth = match restored.as_ref() {
        Some(model) => registry.has_configured_auth(model)?,
        None => false,
    };
    if let Some(model) = &restored
        && has_configured_auth
    {
        if should_print_messages {
            print_dimmed_model("Restored model", saved_provider, saved_model_id);
        }
        return Ok((Some(model.clone()), None));
    }

    let reason = if restored.is_none() {
        "model no longer exists"
    } else {
        "no auth configured"
    };
    if should_print_messages {
        let message = Chalk::stdout().yellow(format!(
            "Warning: Could not restore model {saved_provider}/{saved_model_id} ({reason})."
        ));
        eprintln!("{}", encode_node_utf8(&message));
    }
    if let Some(current) = current_model {
        if should_print_messages {
            print_dimmed_model("Falling back to", &current.provider, &current.id);
        }
        return Ok((
            Some(current.clone()),
            Some(js_format!(
                "Could not restore model {}/{} ({}). Using {}/{}.",
                saved_provider,
                saved_model_id,
                reason,
                current.provider,
                current.id
            )),
        ));
    }

    let available = registry.get_available()?;
    if let Some(model) = preferred_available_model(&available) {
        if should_print_messages {
            print_dimmed_model("Falling back to", &model.provider, &model.id);
        }
        return Ok((
            Some(model.clone()),
            Some(js_format!(
                "Could not restore model {}/{} ({}). Using {}/{}.",
                saved_provider,
                saved_model_id,
                reason,
                model.provider,
                model.id
            )),
        ));
    }
    Ok((None, None))
}
