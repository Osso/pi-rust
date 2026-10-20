use super::*;
use indexmap::IndexMap;

fn same(model: &JsValue, key: &str, value: &str) -> bool {
    equal(field(model, key), value)
}
fn numeric(value: &JsValue) -> Result<f64, GeneratorError> {
    Ok(match value {
        JsValue::Null => 0.0,
        JsValue::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        JsValue::Number(value) => *value,
        value => pi_js_compat::number::parse_number(
            &pi_js_compat::js_json::to_js_string(value)
                .map_err(GeneratorError::type_error)?
                .to_utf8_lossy(),
        ),
    })
}
fn merge_compat(model: &mut JsValue, patch: JsValue) {
    let mut compat = field(model, "compat").clone();
    if !matches!(compat, JsValue::Object(_)) {
        compat = object([]);
    }
    if let (JsValue::Object(compat), JsValue::Object(patch)) = (&mut compat, patch) {
        compat.extend(patch);
    }
    set(model, "compat", compat);
}
fn standard_cost(id: &JsValue) -> Option<JsValue> {
    if equal(id, "gpt-5.6-luna") {
        Some(literal(
            json!({"input":0.2,"output":1.2,"cacheRead":0.02,"cacheWrite":0.25}),
        ))
    } else if equal(id, "gpt-5.6-terra") {
        Some(literal(json!({"input":2,"output":12,"cacheRead":0.2,"cacheWrite":2.5})))
    } else {
        None
    }
}
fn long_context_cost(cost: &JsValue) -> Result<JsValue, GeneratorError> {
    let mut cost = cost.clone();
    let tier = object([
        ("inputTokensAbove", number(272000)),
        (
            "input",
            JsValue::Number(round_cost(numeric(field(&cost, "input"))? * 2.0)),
        ),
        (
            "output",
            JsValue::Number(round_cost(numeric(field(&cost, "output"))? * 1.5)),
        ),
        (
            "cacheRead",
            JsValue::Number(round_cost(numeric(field(&cost, "cacheRead"))? * 2.0)),
        ),
        (
            "cacheWrite",
            JsValue::Number(round_cost(numeric(field(&cost, "cacheWrite"))? * 2.0)),
        ),
    ]);
    set(&mut cost, "tiers", JsValue::Array(vec![tier]));
    Ok(cost)
}
fn rates(values: [f64; 4]) -> JsValue {
    object([
        ("input", JsValue::Number(values[0])),
        ("output", JsValue::Number(values[1])),
        ("cacheRead", JsValue::Number(values[2])),
        ("cacheWrite", JsValue::Number(values[3])),
    ])
}
fn is_id(model: &JsValue, ids: &[&str]) -> bool {
    ids.iter().any(|id| same(model, "id", id))
}

