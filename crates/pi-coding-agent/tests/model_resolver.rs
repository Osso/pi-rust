//! Ports `packages/coding-agent/test/model-resolver.test.ts`.

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::Model;
use pi_coding_agent::core::model_resolver::{
    DEFAULT_MODEL_PER_PROVIDER, FindInitialModelOptions, ModelResolverRuntime, ParseModelPatternOptions,
    ResolveCliModelOptions, ScopedModel, default_model_for_provider, find_exact_model_reference_match,
    find_initial_model, parse_model_pattern, resolve_cli_model, resolve_model_scope_from_models,
    restore_model_from_session,
};
use serde_json::json;

fn model(provider: &str, id: &str, name: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "api": "anthropic-messages",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": true,
        "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1},
        "contextWindow": 128000,
        "maxTokens": 8192,
    }))
    .unwrap()
}

fn mock_models() -> Vec<Model> {
    vec![
        model("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5"),
        model("openai", "gpt-4o", "GPT-4o"),
    ]
}

fn openrouter_models() -> Vec<Model> {
    vec![
        model("openrouter", "qwen/qwen3-coder:exacto", "Qwen3 Coder Exacto"),
        model("openrouter", "openai/gpt-4o:extended", "GPT-4o Extended"),
    ]
}

fn all_models() -> Vec<Model> {
    let mut models = mock_models();
    models.extend(openrouter_models());
    models
}

fn pattern(pattern: &str, models: &[Model]) -> (Option<String>, Option<ThinkingLevel>, Option<String>) {
    let result = parse_model_pattern(pattern, models, ParseModelPatternOptions::default());
    (
        result.model.map(|model| model.id),
        result.thinking_level,
        result.warning,
    )
}

