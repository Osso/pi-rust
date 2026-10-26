use super::*;
use indexmap::IndexMap;

fn same(model: &JsValue, key: &str, value: &str) -> bool {
    equal(field(model, key), value)
}
fn is_id(model: &JsValue, ids: &[&str]) -> bool {
    ids.iter().any(|id| same(model, "id", id))
}
fn is_provider(model: &JsValue, providers: &[&str]) -> bool {
    providers.iter().any(|provider| same(model, "provider", provider))
}
pub(super) fn numeric(value: &JsValue) -> Result<f64, GeneratorError> {
    pi_js_compat::js_json::to_js_number(value).map_err(type_error)
}
fn rates(values: [f64; 4]) -> JsValue {
    object([
        ("input", JsValue::Number(values[0])),
        ("output", JsValue::Number(values[1])),
        ("cacheRead", JsValue::Number(values[2])),
        ("cacheWrite", JsValue::Number(values[3])),
    ])
}
impl Generator<'_> {
    pub fn generate_catalog(&mut self) -> Result<JsValue, GeneratorError> {
        let mut models = Vec::new();
        for fetched in [
            self.load_models_dev_data()?,
            self.fetch_open_router_models()?,
            self.fetch_ai_gateway_models()?,
        ] {
            if let JsValue::Array(fetched) = fetched {
                models.extend(fetched);
            }
        }
        models.retain(|model| {
            !(is_provider(model, &["opencode", "opencode-go"]) && same(model, "id", "gpt-5.3-codex-spark"))
        });
        for model in &mut models {
            apply_overrides(model)?;
        }
        add_direct_supplements(&mut models);
        for model in &mut models {
            if same(model, "api", "openai-completions") && includes(field(model, "id"), "deepseek-v4") {
                let patch = if is_provider(model, &["openrouter", "opencode"]) {
                    json!({"requiresReasoningContentOnAssistantMessages":true})
                } else {
                    json!({"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"})
                };
                metadata::merge(model, "compat", literal(patch));
            }
        }
        models.retain(|model| {
            !is_provider(model, &["minimax", "minimax-cn"])
                || is_id(model, &["MiniMax-M2.7", "MiniMax-M2.7-highspeed", "MiniMax-M3"])
        });
        add_codex(&mut models);
        add_missing_models(&mut models);
        for model in &mut models {
            metadata::apply_model_metadata(model)?;
        }
        self.total_models = models.len();
        self.reasoning_models = models
            .iter()
            .filter(|model| is_truthy(field(model, "reasoning")))
            .count();
        self.group_catalog(models)
    }
    fn group_catalog(&mut self, models: Vec<JsValue>) -> Result<JsValue, GeneratorError> {
        let mut providers: IndexMap<JsString, IndexMap<JsString, JsValue>> = IndexMap::new();
        for model in models {
            let JsValue::String(provider) = field(&model, "provider") else {
                return Err(type_error("Generated model.provider must be a string"));
            };
            let id = require_model_id(&model)?.clone();
            providers
                .entry(provider.clone())
                .or_default()
                .entry(id)
                .or_insert(model);
        }
        self.provider_counts = providers
            .iter()
            .map(|(provider, models)| (provider.clone(), models.len()))
            .collect();
        let mut result = IndexMap::new();
        let mut sorted = providers.into_iter().collect::<Vec<_>>();
        sorted.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (provider, models) in sorted {
            let mut models = models.into_iter().collect::<Vec<_>>();
            models.sort_by(|(left, _), (right, _)| left.cmp(right));
            result.insert(provider, JsValue::Object(models.into_iter().collect()));
        }
        Ok(JsValue::Object(result))
    }
}
fn apply_overrides(model: &mut JsValue) -> Result<(), GeneratorError> {
    if same(model, "provider", "anthropic")
        && is_id(model, &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"])
    {
        let mut patch = object([]);
        if !same(model, "id", "claude-haiku-5-5") {
            set(&mut patch, "off", JsValue::Null);
        }
        for (key, value) in [
            ("minimal", JsValue::Null),
            ("low", text("low")),
            ("medium", text("medium")),
            ("high", text("high")),
            ("xhigh", text("xhigh")),
            ("max", text("max")),
        ] {
            set(&mut patch, key, value);
        }
        metadata::merge(model, "thinkingLevelMap", patch);
    }
    if is_provider(model, &["anthropic", "opencode", "opencode-go", "github-copilot"])
        && is_id(
            model,
            &[
                "claude-opus-4-6",
                "claude-sonnet-4-6",
                "claude-opus-4.6",
                "claude-sonnet-4.6",
            ],
        )
    {
        set(model, "contextWindow", number(1000000));
    }
    if is_provider(model, &["opencode", "opencode-go"]) && is_id(model, &["claude-sonnet-4-5", "claude-sonnet-4"]) {
        set(model, "contextWindow", number(200000));
    }
    if (is_provider(model, &["opencode", "opencode-go"]) && same(model, "id", "gpt-5.4"))
        || (same(model, "provider", "openai") && is_id(model, &["gpt-5.4", "gpt-5.5"]))
    {
        set(model, "contextWindow", number(272000));
        set(model, "maxTokens", number(128000));
    }
    if same(model, "provider", "openai") && same(model, "id", "gpt-5-pro") {
        set(model, "maxTokens", number(128000));
    }
    if same(model, "provider", "openrouter") {
        override_router(model)?;
    }
    if same(model, "provider", "fireworks") && includes(field(model, "id"), "glm-5p2") {
        set(model, "api", text("openai-completions"));
        set(model, "baseUrl", text("https://api.fireworks.ai/inference/v1"));
        set(
            model,
            "compat",
            literal(json!({"supportsStore":false,"supportsDeveloperRole":false})),
        );
    }
    Ok(())
}
fn override_router(model: &mut JsValue) -> Result<(), GeneratorError> {
    if same(model, "id", "moonshotai/kimi-k2.5") {
        let mut cost = field(model, "cost").clone();
        for (key, value) in [("input", 0.41), ("output", 2.06), ("cacheRead", 0.07)] {
            set(&mut cost, key, JsValue::Number(value));
        }
        set(model, "cost", cost);
        set(model, "maxTokens", number(4096));
    }
    if require_model_id(model)?
        .units()
        .starts_with(JsString::from("moonshotai/kimi-k2.6").units())
    {
        metadata::merge(
            model,
            "compat",
            literal(json!({"supportsDeveloperRole":false,"requiresReasoningContentOnAssistantMessages":true})),
        );
    }
    if same(model, "id", "z-ai/glm-5") {
        let mut cost = field(model, "cost").clone();
        for (key, value) in [("input", 0.6), ("output", 1.9), ("cacheRead", 0.119)] {
            set(&mut cost, key, JsValue::Number(value));
        }
        set(model, "cost", cost);
    }
    Ok(())
}
fn add_direct_supplements(models: &mut Vec<JsValue>) {
    if !models
        .iter()
        .any(|model| same(model, "provider", "openai") && same(model, "id", "gpt-5-chat-latest"))
    {
        models.push(literal(json!({"id":"gpt-5-chat-latest","name":"GPT-5 Chat Latest","api":"openai-responses","baseUrl":"https://api.openai.com/v1","provider":"openai","reasoning":false,"input":["text","image"],"cost":{"input":1.25,"output":10,"cacheRead":0.125,"cacheWrite":0},"contextWindow":128000,"maxTokens":16384})));
    }
    for (id, name, cost) in [
        ("deepseek-v4-flash", "DeepSeek V4 Flash", [0.14, 0.28, 0.0028, 0.0]),
        ("deepseek-v4-pro", "DeepSeek V4 Pro", [0.435, 0.87, 0.003625, 0.0]),
    ] {
        models.push(object([
            ("id", text(id)),
            ("name", text(name)),
            ("api", text("openai-completions")),
            ("baseUrl", text("https://api.deepseek.com")),
            ("provider", text("deepseek")),
            ("reasoning", JsValue::Bool(true)),
            ("input", literal(json!(["text"]))),
            ("cost", rates(cost)),
            ("contextWindow", number(1000000)),
            ("maxTokens", number(384000)),
            (
                "compat",
                literal(json!({"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"})),
            ),
        ]));
    }
    for (id, name, reasoning, cost) in [
        ("Ling-2.6-flash", "Ling 2.6 Flash", false, [0.01, 0.02, 0.0, 0.0]),
        ("Ling-2.6-1T", "Ling 2.6 1T", false, [0.06, 0.25, 0.0, 0.0]),
        ("Ring-2.6-1T", "Ring 2.6 1T", true, [0.06, 0.25, 0.0, 0.0]),
    ] {
        let mut compat = literal(
            json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens","supportsLongCacheRetention":false}),
        );
        if reasoning {
            set(&mut compat, "thinkingFormat", text("ant-ling"));
        }
        models.push(object([
            ("id", text(id)),
            ("name", text(name)),
            ("api", text("openai-completions")),
            ("baseUrl", text("https://api.ant-ling.com/v1")),
            ("provider", text("ant-ling")),
            ("reasoning", JsValue::Bool(reasoning)),
            ("input", literal(json!(["text"]))),
            ("cost", rates(cost)),
            ("contextWindow", number(262144)),
            ("maxTokens", number(65536)),
            ("compat", compat),
        ]));
    }
}
fn add_codex(models: &mut Vec<JsValue>) {
    let mut codex = Vec::new();
    for (id, name, cost, context, image) in [
        (
            "gpt-5.3-codex-spark",
            "GPT-5.3 Codex Spark",
            [1.75, 14.0, 0.175, 0.0],
            128000,
            false,
        ),
        ("gpt-5.4", "GPT-5.4", [2.5, 15.0, 0.25, 0.0], 272000, true),
        ("gpt-5.4-mini", "GPT-5.4 mini", [0.75, 4.5, 0.075, 0.0], 272000, true),
        ("gpt-5.5", "GPT-5.5", [5.0, 30.0, 0.5, 0.0], 272000, true),
        ("gpt-5.6-terra", "GPT-5.6 Terra", [2.5, 15.0, 0.25, 0.0], 372000, true),
        ("gpt-5.6-luna", "GPT-5.6 Luna", [1.0, 6.0, 0.1, 1.25], 372000, true),
        ("gpt-5.6-sol", "GPT-5.6 Sol", [5.0, 30.0, 0.5, 0.0], 372000, true),
        ("gpt-6-sol", "GPT-6 Sol", [2.0, 10.0, 0.2, 2.5], 922000, true),
        ("gpt-6-luna", "GPT-6 Luna", [0.1, 0.5, 0.01, 0.125], 922000, true),
        ("gpt-6.1-terra", "GPT-6.1 Terra", [1.0, 5.0, 0.05, 1.25], 922000, true),
        ("gpt-6-astra", "GPT-6 Astra", [10.0, 50.0, 1.0, 12.5], 922000, true),
    ] {
        let mut model = object([
            ("id", text(id)),
            ("name", text(name)),
            ("api", text("openai-codex-responses")),
            ("provider", text("openai-codex")),
            ("baseUrl", text("https://chatgpt.com/backend-api")),
            ("reasoning", JsValue::Bool(true)),
        ]);
        if matches!(id, "gpt-6-sol" | "gpt-6-luna" | "gpt-6.1-terra") {
            set(&mut model, "thinkingLevelMap", literal(json!({"max":"max"})));
        }
        set(
            &mut model,
            "input",
            literal(if image {
                json!(["text", "image"])
            } else {
                json!(["text"])
            }),
        );
        set(&mut model, "cost", rates(cost));
        set(&mut model, "contextWindow", number(context));
        if id == "gpt-5.6-sol" {
            set(&mut model, "autoCompactionThreshold", number(272000));
        }
        set(&mut model, "maxTokens", number(128000));
        codex.push(model);
    }
    for provider in ["openai-codex", "openai-codex-gc", "openai-codex-team"] {
        models.extend(codex.iter().map(|model| {
            let mut model = model.clone();
            set(&mut model, "provider", text(provider));
            model
        }));
    }
}
fn add_missing_models(models: &mut Vec<JsValue>) {
    for (id, name, cost, context) in [
        ("grok-3", "Grok 3", [3.0, 15.0, 0.75, 0.0], 131072),
        ("grok-3-fast", "Grok 3 Fast", [5.0, 25.0, 1.25, 0.0], 131072),
        ("grok-code-fast-1", "Grok Code Fast 1", [0.2, 1.5, 0.02, 0.0], 32768),
    ] {
        if !models
            .iter()
            .any(|model| same(model, "provider", "xai") && same(model, "id", id))
        {
            models.push(object([
                ("id", text(id)),
                ("name", text(name)),
                ("api", text("openai-completions")),
                ("baseUrl", text("https://api.x.ai/v1")),
                ("provider", text("xai")),
                ("reasoning", JsValue::Bool(false)),
                ("input", literal(json!(["text"]))),
                ("cost", rates(cost)),
                ("contextWindow", number(context)),
                ("maxTokens", number(8192)),
            ]));
        }
    }
    if !models
        .iter()
        .any(|model| same(model, "provider", "mistral") && same(model, "id", "mistral-medium-3.5"))
    {
        models.push(literal(json!({"id":"mistral-medium-3.5","name":"Mistral Medium 3.5","api":"mistral-conversations","provider":"mistral","baseUrl":"https://api.mistral.ai","reasoning":true,"input":["text","image"],"cost":{"input":1.5,"output":7.5,"cacheRead":0,"cacheWrite":0},"contextWindow":262144,"maxTokens":262144})));
    }
    for (id, name, input, context) in [
        ("auto", "Auto", json!(["text", "image"]), 2000000),
        ("openrouter/fusion", "OpenRouter: Fusion", json!(["text"]), 1000000),
    ] {
        if !models
            .iter()
            .any(|model| same(model, "provider", "openrouter") && same(model, "id", id))
        {
            models.push(object([
                ("id", text(id)),
                ("name", text(name)),
                ("api", text("openai-completions")),
                ("provider", text("openrouter")),
                ("baseUrl", text("https://openrouter.ai/api/v1")),
                ("reasoning", JsValue::Bool(true)),
                ("input", literal(input)),
                ("cost", rates([0.0; 4])),
                ("contextWindow", number(context)),
                ("maxTokens", number(30000)),
            ]));
        }
    }
}
