#[path = "../support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::Model;
use pi_coding_agent::core::model_resolver::{
    FindInitialModelOptions, ModelResolverRegistry, ParseModelPatternOptions, ResolveCliModelOptions, ScopedModel,
    default_model_for_provider, find_exact_model_reference_match, find_initial_model, parse_model_pattern,
    resolve_cli_model, resolve_model_scope_from_models,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn model(provider: &str, id: &str, name: &str, reasoning: bool) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "api": "anthropic-messages",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": reasoning,
        "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1},
        "contextWindow": 128000,
        "maxTokens": 8192,
    }))
    .unwrap()
}

fn models_json(models: &[Model]) -> Value {
    Value::Array(
        models
            .iter()
            .map(|model| serde_json::to_value(model).unwrap())
            .collect(),
    )
}

fn normalize_model(model: Option<&Model>) -> Value {
    match model {
        Some(model) => json!({
            "id": model.id,
            "provider": model.provider,
            "name": model.name,
            "baseUrl": model.base_url,
            "reasoning": model.reasoning,
        }),
        None => Value::Null,
    }
}

fn thinking(level: Option<ThinkingLevel>) -> Value {
    match level {
        Some(level) => Value::String(level.as_str().to_owned()),
        None => Value::Null,
    }
}

fn fixture_models() -> Vec<Model> {
    vec![
        model("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5", true),
        model("anthropic", "claude-sonnet-4-5-20250929", "Claude Sonnet 4.5", true),
        model("openai", "gpt-4o", "GPT-4o", false),
        model("openrouter", "qwen/qwen3-coder:exacto", "Qwen3 Coder Exacto", true),
        model("openrouter", "openai/gpt-4o:extended", "GPT-4o Extended", false),
        model("zai", "glm-5", "GLM-5", true),
        model("vercel-ai-gateway", "zai/glm-5", "GLM-5", true),
        model(
            "commandcode",
            "xiaomi/mimo-v2.5-pro",
            "Xiaomi MiMo via Commandcode",
            false,
        ),
        model("xiaomi", "mimo-v2.5-pro", "Xiaomi MiMo", false),
        model("neuralwatt", "some-base-model", "Some Base Model", false),
        model("custom", "bracketed-model[1m]", "Bracketed Model", true),
        model("azure-openai-responses", "gpt-5.6-sol", "GPT 5.6 Sol", true),
        model("openai-codex", "gpt-5.6-sol", "GPT 5.6 Sol", true),
        model("CaseProvider", "case-map-a", "Case A", false),
        model("caseprovider", "case-map-b", "Case B", false),
    ]
}

fn parse_cases() -> Vec<(&'static str, Option<bool>)> {
    vec![
        ("claude-sonnet-4-5", None),
        ("sonnet", None),
        ("sonnet:high", None),
        ("gpt-4o:medium", None),
        ("sonnet:off", None),
        ("sonnet:minimal", None),
        ("sonnet:low", None),
        ("sonnet:xhigh", None),
        ("sonnet:max", None),
        ("sonnet:ultra", None),
        ("sonnet:random", None),
        ("gpt-4o:invalid", None),
        ("qwen/qwen3-coder:exacto", None),
        ("openrouter/qwen/qwen3-coder:exacto", None),
        ("qwen/qwen3-coder:exacto:high", None),
        ("openrouter/qwen/qwen3-coder:exacto:high", None),
        ("openai/gpt-4o:extended", None),
        ("qwen/qwen3-coder:exacto:random", None),
        ("qwen/qwen3-coder:exacto:high:random", None),
        ("", None),
        ("sonnet:", None),
        ("gpt-4o:extended", Some(false)),
        ("nonexistent", None),
    ]
}

fn compare_parse(oracle: &mut oracle::TsOracle, models: &[Model]) {
    for (index, (pattern, allow)) in parse_cases().into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "parse",
            "pattern": pattern,
            "models": models_json(models),
            "allowInvalidThinkingLevelFallback": allow,
        });
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{expected}");
        let result = parse_model_pattern(
            &JsString::from(pattern),
            models,
            ParseModelPatternOptions {
                allow_invalid_thinking_level_fallback: allow,
            },
        );
        let actual = json!({
            "model": normalize_model(result.model.as_ref()),
            "thinkingLevel": thinking(result.thinking_level),
            "warning": result.warning,
        });
        assert_eq!(actual, expected["output"], "parse {pattern}");
    }
}