#[test]
fn parse_simple_patterns() {
    assert_eq!(
        pattern("claude-sonnet-4-5", &all_models()).0.as_deref(),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(pattern("sonnet", &all_models()).0.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(pattern("nonexistent", &all_models()), (None, None, None));
}

#[test]
fn parse_valid_thinking_levels() {
    assert_eq!(
        pattern("sonnet:high", &all_models()),
        (Some("claude-sonnet-4-5".to_owned()), Some(ThinkingLevel::High), None)
    );
    assert_eq!(
        pattern("gpt-4o:medium", &all_models()),
        (Some("gpt-4o".to_owned()), Some(ThinkingLevel::Medium), None)
    );
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        let result = pattern(&format!("sonnet:{level}"), &all_models());
        assert_eq!(result.0.as_deref(), Some("claude-sonnet-4-5"), "{level}");
        assert!(result.2.is_none(), "{level}");
    }
}

#[test]
fn parse_invalid_thinking_levels_warns() {
    let (model, thinking, warning) = pattern("sonnet:random", &all_models());
    assert_eq!(model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(thinking, None);
    let warning = warning.unwrap();
    assert!(warning.contains("Invalid thinking level"));
    assert!(warning.contains("random"));

    let (model, thinking, warning) = pattern("gpt-4o:invalid", &all_models());
    assert_eq!(model.as_deref(), Some("gpt-4o"));
    assert_eq!(thinking, None);
    assert!(warning.unwrap().contains("Invalid thinking level"));
}

#[test]
fn parse_openrouter_colon_ids() {
    assert_eq!(
        pattern("qwen/qwen3-coder:exacto", &all_models()),
        (Some("qwen/qwen3-coder:exacto".to_owned()), None, None)
    );
    let result = parse_model_pattern("openrouter/qwen/qwen3-coder:exacto", &all_models(), Default::default());
    assert_eq!(
        result.model.as_ref().map(|model| model.provider.as_str()),
        Some("openrouter")
    );
    assert_eq!(
        result.model.as_ref().map(|model| model.id.as_str()),
        Some("qwen/qwen3-coder:exacto")
    );
    assert_eq!(
        pattern("qwen/qwen3-coder:exacto:high", &all_models()),
        (
            Some("qwen/qwen3-coder:exacto".to_owned()),
            Some(ThinkingLevel::High),
            None
        )
    );
    assert_eq!(
        pattern("openrouter/qwen/qwen3-coder:exacto:high", &all_models()),
        (
            Some("qwen/qwen3-coder:exacto".to_owned()),
            Some(ThinkingLevel::High),
            None
        )
    );
    assert_eq!(
        pattern("openai/gpt-4o:extended", &all_models()),
        (Some("openai/gpt-4o:extended".to_owned()), None, None)
    );
}

#[test]
fn parse_invalid_thinking_levels_with_openrouter_ids() {
    let (model, thinking, warning) = pattern("qwen/qwen3-coder:exacto:random", &all_models());
    assert_eq!(model.as_deref(), Some("qwen/qwen3-coder:exacto"));
    assert_eq!(thinking, None);
    assert!(warning.unwrap().contains("random"));

    let (model, thinking, warning) = pattern("qwen/qwen3-coder:exacto:high:random", &all_models());
    assert_eq!(model.as_deref(), Some("qwen/qwen3-coder:exacto"));
    assert_eq!(thinking, None);
    assert!(warning.unwrap().contains("random"));
}

#[test]
fn parse_edge_cases() {
    assert!(pattern("", &all_models()).0.is_some());
    let (model, thinking, warning) = pattern("sonnet:", &all_models());
    assert_eq!(model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(thinking, None);
    assert!(warning.unwrap().contains("Invalid thinking level"));
}

#[test]
fn strict_mode_rejects_invalid_suffix() {
    let result = parse_model_pattern(
        "gpt-4o:extended",
        &mock_models(),
        ParseModelPatternOptions {
            allow_invalid_thinking_level_fallback: Some(false),
        },
    );
    assert_eq!(result.model, None);
    assert_eq!(result.warning, None);
}

#[test]
fn exact_reference_rejects_ambiguity() {
    let models = vec![
        model("azure-openai-responses", "gpt-5.6-sol", "GPT 5.6 Sol"),
        model("openai-codex", "gpt-5.6-sol", "GPT 5.6 Sol"),
    ];
    assert!(find_exact_model_reference_match(&"gpt-5.6-sol".into(), &models).is_none());
    assert_eq!(
        find_exact_model_reference_match(&"openai-codex/gpt-5.6-sol".into(), &models).map(|model| model.provider),
        Some("openai-codex".to_owned())
    );
    assert!(find_exact_model_reference_match(&"   ".into(), &models).is_none());
}

#[test]
fn alias_preference_prefers_alias_over_dated_and_latest_alias_descending() {
    let models = vec![
        model("anthropic", "claude-sonnet-4-5-20250929", "Sonnet"),
        model("anthropic", "claude-sonnet-4-5", "Sonnet"),
        model("anthropic", "claude-sonnet-4-5-latest", "Sonnet"),
    ];
    let result = parse_model_pattern("sonnet", &models, Default::default());
    assert_eq!(
        result.model.map(|model| model.id),
        Some("claude-sonnet-4-5-latest".to_owned())
    );

    let dated_only = vec![
        model("anthropic", "claude-sonnet-4-5-20250929", "Sonnet"),
        model("anthropic", "claude-sonnet-4-5-20250101", "Sonnet"),
    ];
    let result = parse_model_pattern("sonnet", &dated_only, Default::default());
    assert_eq!(
        result.model.map(|model| model.id),
        Some("claude-sonnet-4-5-20250929".to_owned())
    );
}

#[test]
fn scope_diagnostics_match_the_source_order() {
    let result = resolve_model_scope_from_models(
        &[
            "sonnet:high".to_owned(),
            "gpt-4o:invalid".to_owned(),
            "missing".to_owned(),
        ],
        &all_models(),
    );
    assert_eq!(
        result
            .scoped_models
            .iter()
            .map(|scoped| scoped.model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-sonnet-4-5", "gpt-4o"]
    );
    assert_eq!(result.scoped_models[0].thinking_level, Some(ThinkingLevel::High));
    assert_eq!(result.scoped_models[1].thinking_level, None);
    assert_eq!(result.diagnostics.len(), 2);
    assert_eq!(
        result.diagnostics[0].message,
        "Invalid thinking level \"invalid\" in pattern \"gpt-4o:invalid\". Using default instead."
    );
    assert_eq!(result.diagnostics[0].code.as_str(), "invalid-thinking-level");
    assert_eq!(result.diagnostics[0].pattern, "gpt-4o:invalid");
    assert_eq!(result.diagnostics[1].message, "No models match pattern \"missing\"");
    assert_eq!(result.diagnostics[1].code.as_str(), "no-match");
}

#[test]
fn scope_resolves_bracketed_ids_before_glob_matching() {
    let mut models = all_models();
    models.push(model("custom", "bracketed-model[1m]", "Bracketed Model"));
    let result = resolve_model_scope_from_models(&["custom/bracketed-model[1m]".to_owned()], &models);
    assert_eq!(
        result
            .scoped_models
            .iter()
            .map(|scoped| scoped.model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["bracketed-model[1m]"]
    );
    assert!(result.diagnostics.is_empty());

    let result = resolve_model_scope_from_models(&["custom/bracketed-model[1m]:high".to_owned()], &models);
    assert_eq!(result.scoped_models[0].model.id, "bracketed-model[1m]");
    assert_eq!(result.scoped_models[0].thinking_level, Some(ThinkingLevel::High));
    assert!(result.diagnostics.is_empty());
}

#[test]
fn scope_globs_match_provider_and_bare_ids() {
    let models = all_models();
    let result = resolve_model_scope_from_models(&["*sonnet*".to_owned()], &models);
    assert_eq!(result.scoped_models.len(), 1);
    assert_eq!(result.scoped_models[0].model.id, "claude-sonnet-4-5");

    let result = resolve_model_scope_from_models(&["openrouter/*/*:high".to_owned()], &models);
    assert_eq!(result.scoped_models.len(), 2);
    assert!(
        result
            .scoped_models
            .iter()
            .all(|scoped| scoped.thinking_level == Some(ThinkingLevel::High))
    );

    let result = resolve_model_scope_from_models(&["nope/*".to_owned()], &models);
    assert!(result.scoped_models.is_empty());
    assert_eq!(result.diagnostics[0].message, "No models match pattern \"nope/*\"");
}

#[test]
fn scope_deduplicates_models() {
    let models = all_models();
    let result = resolve_model_scope_from_models(&["sonnet".to_owned(), "claude-sonnet-4-5".to_owned()], &models);
    assert_eq!(result.scoped_models.len(), 1);
}

#[test]
fn defaults_match_the_source_catalog() {
    assert_eq!(default_model_for_provider("openai"), Some("gpt-5.5"));
    assert_eq!(default_model_for_provider("openai-codex"), Some("gpt-5.5"));
    assert_eq!(default_model_for_provider("openai-codex-gc"), Some("gpt-5.5"));
    assert_eq!(default_model_for_provider("zai"), Some("glm-5.1"));
    assert_eq!(default_model_for_provider("minimax"), Some("MiniMax-M2.7"));
    assert_eq!(default_model_for_provider("minimax-cn"), Some("MiniMax-M2.7"));
    assert_eq!(default_model_for_provider("cerebras"), Some("zai-glm-4.7"));
    assert_eq!(default_model_for_provider("ant-ling"), Some("Ring-2.6-1T"));
    assert_eq!(default_model_for_provider("vercel-ai-gateway"), Some("zai/glm-5.1"));
    assert_eq!(
        default_model_for_provider("qwen-token-plan-individual"),
        Some("qwen3.8-max")
    );
    assert_eq!(DEFAULT_MODEL_PER_PROVIDER.len(), 41);
}

struct StubRuntime {
    models: Vec<Model>,
    authenticated: Vec<&'static str>,
}

impl ModelResolverRuntime for StubRuntime {
    fn get_models(&self) -> Vec<Model> {
        self.models.clone()
    }

    fn get_available_snapshot(&self) -> Vec<Model> {
        self.models.clone()
    }

    fn get_model(&self, provider: &str, model_id: &str) -> Option<Model> {
        self.models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
            .cloned()
    }

    fn has_configured_auth(&self, provider: &str) -> bool {
        self.authenticated.contains(&provider)
    }

    fn available_models(
        &self,
        _options: pi_ai::auth::types::AuthOperationOptions,
    ) -> pi_ai::request::BoxFuture<'_, Vec<Model>> {
        Box::pin(async { self.models.clone() })
    }
}

fn stub(models: Vec<Model>, authenticated: Vec<&'static str>) -> StubRuntime {
    StubRuntime { models, authenticated }
}

fn cli(
    options: ResolveCliModelOptions,
    runtime: &dyn ModelResolverRuntime,
) -> (Option<String>, Option<String>, Option<ThinkingLevel>, Option<String>) {
    let result = resolve_cli_model(&options, runtime);
    (
        result.model.as_ref().map(|model| model.provider.clone()),
        result.model.map(|model| model.id),
        result.thinking_level,
        result.error,
    )
}

#[test]
fn cli_resolves_provider_id_without_provider_flag() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, thinking, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("openai/gpt-4o".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openai"));
    assert_eq!(id.as_deref(), Some("gpt-4o"));
    assert_eq!(thinking, None);
    assert_eq!(error, None);
}

#[test]
fn cli_resolves_fuzzy_patterns_within_provider() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_provider: Some("openai".to_owned()),
            cli_model: Some("4o".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openai"));
    assert_eq!(id.as_deref(), Some("gpt-4o"));
    assert_eq!(error, None);
}

#[test]
fn cli_parses_thinking_suffix() {
    let runtime = stub(all_models(), vec![]);
    let (_, id, thinking, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("sonnet:high".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(id.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(thinking, Some(ThinkingLevel::High));
    assert_eq!(error, None);
}

#[test]
fn cli_prefers_exact_id_over_provider_inference() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("openai/gpt-4o:extended".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openrouter"));
    assert_eq!(id.as_deref(), Some("openai/gpt-4o:extended"));
    assert_eq!(error, None);
}

#[test]
fn cli_keeps_invalid_suffix_as_part_of_the_id() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_provider: Some("openai".to_owned()),
            cli_model: Some("gpt-4o:extended".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openai"));
    assert_eq!(id.as_deref(), Some("gpt-4o:extended"));
    assert_eq!(error, None);
}

#[test]
fn cli_allows_custom_ids_without_double_prefixing() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_provider: Some("openrouter".to_owned()),
            cli_model: Some("openrouter/openai/ghost-model".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openrouter"));
    assert_eq!(id.as_deref(), Some("openai/ghost-model"));
    assert_eq!(error, None);
}

#[test]
fn cli_reports_no_models() {
    let runtime = stub(Vec::new(), vec![]);
    let (model, _, _, error) = cli(
        ResolveCliModelOptions {
            cli_provider: Some("openai".to_owned()),
            cli_model: Some("gpt-4o".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(model, None);
    assert!(error.unwrap().contains("No models available"));
}

#[test]
fn cli_prefers_the_sole_authenticated_provider_for_ambiguous_ids() {
    let models = vec![
        model("azure-openai-responses", "gpt-5.6-sol", "GPT 5.6 Sol"),
        model("openai-codex", "gpt-5.6-sol", "GPT 5.6 Sol"),
    ];
    let runtime = stub(models, vec!["openai-codex"]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("gpt-5.6-sol".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openai-codex"));
    assert_eq!(id.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(error, None);
}

#[test]
fn cli_requires_a_provider_for_ambiguous_ids_without_unique_auth() {
    let models = vec![
        model("azure-openai-responses", "gpt-5.6-sol", "GPT 5.6 Sol"),
        model("openai-codex", "gpt-5.6-sol", "GPT 5.6 Sol"),
    ];
    let runtime = stub(models, vec![]);
    let (model, _, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("gpt-5.6-sol".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(model, None);
    let error = error.unwrap();
    assert!(error.contains("Model \"gpt-5.6-sol\" is ambiguous across providers"));
    assert!(error.contains("azure-openai-responses/gpt-5.6-sol"));
    assert!(error.contains("openai-codex/gpt-5.6-sol"));
    assert!(error.contains("Use --provider or provider/model"));
}

#[test]
fn cli_prefers_provider_split_over_gateway_model_id() {
    let mut models = all_models();
    models.push(model("zai", "glm-5", "GLM-5"));
    models.push(model("vercel-ai-gateway", "zai/glm-5", "GLM-5"));
    let runtime = stub(models, vec!["zai", "vercel-ai-gateway"]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("zai/glm-5".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("zai"));
    assert_eq!(id.as_deref(), Some("glm-5"));
    assert_eq!(error, None);
}

#[test]
fn cli_prefers_authenticated_raw_id_over_unauthenticated_inferred_provider() {
    let mut models = all_models();
    models.push(model(
        "commandcode",
        "xiaomi/mimo-v2.5-pro",
        "Xiaomi MiMo via Commandcode",
    ));
    models.push(model("xiaomi", "mimo-v2.5-pro", "Xiaomi MiMo"));
    let runtime = stub(models, vec!["commandcode"]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("xiaomi/mimo-v2.5-pro".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("commandcode"));
    assert_eq!(id.as_deref(), Some("xiaomi/mimo-v2.5-pro"));
    assert_eq!(error, None);
}

#[test]
fn cli_resolves_provider_prefixed_fuzzy_patterns() {
    let runtime = stub(all_models(), vec![]);
    let (provider, id, _, error) = cli(
        ResolveCliModelOptions {
            cli_model: Some("openrouter/qwen".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(provider.as_deref(), Some("openrouter"));
    assert_eq!(id.as_deref(), Some("qwen/qwen3-coder:exacto"));
    assert_eq!(error, None);
}

fn neuralwatt_models() -> Vec<Model> {
    let mut models = all_models();
    models.push(model("neuralwatt", "some-base-model", "Some Base Model"));
    models
}

#[test]
fn cli_strips_thinking_suffix_from_custom_model_id() {
    let runtime = stub(neuralwatt_models(), vec![]);
    let result = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_model: Some("neuralwatt/zai-org/GLM-5.1-FP8:high".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    let model = result.model.unwrap();
    assert_eq!(model.provider, "neuralwatt");
    assert_eq!(model.id, "zai-org/GLM-5.1-FP8");
    assert!(model.reasoning);
    assert_eq!(result.thinking_level, Some(ThinkingLevel::High));
    assert_eq!(result.error, None);
}

#[test]
fn cli_custom_model_without_suffix_works() {
    let runtime = stub(neuralwatt_models(), vec![]);
    let result = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_model: Some("neuralwatt/zai-org/GLM-5.1-FP8".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    let model = result.model.unwrap();
    assert_eq!(model.id, "zai-org/GLM-5.1-FP8");
    assert_eq!(result.thinking_level, None);
}

#[test]
fn cli_all_valid_thinking_levels_work_in_fallback_path() {
    let runtime = stub(neuralwatt_models(), vec![]);
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        let result = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_model: Some(format!("neuralwatt/zai-org/GLM-5.1-FP8:{level}")),
                ..Default::default()
            },
            &runtime,
        );
        assert_eq!(
            result.model.as_ref().map(|model| model.id.as_str()),
            Some("zai-org/GLM-5.1-FP8")
        );
        assert_eq!(
            result.thinking_level,
            Some(match level {
                "off" => ThinkingLevel::Off,
                "minimal" => ThinkingLevel::Minimal,
                "low" => ThinkingLevel::Low,
                "medium" => ThinkingLevel::Medium,
                "high" => ThinkingLevel::High,
                "xhigh" => ThinkingLevel::XHigh,
                "max" => ThinkingLevel::Max,
                _ => ThinkingLevel::Ultra,
            })
        );
    }
}

#[test]
fn cli_invalid_suffix_on_custom_model_stays_in_id() {
    let runtime = stub(neuralwatt_models(), vec![]);
    let result = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_model: Some("neuralwatt/zai-org/GLM-5.1-FP8:banana".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(result.model.unwrap().id, "zai-org/GLM-5.1-FP8:banana");
    assert_eq!(result.thinking_level, None);
}

#[test]
fn cli_explicit_provider_strips_custom_model_thinking_suffix() {
    let runtime = stub(neuralwatt_models(), vec![]);
    let result = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_provider: Some("neuralwatt".to_owned()),
            cli_model: Some("zai-org/GLM-5.1-FP8:high".to_owned()),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(result.model.unwrap().id, "zai-org/GLM-5.1-FP8");
    assert_eq!(result.thinking_level, Some(ThinkingLevel::High));
}

#[test]
fn cli_explicit_thinking_keeps_suffix_in_id() {
    let runtime = stub(neuralwatt_models(), vec![]);
    let result = resolve_cli_model(
        &ResolveCliModelOptions {
            cli_model: Some("neuralwatt/zai-org/GLM-5.1-FP8:high".to_owned()),
            cli_thinking: Some(ThinkingLevel::Medium),
            ..Default::default()
        },
        &runtime,
    );
    assert_eq!(result.model.unwrap().id, "zai-org/GLM-5.1-FP8:high");
    assert_eq!(result.thinking_level, None);
}

fn scoped(model: Model, thinking: Option<ThinkingLevel>) -> ScopedModel {
    ScopedModel {
        model,
        thinking_level: thinking,
    }
}

#[tokio::test]
async fn initial_model_prefers_openai_codex_over_openai() {
    let openai = model("openai", "gpt-5.5", "GPT-5.5");
    let codex = model("openai-codex", "gpt-5.5", "GPT-5.5");
    let runtime = stub(vec![openai, codex], vec!["openai", "openai-codex"]);
    let result = find_initial_model(
        &FindInitialModelOptions {
            scoped_models: Vec::new(),
            is_continuing: false,
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    let model = result.model.unwrap();
    assert_eq!(model.provider, "openai-codex");
    assert_eq!(model.id, "gpt-5.5");
}

#[tokio::test]
async fn initial_model_accepts_explicit_provider_custom_ids() {
    let runtime = stub(all_models(), vec![]);
    let result = find_initial_model(
        &FindInitialModelOptions {
            cli_provider: Some("openrouter".to_owned()),
            cli_model: Some("openrouter/openai/ghost-model".to_owned()),
            scoped_models: Vec::new(),
            is_continuing: false,
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    let model = result.model.unwrap();
    assert_eq!(model.provider, "openrouter");
    assert_eq!(model.id, "openai/ghost-model");
}

#[tokio::test]
async fn initial_model_selects_ai_gateway_default() {
    let gateway = model("vercel-ai-gateway", "anthropic/claude-opus-4-6", "Claude Opus 4.6");
    let runtime = stub(vec![gateway], vec!["vercel-ai-gateway"]);
    let result = find_initial_model(
        &FindInitialModelOptions {
            scoped_models: Vec::new(),
            is_continuing: false,
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    let model = result.model.unwrap();
    assert_eq!(model.provider, "vercel-ai-gateway");
    assert_eq!(model.id, "anthropic/claude-opus-4-6");
}

#[tokio::test]
async fn initial_model_ignores_unauthenticated_saved_default() {
    let saved = model("deepseek", "deepseek-v4-flash", "DeepSeek V4 Flash");
    let local = model("spark-two", "deepseek-v4-flash", "DeepSeek V4 Flash");
    // `get_model` finds the saved default but its provider has no auth; the
    // available snapshot (local model) wins.
    struct LocalRuntime {
        saved: Model,
        local: Model,
    }
    impl ModelResolverRuntime for LocalRuntime {
        fn get_models(&self) -> Vec<Model> {
            vec![self.saved.clone(), self.local.clone()]
        }
        fn get_available_snapshot(&self) -> Vec<Model> {
            vec![self.local.clone()]
        }
        fn get_model(&self, provider: &str, model_id: &str) -> Option<Model> {
            (provider == self.saved.provider && model_id == self.saved.id).then(|| self.saved.clone())
        }
        fn has_configured_auth(&self, provider: &str) -> bool {
            provider == "spark-two"
        }
        fn available_models(
            &self,
            _options: pi_ai::auth::types::AuthOperationOptions,
        ) -> pi_ai::request::BoxFuture<'_, Vec<Model>> {
            Box::pin(async { vec![self.local.clone()] })
        }
    }
    let runtime = LocalRuntime { saved, local };
    let result = find_initial_model(
        &FindInitialModelOptions {
            scoped_models: Vec::new(),
            is_continuing: false,
            default_provider: Some("deepseek".to_owned()),
            default_model_id: Some("deepseek-v4-flash".to_owned()),
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    let model = result.model.unwrap();
    assert_eq!(model.provider, "spark-two");
    assert_eq!(model.id, "deepseek-v4-flash");
}

#[tokio::test]
async fn initial_model_uses_first_scoped_model_unless_continuing() {
    let runtime = stub(all_models(), vec![]);
    let sonnet = model("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5");
    let result = find_initial_model(
        &FindInitialModelOptions {
            scoped_models: vec![scoped(sonnet.clone(), Some(ThinkingLevel::High))],
            is_continuing: false,
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    assert_eq!(result.model.unwrap().id, "claude-sonnet-4-5");
    assert_eq!(result.thinking_level, ThinkingLevel::High);

    let result = find_initial_model(
        &FindInitialModelOptions {
            scoped_models: vec![scoped(sonnet, None)],
            is_continuing: true,
            ..Default::default()
        },
        &runtime,
    )
    .await
    .unwrap();
    assert_eq!(result.model.unwrap().id, "claude-sonnet-4-5");
}

#[tokio::test]
async fn restore_falls_back_when_auth_is_missing() {
    let runtime = stub(
        vec![model("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5")],
        vec![],
    );
    let (model, message) = restore_model_from_session("anthropic", "claude-sonnet-4-5", None, false, &runtime).await;
    let model = model.unwrap();
    assert_eq!(model.id, "claude-sonnet-4-5");
    assert_eq!(
        message.as_deref(),
        Some(
            "Could not restore model anthropic/claude-sonnet-4-5 (no auth configured). Using anthropic/claude-sonnet-4-5."
        )
    );
}

#[tokio::test]
async fn restore_keeps_current_model_as_fallback() {
    let runtime = stub(vec![], vec![]);
    let current = model("openai", "gpt-4o", "GPT-4o");
    let (model, message) = restore_model_from_session("anthropic", "missing", Some(&current), false, &runtime).await;
    assert_eq!(model.unwrap().id, "gpt-4o");
    assert_eq!(
        message.as_deref(),
        Some("Could not restore model anthropic/missing (model no longer exists). Using openai/gpt-4o.")
    );
}

#[tokio::test]
async fn restore_returns_none_when_nothing_is_available() {
    let runtime = stub(vec![], vec![]);
    let (model, message) = restore_model_from_session("anthropic", "missing", None, false, &runtime).await;
    assert_eq!(model, None);
    assert_eq!(message, None);
}