impl Generator<'_> {
    pub fn generate_catalog(&mut self) -> Result<JsValue, GeneratorError> {
        let mut all = vec![];
        for result in [
            self.load_models_dev_data()?,
            self.fetch_open_router_models()?,
            self.fetch_ai_gateway_models()?,
        ] {
            if let JsValue::Array(models) = result {
                all.extend(models);
            }
        }
        all.retain(|model| {
            !(same(model, "provider", "xai")
                && is_id(
                    model,
                    &[
                        "grok-3",
                        "grok-3-fast",
                        "grok-4.20-0309-non-reasoning",
                        "grok-4.20-0309-reasoning",
                        "grok-code-fast-1",
                    ],
                ))
                && !(is_id(model, &["gpt-5.3-codex-spark"])
                    && (same(model, "provider", "opencode") || same(model, "provider", "opencode-go")))
        });
        for model in &mut all {
            apply_overrides(model)?;
        }
        self.add_supplements(&mut all)?;
        for model in &mut all {
            if same(model, "api", "openai-completions")
                && includes(field(model, "id"), "deepseek-v4")
                && !is_id_provider(
                    model,
                    &["qwen-token-plan", "qwen-token-plan-cn", "qwen-token-plan-individual"],
                )
            {
                let native = is_id_provider(model, &["openrouter", "opencode"]);
                merge_compat(
                    model,
                    if native {
                        literal(json!({"requiresReasoningContentOnAssistantMessages":true}))
                    } else {
                        literal(json!({"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"}))
                    },
                );
            }
        }
        all.retain(|model| {
            !is_id_provider(model, &["minimax", "minimax-cn"])
                || is_id(model, &["MiniMax-M2.7", "MiniMax-M2.7-highspeed", "MiniMax-M3"])
        });
        add_codex(&mut all)?;
        if !all
            .iter()
            .any(|model| same(model, "provider", "mistral") && same(model, "id", "mistral-medium-3.5"))
        {
            all.push(literal(json!({"id":"mistral-medium-3.5","name":"Mistral Medium 3.5","api":"mistral-conversations","provider":"mistral","baseUrl":"https://api.mistral.ai","reasoning":true,"input":["text","image"],"cost":{"input":1.5,"output":7.5,"cacheRead":0,"cacheWrite":0},"contextWindow":262144,"maxTokens":262144})));
        }
        if !all
            .iter()
            .any(|model| same(model, "provider", "openrouter") && same(model, "id", "auto"))
        {
            all.push(literal(json!({"id":"auto","name":"Auto","api":"openai-completions","provider":"openrouter","baseUrl":"https://openrouter.ai/api/v1","reasoning":true,"input":["text","image"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":2000000,"maxTokens":30000})));
        }
        if !all
            .iter()
            .any(|model| same(model, "provider", "openrouter") && same(model, "id", "openrouter/fusion"))
        {
            all.push(literal(json!({"id":"openrouter/fusion","name":"OpenRouter: Fusion","api":"openai-completions","provider":"openrouter","baseUrl":"https://openrouter.ai/api/v1","reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":1000000,"maxTokens":30000})));
        }
        let azure: Vec<_> = all
            .iter()
            .filter(|model| same(model, "provider", "openai") && same(model, "api", "openai-responses"))
            .map(|model| {
                let mut azure = model.clone();
                set(&mut azure, "api", text("azure-openai-responses"));
                set(&mut azure, "provider", text("azure-openai-responses"));
                set(&mut azure, "baseUrl", text(""));
                let cost = field(model, "cost");
                set(
                    &mut azure,
                    "cost",
                    object([
                        ("input", field(cost, "input").clone()),
                        ("output", field(cost, "output").clone()),
                        ("cacheRead", field(cost, "cacheRead").clone()),
                        ("cacheWrite", field(cost, "cacheWrite").clone()),
                    ]),
                );
                if is_id(
                    model,
                    &["gpt-5.4", "gpt-5.5", "gpt-5.6-luna", "gpt-5.6-sol", "gpt-5.6-terra"],
                ) {
                    set(&mut azure, "contextWindow", number(1050000));
                }
                azure
            })
            .collect();
        all.extend(azure);
        for model in &mut all {
            metadata::apply_completions(model)?;
            let provider =
                pi_js_compat::js_json::to_js_string(field(model, "provider")).map_err(GeneratorError::type_error)?;
            let id = pi_js_compat::js_json::to_js_string(field(model, "id")).map_err(GeneratorError::type_error)?;
            let mut key = provider.units().to_vec();
            key.push(u16::from(b':'));
            key.extend_from_slice(id.units());
            if let Some(options) = self.reasoning_options.get(&JsString::from_utf16(key)) {
                if is_truthy(options) && direct_effort(model)? {
                    if let Some(map) = reasoning_map(options) {
                        let mut existing = field(model, "thinkingLevelMap").clone();
                        if !matches!(existing, JsValue::Object(_)) {
                            existing = object([]);
                        }
                        if let (JsValue::Object(existing), JsValue::Object(map)) = (&mut existing, map) {
                            existing.extend(map);
                        }
                        set(model, "thinkingLevelMap", existing);
                    }
                }
            }
            metadata::apply_after_reasoning(model)?;
        }
        self.total_models = all.len();
        self.reasoning_models = all.iter().filter(|model| is_truthy(field(model, "reasoning"))).count();
        let mut providers: IndexMap<JsString, IndexMap<JsString, JsValue>> = IndexMap::new();
        for model in all {
            let provider =
                pi_js_compat::js_json::to_js_string(field(&model, "provider")).map_err(GeneratorError::type_error)?;
            let id = pi_js_compat::js_json::to_js_string(field(&model, "id")).map_err(GeneratorError::type_error)?;
            let catalog = providers.entry(provider).or_default();
            // Pinned source tests truthiness through Object.prototype during dedup.
            if prototype_property(&id) {
                continue;
            }
            catalog.entry(id).or_insert(model);
        }
        self.provider_counts = providers
            .iter()
            .map(|(key, value)| (key.clone(), value.len()))
            .collect();
        let mut provider_ids = providers.keys().cloned().collect::<Vec<_>>();
        provider_ids.sort();
        let mut result = IndexMap::new();
        for provider in provider_ids {
            let mut ids = providers[&provider].keys().cloned().collect::<Vec<_>>();
            ids.sort();
            let mut models = IndexMap::new();
            for id in ids {
                models.insert(id.clone(), providers[&provider][&id].clone());
            }
            result.insert(provider, JsValue::Object(models));
        }
        Ok(JsValue::Object(result))
    }
    fn add_supplements(&self, all: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
        for (id, name, cost, context, max, long) in [
            (
                "gpt-5.6-sol",
                "GPT-5.6 Sol",
                [5.0, 30.0, 0.5, 6.25],
                272000,
                128000,
                true,
            ),
            (
                "gpt-5.6-terra",
                "GPT-5.6 Terra",
                [2.0, 12.0, 0.2, 2.5],
                272000,
                128000,
                true,
            ),
            (
                "gpt-5.6-luna",
                "GPT-5.6 Luna",
                [0.2, 1.2, 0.02, 0.25],
                272000,
                128000,
                true,
            ),
            (
                "gpt-5-chat-latest",
                "GPT-5 Chat Latest",
                [1.25, 10.0, 0.125, 0.0],
                128000,
                16384,
                false,
            ),
        ] {
            if all
                .iter()
                .any(|model| same(model, "provider", "openai") && same(model, "id", id))
            {
                continue;
            }
            let cost = rates(cost);
            let cost = if long { long_context_cost(&cost)? } else { cost };
            all.push(object([
                ("id", text(id)),
                ("name", text(name)),
                ("api", text("openai-responses")),
                ("baseUrl", text("https://api.openai.com/v1")),
                ("provider", text("openai")),
                ("reasoning", JsValue::Bool(long)),
                ("input", literal(json!(["text", "image"]))),
                ("cost", cost),
                ("contextWindow", number(context)),
                ("maxTokens", number(max)),
            ]));
        }
        for (id, name, cost) in [
            ("deepseek-v4-flash", "DeepSeek V4 Flash", [0.14, 0.28, 0.0028, 0.0]),
            ("deepseek-v4-pro", "DeepSeek V4 Pro", [0.435, 0.87, 0.003625, 0.0]),
        ] {
            all.push(object([
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
            all.push(object([
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
        Ok(())
    }
}
fn is_id_provider(model: &JsValue, providers: &[&str]) -> bool {
    providers.iter().any(|provider| same(model, "provider", provider))
}
fn direct_effort(model: &JsValue) -> Result<bool, GeneratorError> {
    if same(model, "api", "anthropic-messages") {
        return Ok(field(field(model, "compat"), "forceAdaptiveThinking") == &JsValue::Bool(true));
    }
    if is_id_api(
        model,
        &["openai-responses", "azure-openai-responses", "openai-codex-responses"],
    ) {
        return Ok(true);
    }
    if !same(model, "api", "openai-completions") {
        return Ok(false);
    }
    let mut detected = metadata::detect_compat(model)?;
    if let (JsValue::Object(detected), JsValue::Object(compat)) = (&mut detected, field(model, "compat")) {
        detected.extend(compat.clone());
    }
    Ok(same(&detected, "thinkingFormat", "openai") && is_truthy(field(&detected, "supportsReasoningEffort")))
}
fn is_id_api(model: &JsValue, apis: &[&str]) -> bool {
    apis.iter().any(|api| same(model, "api", api))
}
fn prototype_property(key: &JsString) -> bool {
    [
        "__proto__",
        "constructor",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "toString",
        "valueOf",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ]
    .iter()
    .any(|name| key == &JsString::from(*name))
}
fn apply_overrides(model: &mut JsValue) -> Result<(), GeneratorError> {
    let provider = field(model, "provider").clone();
    let id = field(model, "id").clone();
    if equal(&provider, "github-copilot")
        && is_id(
            model,
            &[
                "claude-fable-5",
                "claude-opus-4.6",
                "claude-opus-4.7",
                "claude-opus-4.8",
                "claude-opus-5",
                "claude-sonnet-4.6",
                "claude-sonnet-5",
                "gpt-5.3-codex",
                "gpt-5.4",
                "gpt-5.5",
            ],
        )
    {
        set(model, "contextWindow", number(1000000));
    }
    if is_id_provider(model, &["anthropic", "opencode", "opencode-go"])
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
    if is_id_provider(model, &["opencode", "opencode-go"]) {
        if is_id(model, &["claude-sonnet-4-5", "claude-sonnet-4"]) {
            set(model, "contextWindow", number(200000));
        }
        if same(model, "id", "gpt-5.4") {
            set(model, "contextWindow", number(272000));
            set(model, "maxTokens", number(128000));
        }
    }
    if equal(&provider, "openai") {
        if is_id(
            model,
            &["gpt-5.4", "gpt-5.5", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"],
        ) {
            set(model, "contextWindow", number(272000));
            set(model, "maxTokens", number(128000));
        }
        if is_id(
            model,
            &[
                "gpt-5.4",
                "gpt-5.4-pro",
                "gpt-5.5",
                "gpt-5.5-pro",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
            ],
        ) {
            let cost = standard_cost(&id).unwrap_or_else(|| field(model, "cost").clone());
            set(model, "cost", long_context_cost(&cost)?);
        }
        if equal(&id, "gpt-5-pro") {
            set(model, "maxTokens", number(128000));
        }
    }
    if equal(&provider, "cloudflare-ai-gateway") {
        if let Some(cost) = standard_cost(&id) {
            set(model, "cost", long_context_cost(&cost)?);
        }
    }
    if (equal(&provider, "openrouter") && (equal(&id, "moonshotai/kimi-k3") || equal(&id, "~moonshotai/kimi-latest")))
        || (equal(&provider, "vercel-ai-gateway") && equal(&id, "moonshotai/kimi-k3"))
    {
        set(model, "maxTokens", number(131072));
    }
    if equal(&provider, "openrouter") {
        if equal(&id, "moonshotai/kimi-k2.5") {
            let mut cost = field(model, "cost").clone();
            for (key, value) in [("input", 0.41), ("output", 2.06), ("cacheRead", 0.07)] {
                set(&mut cost, key, JsValue::Number(value));
            }
            set(model, "cost", cost);
            set(model, "maxTokens", number(4096));
        }
        if matches!(&id,JsValue::String(id) if id.units().starts_with(JsString::from("moonshotai/kimi-k2.6").units())) {
            merge_compat(
                model,
                literal(json!({"supportsDeveloperRole":false,"requiresReasoningContentOnAssistantMessages":true})),
            );
        }
        if equal(&id, "z-ai/glm-5") {
            let mut cost = field(model, "cost").clone();
            for (key, value) in [("input", 0.6), ("output", 1.9), ("cacheRead", 0.119)] {
                set(&mut cost, key, JsValue::Number(value));
            }
            set(model, "cost", cost);
        }
    }
    Ok(())
}
fn add_codex(all: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    let mut models = vec![];
    for (id, name, cost, context, image, long) in [
        (
            "gpt-5.3-codex-spark",
            "GPT-5.3 Codex Spark",
            [1.75, 14.0, 0.175, 0.0],
            128000,
            false,
            false,
        ),
        ("gpt-5.4", "GPT-5.4", [2.5, 15.0, 0.25, 0.0], 272000, true, true),
        (
            "gpt-5.4-mini",
            "GPT-5.4 mini",
            [0.75, 4.5, 0.075, 0.0],
            272000,
            true,
            false,
        ),
        ("gpt-5.5", "GPT-5.5", [5.0, 30.0, 0.5, 0.0], 272000, true, true),
        (
            "gpt-5.6-luna",
            "GPT-5.6 Luna",
            [0.2, 1.2, 0.02, 0.25],
            372000,
            true,
            true,
        ),
        ("gpt-5.6-sol", "GPT-5.6 Sol", [5.0, 30.0, 0.5, 6.25], 372000, true, true),
        (
            "gpt-5.6-terra",
            "GPT-5.6 Terra",
            [2.0, 12.0, 0.2, 2.5],
            372000,
            true,
            true,
        ),
    ] {
        let cost = rates(cost);
        let cost = if long { long_context_cost(&cost)? } else { cost };
        let mut model = object([
            ("id", text(id)),
            ("name", text(name)),
            ("api", text("openai-codex-responses")),
            ("provider", text("openai-codex")),
            ("baseUrl", text("https://chatgpt.com/backend-api")),
            ("reasoning", JsValue::Bool(true)),
            (
                "input",
                literal(if image {
                    json!(["text", "image"])
                } else {
                    json!(["text"])
                }),
            ),
            ("cost", cost),
            ("contextWindow", number(context)),
        ]);
        if id == "gpt-5.6-sol" {
            set(&mut model, "autoCompactionThreshold", number(272000));
        }
        set(&mut model, "maxTokens", number(128000));
        models.push(model);
    }
    let gc: Vec<_> = models
        .iter()
        .map(|model| {
            let mut model = model.clone();
            set(&mut model, "provider", text("openai-codex-gc"));
            model
        })
        .collect();
    all.extend(models);
    all.extend(gc);
    Ok(())
}