fn compare_scope(oracle: &mut oracle::TsOracle, models: &[Model]) {
    let cases: Vec<Vec<&str>> = vec![
        vec!["sonnet:high", "gpt-4o:invalid", "missing"],
        vec!["custom/bracketed-model[1m]"],
        vec!["custom/bracketed-model[1m]:high"],
        vec!["*sonnet*"],
        vec!["openrouter/*/*:high"],
        vec!["nope/*"],
        vec!["sonnet", "claude-sonnet-4-5"],
        vec!["zai/glm-5", "xiaomi/mimo-v2.5-pro"],
    ];
    for (index, patterns) in cases.into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "scope",
            "patterns": patterns,
            "models": models_json(models),
        });
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{expected}");
        let owned: Vec<String> = patterns.iter().map(|pattern| (*pattern).to_owned()).collect();
        let result = resolve_model_scope_from_models(&owned.iter().map(JsString::from).collect::<Vec<_>>(), models);
        let actual = json!({
            "scopedModels": result
                .scoped_models
                .iter()
                .map(|scoped| json!({
                    "model": normalize_model(Some(&scoped.model)),
                    "thinkingLevel": thinking(scoped.thinking_level),
                }))
                .collect::<Vec<_>>(),
            "diagnostics": result
                .diagnostics
                .iter()
                .map(|diagnostic| json!({
                    "type": diagnostic.diagnostic_type,
                    "message": diagnostic.message,
                    "pattern": diagnostic.pattern,
                }))
                .collect::<Vec<_>>(),
        });
        assert_eq!(actual, expected["output"], "scope {patterns:?}");
    }
}

struct StubRuntime {
    models: Vec<Model>,
    available: Vec<Model>,
    authenticated: Vec<String>,
}

impl ModelResolverRegistry for StubRuntime {
    fn get_all(&self) -> Vec<Model> {
        self.models.clone()
    }

    fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError> {
        Ok(self.available.clone())
    }

    fn find(&self, provider: &str, model_id: &str) -> Option<Model> {
        self.models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
            .cloned()
    }

    fn has_configured_auth(&self, model: &Model) -> Result<bool, pi_js_compat::node_error::NativeNodeError> {
        let provider = model.provider.as_str();
        Ok(self.authenticated.iter().any(|authenticated| authenticated == provider))
    }
}

type CliCase = (
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    Vec<&'static str>,
);

fn cli_cases() -> Vec<CliCase> {
    vec![
        ("openai/gpt-4o", None, None, vec![]),
        ("4o", Some("openai"), None, vec![]),
        ("sonnet:high", None, None, vec![]),
        ("openai/gpt-4o:extended", None, None, vec![]),
        ("gpt-4o:extended", Some("openai"), None, vec![]),
        ("openrouter/openai/ghost-model", Some("openrouter"), None, vec![]),
        ("gpt-5.6-sol", None, None, vec!["openai-codex"]),
        ("gpt-5.6-sol", None, None, vec![]),
        ("zai/glm-5", None, None, vec!["zai", "vercel-ai-gateway"]),
        ("xiaomi/mimo-v2.5-pro", None, None, vec!["commandcode"]),
        ("openrouter/qwen", None, None, vec![]),
        ("neuralwatt/zai-org/GLM-5.1-FP8:high", None, None, vec![]),
        ("neuralwatt/zai-org/GLM-5.1-FP8", None, None, vec![]),
        ("neuralwatt/zai-org/GLM-5.1-FP8:banana", None, None, vec![]),
        ("zai-org/GLM-5.1-FP8:high", Some("neuralwatt"), None, vec![]),
        ("neuralwatt/zai-org/GLM-5.1-FP8:high", None, Some("medium"), vec![]),
        ("gpt-4o", Some("missing-provider"), None, vec![]),
    ]
}

fn compare_cli(oracle: &mut oracle::TsOracle, models: &[Model], cases: Vec<CliCase>) {
    for (index, (cli_model, cli_provider, cli_thinking, authenticated)) in cases.into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "cli",
            "models": models_json(models),
            "authenticated": authenticated,
            "cliModel": cli_model,
            "cliProvider": cli_provider,
            "cliThinking": cli_thinking,
        });
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{expected}");
        let runtime = StubRuntime {
            models: models.to_vec(),
            available: models.to_vec(),
            authenticated: authenticated.iter().map(|value| (*value).to_owned()).collect(),
        };
        let thinking_level = cli_thinking.map(|value| match value {
            "off" => ThinkingLevel::Off,
            "minimal" => ThinkingLevel::Minimal,
            "low" => ThinkingLevel::Low,
            "medium" => ThinkingLevel::Medium,
            "high" => ThinkingLevel::High,
            "xhigh" => ThinkingLevel::XHigh,
            "max" => ThinkingLevel::Max,
            _ => ThinkingLevel::Ultra,
        });
        let result = resolve_cli_model(
            &ResolveCliModelOptions {
                cli_provider: cli_provider.map(str::to_owned),
                cli_model: Some(cli_model.to_owned().into()),
                cli_thinking: thinking_level,
            },
            &runtime,
        )
        .unwrap();
        let actual = json!({
            "model": normalize_model(result.model.as_ref()),
            "thinkingLevel": thinking(result.thinking_level),
            "warning": result.warning,
            "error": result.error,
        });
        assert_eq!(actual, expected["output"], "cli {cli_model}");
    }
}

fn compare_defaults(oracle: &mut oracle::TsOracle) {
    let expected = oracle.request(&json!({"id": 0, "operation": "defaults"}));
    assert!(expected["ok"].as_bool().unwrap());
    let mut actual = serde_json::Map::new();
    for (provider, default) in pi_coding_agent::core::model_resolver::DEFAULT_MODEL_PER_PROVIDER {
        actual.insert((*provider).to_owned(), Value::String((*default).to_owned()));
    }
    for (provider, default) in expected["output"]["defaults"].as_object().unwrap() {
        assert_eq!(
            actual.get(provider).and_then(Value::as_str),
            default.as_str(),
            "{provider}"
        );
    }
    assert_eq!(actual.len(), expected["output"]["defaults"].as_object().unwrap().len());
    assert_eq!(default_model_for_provider("radius"), None);
}

fn compare_exact(oracle: &mut oracle::TsOracle, models: &[Model]) {
    for (index, reference) in [
        "gpt-5.6-sol",
        "openai-codex/gpt-5.6-sol",
        "azure-openai-responses/gpt-5.6-sol",
        "custom/bracketed-model[1m]",
        "   ",
        "SONNET",
        "anthropic/claude-sonnet-4-5",
    ]
    .into_iter()
    .enumerate()
    {
        let request = json!({
            "id": index,
            "operation": "exact",
            "reference": reference,
            "models": models_json(models),
        });
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{expected}");
        let actual = normalize_model(find_exact_model_reference_match(&reference.into(), models).as_ref());
        assert_eq!(actual, expected["output"]["model"], "exact {reference}");
    }
}

async fn compare_initial(oracle: &mut oracle::TsOracle, models: &[Model]) {
    let openai = model("openai", "gpt-5.5", "GPT-5.5", true);
    let codex = model("openai-codex", "gpt-5.5", "GPT-5.5", true);
    let gateway = model(
        "vercel-ai-gateway",
        "anthropic/claude-opus-4-6",
        "Claude Opus 4.6",
        true,
    );
    type InitialCase = (Value, Vec<Model>, Vec<Model>, Vec<String>);
    let cases: Vec<InitialCase> = vec![
        (
            json!({"cliProvider": "", "cliModel": "gpt-4o", "scopedModels": [{"model": serde_json::to_value(&openai).unwrap()}], "isContinuing": false}),
            models.to_vec(),
            models.to_vec(),
            vec![],
        ),
        (
            json!({"scopedModels": [], "isContinuing": false}),
            vec![openai.clone(), codex.clone()],
            vec![openai.clone(), codex.clone()],
            vec!["openai".to_owned(), "openai-codex".to_owned()],
        ),
        (
            json!({
                "cliProvider": "openrouter",
                "cliModel": "openrouter/openai/ghost-model",
                "scopedModels": [],
                "isContinuing": false
            }),
            models.to_vec(),
            models.to_vec(),
            vec![],
        ),
        (
            json!({"scopedModels": [], "isContinuing": false}),
            vec![gateway.clone()],
            vec![gateway.clone()],
            vec!["vercel-ai-gateway".to_owned()],
        ),
        (
            json!({
                "scopedModels": [{"model": serde_json::to_value(&openai).unwrap(), "thinkingLevel": "high"}],
                "isContinuing": false
            }),
            models.to_vec(),
            models.to_vec(),
            vec![],
        ),
        (
            json!({
                "scopedModels": [{"model": serde_json::to_value(&openai).unwrap()}],
                "isContinuing": true
            }),
            models.to_vec(),
            models.to_vec(),
            vec![],
        ),
        (
            json!({
                "scopedModels": [],
                "isContinuing": false,
                "defaultProvider": "deepseek",
                "defaultModelId": "deepseek-v4-flash"
            }),
            models.to_vec(),
            models.to_vec(),
            vec!["spark-two".to_owned()],
        ),
    ];

    for (index, (options, all, available, authenticated)) in cases.into_iter().enumerate() {
        let mut request = json!({
            "id": index,
            "operation": "initial",
            "models": models_json(&all),
            "available": models_json(&available),
            "authenticated": authenticated,
        });
        for (key, value) in options.as_object().unwrap() {
            request[key] = value.clone();
        }
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{expected}");
        let runtime = StubRuntime {
            models: all.clone(),
            available: available.clone(),
            authenticated,
        };
        let scoped_models: Vec<ScopedModel> = request["scopedModels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| ScopedModel {
                model: serde_json::from_value(entry["model"].clone()).unwrap(),
                thinking_level: entry["thinkingLevel"].as_str().map(|value| match value {
                    "off" => ThinkingLevel::Off,
                    "minimal" => ThinkingLevel::Minimal,
                    "low" => ThinkingLevel::Low,
                    "medium" => ThinkingLevel::Medium,
                    "high" => ThinkingLevel::High,
                    "xhigh" => ThinkingLevel::XHigh,
                    "max" => ThinkingLevel::Max,
                    _ => ThinkingLevel::Ultra,
                }),
            })
            .collect();
        let result = find_initial_model(
            &FindInitialModelOptions {
                cli_provider: request["cliProvider"].as_str().map(str::to_owned),
                cli_model: (request["cliModel"].as_str().map(str::to_owned)).map(JsString::from),
                scoped_models,
                is_continuing: request["isContinuing"].as_bool().unwrap_or(false),
                default_provider: request["defaultProvider"].as_str().map(str::to_owned),
                default_model_id: request["defaultModelId"].as_str().map(str::to_owned),
                default_thinking_level: None,
            },
            &runtime,
        )
        .await
        .unwrap();
        let actual = json!({
            "model": normalize_model(result.model.as_ref()),
            "thinkingLevel": result.thinking_level.as_str(),
            "fallbackMessage": result.fallback_message,
        });
        assert_eq!(actual, expected["output"], "initial case {index}");
    }
}

fn compare_restore() {
    use std::ffi::OsString;
    let models = fixture_models();
    for color in [false, true] {
        let color_env = [("FORCE_COLOR", OsString::from(if color { "1" } else { "0" }))];
        let mut source = oracle::TsOracle::spawn_with_environment(
            "model-resolver-oracle.ts",
            "packages/coding-agent/src/core/model-resolver.ts",
            &color_env,
        );
        for (provider, id, current, available, authenticated) in [
            ("openai", "gpt-4o", None, models.clone(), vec!["openai"]),
            ("missing", "gone", Some(models[0].clone()), models.clone(), vec![]),
            ("openai", "gpt-4o", Some(models[0].clone()), models.clone(), vec![]),
            (
                "missing",
                "gone",
                None,
                vec![model("openai", "gpt-5.5", "Default", true), models[0].clone()],
                vec![],
            ),
            ("openai", "gpt-4o", None, vec![models[0].clone()], vec![]),
            ("missing", "gone", None, vec![], vec![]),
        ] {
            for print in [false, true] {
                let request = json!({
                    "id": 1, "operation": "restore", "models": models_json(&models),
                    "savedProvider": provider, "savedModelId": id, "currentModel": current,
                    "available": models_json(&available), "authenticated": authenticated, "print": print,
                });
                let expected = source.request(&request);
                assert_eq!(expected["ok"], true, "{expected}");
                let mut environment = color_env.to_vec();
                environment.push(("PI_RESTORE_CASE", OsString::from(request.to_string())));
                let child = env_probe::run_probe("resolver_restore_probe_child", &environment);
                let stdout = String::from_utf8(child.stdout).unwrap();
                let (_, captured) = stdout.split_once("RESTORE_BEGIN\n").unwrap();
                let (messages, result) = captured.split_once("RESTORE_END:").unwrap();
                let mut actual: Value = serde_json::from_str(result.lines().next().unwrap()).unwrap();
                actual["stdout"] = json!(messages);
                actual["stderr"] = json!(String::from_utf8(child.stderr).unwrap());
                assert_eq!(actual, expected["output"], "restore={request}, color={color}");
            }
        }
    }
}

pub(crate) fn list_models_stdout_color_probe() {
    use std::process::{Command, Stdio};
    let models = fixture_models();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "list-models-oracle.ts",
        "packages/coding-agent/src/cli/list-models.ts",
        &[("FORCE_COLOR", "0".into())],
    );
    let expected =
        source.request(&json!({"id": 1, "loadError": "fixture warning", "models": models_json(&models[..1])}));
    assert_eq!(expected["ok"], true, "{expected}");
    let directory = tempfile::tempdir().unwrap();
    let quote = |value: &std::path::Path| format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"));
    let command = format!(
        "{} --ignored --exact resolver_restore_probe_child --nocapture > {}",
        quote(&std::env::current_exe().unwrap()),
        quote(&directory.path().join("stdout"))
    );
    let output = Command::new("script")
        .args(["-qefc", &command, "/dev/null"])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("TERM", "xterm-256color")
        .env(env_probe::PROBE_ENV, "1")
        .env("PI_LIST_MODEL_COLOR_PROBE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    // The pty's standard ONLCR output transform adds CR before each console newline.
    let expected = format!("{}\n", expected["output"]["warning"].as_str().unwrap()).replace('\n', "\r\n");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

fn list_models_color_child() {
    use pi_coding_agent::cli::list_models::{ListModelsRuntime, StdioListModelsIo, list_models};
    struct Runtime;
    impl ListModelsRuntime for Runtime {
        fn get_error(&self) -> Option<JsString> {
            Some("fixture warning".into())
        }
        fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError> {
            Ok(fixture_models()[..1].to_vec())
        }
    }
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(list_models(&Runtime, None, &StdioListModelsIo))
        .unwrap();
}

pub(crate) fn restore_child() {
    if std::env::var_os("PI_LIST_MODEL_COLOR_PROBE").is_some() {
        list_models_color_child();
        return;
    }
    if !env_probe::in_probe() {
        return;
    }
    let request: Value = serde_json::from_str(&std::env::var("PI_RESTORE_CASE").unwrap()).unwrap();
    let registry = StubRuntime {
        models: serde_json::from_value(request["models"].clone()).unwrap(),
        available: serde_json::from_value(request["available"].clone()).unwrap(),
        authenticated: serde_json::from_value(request["authenticated"].clone()).unwrap(),
    };
    let current: Option<Model> = serde_json::from_value(request["currentModel"].clone()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    println!("RESTORE_BEGIN");
    let (model, fallback) = runtime
        .block_on(pi_coding_agent::core::model_resolver::restore_model_from_session(
            request["savedProvider"].as_str().unwrap(),
            request["savedModelId"].as_str().unwrap(),
            current.as_ref(),
            request["print"].as_bool().unwrap(),
            &registry,
        ))
        .unwrap();
    println!("RESTORE_END:{}", json!({"model": model, "fallbackMessage": fallback}));
}

pub(crate) fn run(operation: &str) {
    let mut oracle = oracle::TsOracle::spawn(
        "model-resolver-oracle.ts",
        "packages/coding-agent/src/core/model-resolver.ts",
    );
    let models = fixture_models();
    match operation {
        "restore" => compare_restore(),
        "parse" => compare_parse(&mut oracle, &models),
        "scope" => compare_scope(&mut oracle, &models),
        "cli" => compare_cli(&mut oracle, &models, cli_cases()),
        "cli_empty_model" => compare_cli(&mut oracle, &models, vec![("", None, None, vec![])]),
        "cli_blank_model" => compare_cli(&mut oracle, &models, vec![(" ", None, None, vec![])]),
        "cli_empty_provider" => compare_cli(&mut oracle, &models, vec![("gpt-4o", Some(""), None, vec![])]),
        "cli_casefold_provider" => compare_cli(
            &mut oracle,
            &models,
            vec![("case-map-b", Some("CASEPROVIDER"), None, vec![])],
        ),
        "defaults" => compare_defaults(&mut oracle),
        "exact" => compare_exact(&mut oracle, &models),
        "initial" => {
            let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
            runtime.block_on(compare_initial(&mut oracle, &models));
        }
        _ => panic!("unknown resolver proof case: {operation}"),
    }
}
