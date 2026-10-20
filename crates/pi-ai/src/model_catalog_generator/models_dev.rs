use super::*;
use indexmap::IndexMap;

const INDIVIDUAL_IDS: &[&str] = &[
    "deepseek-v4-flash-0731",
    "deepseek-v4-pro",
    "glm-5.2",
    "qwen3.6-flash",
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.8-max",
];
const NVIDIA_UNSUPPORTED: &[&str] = &[
    "abacusai/dracarys-llama-3.1-70b-instruct",
    "bytedance/seed-oss-36b-instruct",
    "deepseek-ai/deepseek-v4-flash",
    "deepseek-ai/deepseek-v4-pro",
    "google/gemma-2-2b-it",
    "google/gemma-3n-e2b-it",
    "google/gemma-3n-e4b-it",
    "google/gemma-4-31b-it",
    "meta/llama-3.2-1b-instruct",
    "meta/llama-4-maverick-17b-128e-instruct",
    "microsoft/phi-4-mini-instruct",
    "minimaxai/minimax-m2.7",
    "mistralai/mistral-nemotron",
    "nvidia/nemotron-mini-4b-instruct",
    "qwen/qwen3-next-80b-a3b-instruct",
    "qwen/qwen3.5-397b-a17b",
    "sarvamai/sarvam-m",
    "upstage/solar-10.7b-instruct",
];

fn matches_id(id: &JsString, names: &[&str]) -> bool {
    names.iter().any(|name| id == &JsString::from(*name))
}
fn normalized_nvidia(id: &JsString) -> JsString {
    let mut units = Vec::new();
    let mut run = String::new();
    for character in char::decode_utf16(id.units().iter().copied()) {
        match character {
            Ok(character) => run.push(character),
            Err(surrogate) => {
                units.extend(run.to_lowercase().encode_utf16());
                run.clear();
                units.push(surrogate.unpaired_surrogate());
            }
        }
    }
    units.extend(run.to_lowercase().encode_utf16());
    for unit in &mut units {
        if *unit == u16::from(b'_') {
            *unit = u16::from(b'.');
        }
    }
    JsString::from_utf16(units)
}
fn starts(id: &JsString, prefix: &str) -> bool {
    id.units().starts_with(JsString::from(prefix).units())
}
fn source_models<'a>(data: &'a JsValue, key: &str) -> Result<Vec<(&'a JsString, &'a JsValue)>, GeneratorError> {
    receiver_types::model_entries(field(data, key))
}

fn insert_before(model: &mut JsValue, key: &str, extra: JsValue) {
    let (JsValue::Object(model), JsValue::Object(extra)) = (model, extra) else {
        return;
    };
    let mut result = IndexMap::new();
    for (existing, value) in std::mem::take(model) {
        if existing == JsString::from(key) {
            result.extend(extra.clone());
        }
        result.insert(existing, value);
    }
    *model = result;
}
fn source_model(
    id: &JsString,
    source: &JsValue,
    provider: &str,
    api: &str,
    url: &str,
) -> Result<JsValue, GeneratorError> {
    validate_modalities(source)?;
    let common = common_model(id, source, provider)?;
    Ok(object([
        ("id", field(&common, "id").clone()),
        ("name", field(&common, "name").clone()),
        ("api", text(api)),
        ("provider", text(provider)),
        ("baseUrl", text(url)),
        ("reasoning", field(&common, "reasoning").clone()),
        ("input", field(&common, "input").clone()),
        ("cost", field(&common, "cost").clone()),
        ("contextWindow", field(&common, "contextWindow").clone()),
        ("maxTokens", field(&common, "maxTokens").clone()),
    ]))
}
fn tool_models<'a>(
    data: &'a JsValue,
    key: &str,
) -> Result<impl Iterator<Item = Result<(&'a JsString, &'a JsValue), GeneratorError>>, GeneratorError> {
    Ok(source_models(data, key)?.into_iter().filter_map(|(id, model)| {
        if let Err(error) = receiver_types::model(model) {
            Some(Err(error))
        } else if field(model, "tool_call") == &JsValue::Bool(true) {
            Some(Ok((id, model)))
        } else {
            None
        }
    }))
}

fn base_openai_compat() -> JsValue {
    literal(
        json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens","supportsStrictMode":false,"supportsLongCacheRetention":false}),
    )
}

impl Generator<'_> {
    fn record(&mut self, provider: &str, id: &JsString, model: &JsValue) {
        if let JsValue::Object(model) = model {
            if let Some(options) = model.get(&JsString::from("reasoning_options")) {
                let mut key = provider.encode_utf16().collect::<Vec<_>>();
                key.push(u16::from(b':'));
                key.extend_from_slice(id.units());
                self.reasoning_options
                    .insert(JsString::from_utf16(key), options.clone());
            }
        }
    }
    pub fn fetch_nvidia_nim_model_ids(&mut self) -> Result<IndexMap<JsString, JsString>, GeneratorError> {
        self.log("log", "Fetching models from NVIDIA NIM API...");
        let result = self
            .fetch_json("https://integrate.api.nvidia.com/v1/models", "NVIDIA NIM")
            .and_then(|data| {
                if matches!(data, JsValue::Null) {
                    return Err(GeneratorError::type_error(
                        "Cannot read properties of null (reading 'data')",
                    ));
                }
                let items = match field(&data, "data") {
                    JsValue::Null => &[][..],
                    JsValue::Array(items) => items.as_slice(),
                    _ => {
                        return Err(GeneratorError::type_error(
                            "object is not iterable (cannot read property Symbol(Symbol.iterator))",
                        ));
                    }
                };
                let mut ids = IndexMap::new();
                for model in items {
                    let JsValue::String(id) = field(model, "id") else {
                        return Err(GeneratorError::type_error("modelId.toLowerCase is not a function"));
                    };
                    ids.insert(id.clone(), id.clone());
                    ids.insert(normalized_nvidia(id), id.clone());
                }
                self.log("log", &format!("Fetched {} model IDs from NVIDIA NIM", items.len()));
                Ok(ids)
            });
        match result {
            Ok(ids) => Ok(ids),
            Err(error) => {
                self.emit(JsValue::Array(vec![
                    text("error"),
                    text("Failed to fetch NVIDIA NIM models:"),
                    text(&error.console_message()),
                ]));
                if self.options.strict {
                    Err(error)
                } else {
                    Ok(IndexMap::new())
                }
            }
        }
    }
    pub fn load_models_dev_data(&mut self) -> Result<JsValue, GeneratorError> {
        self.log("log", "Fetching models from models.dev API...");
        let result = self
            .fetch_json("https://models.dev/api.json", "models.dev")
            .and_then(|data| {
                emission_types::expect(&data, "models.dev response", "object")?;
                receiver_types::optional_object(field(&data, "nvidia"), "models.dev provider")?;
                let nvidia = if is_truthy(field(field(&data, "nvidia"), "models")) {
                    self.fetch_nvidia_nim_model_ids()?
                } else {
                    IndexMap::new()
                };
                self.transform_models_dev(&data, &nvidia)
            });
        match result {
            Ok(models) => {
                let count = match &models {
                    JsValue::Array(models) => models.len(),
                    _ => 0,
                };
                self.log("log", &format!("Loaded {count} tool-capable models from models.dev"));
                Ok(models)
            }
            Err(error) => {
                self.emit(JsValue::Array(vec![
                    text("error"),
                    text("Failed to load models.dev data:"),
                    text(&error.console_message()),
                ]));
                if self.options.strict {
                    Err(error)
                } else {
                    Ok(JsValue::Array(vec![]))
                }
            }
        }
    }
    fn transform_models_dev(
        &mut self,
        data: &JsValue,
        nvidia: &IndexMap<JsString, JsString>,
    ) -> Result<JsValue, GeneratorError> {
        let mut models = vec![];
        for (key, provider, api, url) in [
            ("amazon-bedrock", "amazon-bedrock", "bedrock-converse-stream", ""),
            (
                "anthropic",
                "anthropic",
                "anthropic-messages",
                "https://api.anthropic.com",
            ),
            (
                "google",
                "google",
                "google-generative-ai",
                "https://generativelanguage.googleapis.com/v1beta",
            ),
            (
                "google-vertex",
                "google-vertex",
                "google-vertex",
                "https://{location}-aiplatform.googleapis.com",
            ),
            ("openai", "openai", "openai-responses", "https://api.openai.com/v1"),
            ("groq", "groq", "openai-completions", "https://api.groq.com/openai/v1"),
            (
                "cerebras",
                "cerebras",
                "openai-completions",
                "https://api.cerebras.ai/v1",
            ),
            (
                "cloudflare-workers-ai",
                "cloudflare-workers-ai",
                "openai-completions",
                crate::api::cloudflare::CLOUDFLARE_WORKERS_AI_BASE_URL,
            ),
            ("cloudflare-ai-gateway", "cloudflare-ai-gateway", "", ""),
            ("xai", "xai", "openai-completions", "https://api.x.ai/v1"),
        ] {
            for entry in tool_models(data, key)? {
                let (id, m) = entry?;
                if provider == "amazon-bedrock"
                    && (matches_id(id, &["anthropic.claude-opus-5"])
                        || starts(id, "ai21.jamba")
                        || starts(id, "mistral.mistral-7b-instruct-v0"))
                {
                    continue;
                }
                if provider == "google-vertex"
                    && (!starts(id, "gemini-") || matches_id(id, &["gemini-3.1-flash-lite-preview"]))
                {
                    continue;
                }
                if provider == "openai" && matches_id(id, &["gpt-5.6"]) {
                    continue;
                }
                let mut source = m;
                if matches!(provider, "google" | "google-vertex") {
                    let target = if matches_id(id, &["gemini-flash-latest"]) {
                        Some("gemini-3.5-flash")
                    } else if matches_id(id, &["gemini-flash-lite-latest"]) {
                        Some("gemini-3.1-flash-lite")
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        let candidate = field(field(field(data, key), "models"), target);
                        if !matches!(candidate, JsValue::Null) {
                            source = candidate;
                        }
                    }
                }
                let mut model_id = id.clone();
                let mut model_api = api;
                let mut base = url;
                let mut head = object([]);
                let mut tail = object([]);
                if provider == "amazon-bedrock" {
                    base = if starts(id, "eu.") {
                        "https://bedrock-runtime.eu-central-1.amazonaws.com"
                    } else {
                        "https://bedrock-runtime.us-east-1.amazonaws.com"
                    };
                    if field(m, "structured_output") == &JsValue::Bool(true) {
                        tail = literal(json!({"compat":{"supportsStrictMode":true}}));
                    }
                }
                if provider == "cloudflare-ai-gateway" {
                    let Some(slash) = id.units().iter().position(|unit| *unit == u16::from(b'/')) else {
                        continue;
                    };
                    let upstream = JsString::from_utf16(id.units()[..slash].to_vec());
                    let native = JsString::from_utf16(id.units()[slash + 1..].to_vec());
                    if upstream == JsString::from("openai") {
                        model_api = "openai-responses";
                        base = crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL;
                        model_id = native;
                    } else if upstream == JsString::from("anthropic") {
                        model_api = "anthropic-messages";
                        base = crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL;
                        model_id = native;
                    } else if upstream == JsString::from("workers-ai") {
                        model_api = "openai-completions";
                        base = crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL;
                    } else {
                        continue;
                    }
                    if upstream != JsString::from("openai") {
                        tail = literal(json!({"compat":{"sendSessionAffinityHeaders":true}}));
                    }
                }
                if provider == "cloudflare-workers-ai" {
                    tail = literal(json!({"compat":{"sendSessionAffinityHeaders":true}}));
                }
                if provider == "xai" && matches_id(id, &["grok-4.5"]) {
                    model_api = "openai-responses";
                    head = literal(json!({"compat":{"supportsLongCacheRetention":false}}));
                }
                let mut model = source_model(&model_id, source, provider, model_api, base)?;
                set(
                    &mut model,
                    "name",
                    fallback(field(m, "name"), JsValue::String(model_id.clone())),
                );
                insert_before(&mut model, "reasoning", head);
                if provider == "google-vertex" {
                    let mut cost = field(&model, "cost").clone();
                    if matches_id(id, &["gemini-2.5-flash"]) {
                        set(&mut cost, "cacheRead", JsValue::Number(0.03));
                    }
                    set(&mut cost, "cacheWrite", number(0));
                    set(&mut model, "cost", cost);
                }
                if let (JsValue::Object(model), JsValue::Object(tail)) = (&mut model, tail) {
                    model.extend(tail);
                }
                self.record(provider, &model_id, source);
                models.push(model);
            }
        }
        for (provider, url) in [
            ("zai", "https://api.z.ai/api/coding/paas/v4"),
            ("zai-coding-cn", "https://open.bigmodel.cn/api/coding/paas/v4"),
        ] {
            for entry in tool_models(data, "zai-coding-plan")? {
                let (id, m) = entry?;
                let glm = matches_id(id, &["glm-5.2"]);
                let mut model = source_model(id, m, provider, "openai-completions", url)?;
                if glm {
                    insert_before(
                        &mut model,
                        "input",
                        literal(
                            json!({"thinkingLevelMap":{"minimal":null,"low":"high","medium":"high","high":"high","xhigh":"max"}}),
                        ),
                    );
                }
                let mut compat = literal(json!({"supportsDeveloperRole":false,"thinkingFormat":"zai"}));
                if glm {
                    set(&mut compat, "supportsReasoningEffort", JsValue::Bool(true));
                }
                if !matches_id(id, &["glm-4.5", "glm-4.5-air", "glm-4.5-flash", "glm-4.5v"]) {
                    set(&mut compat, "zaiToolStream", JsValue::Bool(true));
                }
                insert_before(&mut model, "contextWindow", object([("compat", compat)]));
                self.record(provider, id, m);
                models.push(model);
            }
        }
        for (key, api, url) in [
            ("mistral", "mistral-conversations", "https://api.mistral.ai"),
            ("huggingface", "openai-completions", "https://router.huggingface.co/v1"),
        ] {
            for entry in tool_models(data, key)? {
                let (id, m) = entry?;
                let mut model = source_model(id, m, key, api, url)?;
                if key == "mistral" {
                    let input = field(field(m, "cost"), "input");
                    let cache = field(field(m, "cost"), "cache_read");
                    let read = if matches!(cache, JsValue::Null) {
                        JsValue::Number(if is_truthy(input) {
                            round_cost(catalog::numeric(input)? * 0.1)
                        } else {
                            0.0
                        })
                    } else {
                        cache.clone()
                    };
                    let mut cost = field(&model, "cost").clone();
                    set(&mut cost, "cacheRead", read);
                    set(&mut model, "cost", cost);
                }
                if key == "huggingface" {
                    insert_before(
                        &mut model,
                        "contextWindow",
                        literal(json!({"compat":{"supportsDeveloperRole":false}})),
                    );
                }
                self.record(key, id, m);
                models.push(model);
            }
        }
        if let JsValue::Array(fireworks) = process_fireworks_models(field(data, "fireworks-ai"))? {
            models.extend(fireworks);
        }
        for entry in tool_models(data, "fireworks-ai")? {
            let (id, m) = entry?;
            self.record("fireworks", id, m);
        }
        for entry in tool_models(data, "nvidia")? {
            let (id, m) = entry?;
            if !includes(field(field(m, "modalities"), "input"), "text")
                || !includes(field(field(m, "modalities"), "output"), "text")
            {
                continue;
            }
            let Some(live) = nvidia.get(id).or_else(|| nvidia.get(&normalized_nvidia(id))) else {
                continue;
            };
            if matches_id(live, NVIDIA_UNSUPPORTED) {
                continue;
            }
            let mut model = source_model(
                live,
                m,
                "nvidia",
                "openai-completions",
                "https://integrate.api.nvidia.com/v1",
            )?;
            insert_before(
                &mut model,
                "reasoning",
                literal(json!({"headers":{"NVCF-POLL-SECONDS":"3600"}})),
            );
            insert_before(&mut model, "contextWindow", object([("compat", base_openai_compat())]));
            self.record("nvidia", live, m);
            models.push(model);
        }
        let together = ["together", "togetherai", "together-ai"]
            .into_iter()
            .find(|key| !matches!(field(data, key), JsValue::Null));
        if let Some(key) = together {
            for entry in tool_models(data, key)? {
                let (id, m) = entry?;
                if equal(field(m, "status"), "deprecated") {
                    continue;
                }
                let reasoning = field(m, "reasoning") == &JsValue::Bool(true);
                let effort = matches_id(id, &["openai/gpt-oss-20b", "openai/gpt-oss-120b"]);
                let both = matches_id(id, &["deepseek-ai/DeepSeek-V4-Pro"]);
                let only = matches_id(id, &["deepseek-ai/DeepSeek-R1", "MiniMaxAI/MiniMax-M2.7"]);
                let mut model = source_model(id, m, "together", "openai-completions", "https://api.together.ai/v1")?;
                let mut compat = base_openai_compat();
                if reasoning {
                    let map = if effort {
                        json!({"off":null,"minimal":null})
                    } else if both {
                        json!({"minimal":null,"low":null,"medium":null,"high":"high","xhigh":null})
                    } else if only {
                        json!({"off":null,"minimal":null,"low":null,"medium":null})
                    } else {
                        json!({"minimal":null,"low":null,"medium":null})
                    };
                    insert_before(&mut model, "input", literal(json!({"thinkingLevelMap":map})));
                    if effort || both {
                        set(&mut compat, "supportsReasoningEffort", JsValue::Bool(true));
                    }
                    if effort {
                        set(&mut compat, "thinkingFormat", text("openai"));
                    } else if !only {
                        set(&mut compat, "thinkingFormat", text("together"));
                    }
                }
                insert_before(&mut model, "contextWindow", object([("compat", compat)]));
                self.record("together", id, m);
                models.push(model);
            }
        }
        if let JsValue::Array(baseten) = process_baseten_models(field(data, "baseten"))? {
            models.extend(baseten);
        }
        self.transform_opencode(data, &mut models)?;
        self.transform_subscriptions(data, &mut models)?;
        Ok(JsValue::Array(models))
    }
}

impl Generator<'_> {
    fn transform_opencode(&mut self, data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
        for (provider, path) in [
            ("opencode", "https://opencode.ai/zen"),
            ("opencode-go", "https://opencode.ai/zen/go"),
        ] {
            for entry in tool_models(data, provider)? {
                let (id, m) = entry?;
                if equal(field(m, "status"), "deprecated") {
                    continue;
                }
                let npm = field(field(m, "provider"), "npm");
                let mut api = "openai-completions";
                let mut base = format!("{path}/v1");
                let mut compat = object([]);
                if equal(npm, "@ai-sdk/openai") {
                    api = "openai-responses";
                    set(&mut compat, "sessionAffinityFormat", text("openai-nosession"));
                } else if equal(npm, "@ai-sdk/anthropic") {
                    api = "anthropic-messages";
                    base = path.to_owned();
                } else if equal(npm, "@ai-sdk/google") {
                    api = "google-generative-ai";
                } else if equal(npm, "@ai-sdk/alibaba") {
                    set(&mut compat, "cacheControlFormat", text("anthropic"));
                }
                if provider == "opencode" && matches_id(id, &["grok-build-0.1"]) {
                    set(&mut compat, "supportsReasoningEffort", JsValue::Bool(false));
                }
                if matches_id(id, &["kimi-k2.6"]) {
                    set(&mut compat, "thinkingFormat", text("deepseek"));
                    set(&mut compat, "supportsReasoningEffort", JsValue::Bool(false));
                }
                if provider == "opencode-go" {
                    if matches_id(id, &["minimax-m2.7", "qwen3.5-plus", "qwen3.6-plus"]) {
                        api = "openai-completions";
                        base = format!("{path}/v1");
                    }
                    if matches_id(id, &["qwen3.5-plus", "qwen3.6-plus"]) {
                        set(&mut compat, "thinkingFormat", text("qwen"));
                    }
                }
                if api == "openai-completions" {
                    set(&mut compat, "maxTokensField", text("max_tokens"));
                    if (provider == "opencode"
                        && matches_id(
                            id,
                            &[
                                "deepseek-v4-flash",
                                "deepseek-v4-pro",
                                "kimi-k2.5",
                                "kimi-k2.6",
                                "minimax-m2.7",
                            ],
                        ))
                        || (provider == "opencode-go" && matches_id(id, &["kimi-k2.6"]))
                    {
                        set(&mut compat, "supportsLongCacheRetention", JsValue::Bool(false));
                    }
                }
                let mut model = source_model(id, m, provider, api, &base)?;
                if matches!(&compat,JsValue::Object(map) if !map.is_empty()) {
                    insert_before(&mut model, "contextWindow", object([("compat", compat)]));
                }
                self.record(provider, id, m);
                models.push(model);
            }
        }
        Ok(())
    }
    fn transform_subscriptions(&mut self, data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
        for entry in tool_models(data, "github-copilot")? {
            let (id, m) = entry?;
            if equal(field(m, "status"), "deprecated") {
                continue;
            }
            let name = id.to_utf8_lossy();
            let claude = regex::Regex::new(r"^claude-(haiku|sonnet|opus)-[45]([.\-]|$)")
                .unwrap()
                .is_match(&name);
            let responses =
                name == "grok-4.5" || name.starts_with("gpt-5") || name.starts_with("oswe") || name.starts_with("mai-");
            let api = if claude {
                "anthropic-messages"
            } else if responses {
                "openai-responses"
            } else {
                "openai-completions"
            };
            let mut model = source_model(id, m, "github-copilot", api, "https://api.individual.githubcopilot.com")?;
            set(&mut model, "cost", get_models_dev_cost(field(m, "cost"))?);
            set(
                &mut model,
                "contextWindow",
                fallback(field(field(m, "limit"), "context"), number(128000)),
            );
            set(
                &mut model,
                "maxTokens",
                fallback(field(field(m, "limit"), "output"), number(8192)),
            );
            set(
                &mut model,
                "headers",
                literal(
                    json!({"User-Agent":"GitHubCopilotChat/0.35.0","Editor-Version":"vscode/1.107.0","Editor-Plugin-Version":"copilot-chat/0.35.0","Copilot-Integration-Id":"vscode-chat"}),
                ),
            );
            if claude && matches_id(id, &["claude-haiku-4.5", "claude-sonnet-4", "claude-sonnet-4.5"]) {
                set(
                    &mut model,
                    "compat",
                    literal(json!({"supportsEagerToolInputStreaming":false})),
                );
            }
            if api == "openai-completions" {
                set(
                    &mut model,
                    "compat",
                    literal(
                        json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false}),
                    ),
                );
            }
            self.record("github-copilot", id, m);
            models.push(model);
        }
        for (provider, url) in [
            ("minimax", "https://api.minimax.io/anthropic"),
            ("minimax-cn", "https://api.minimaxi.com/anthropic"),
        ] {
            for entry in tool_models(data, provider)? {
                let (id, m) = entry?;
                models.push(source_model(id, m, provider, "anthropic-messages", url)?);
                self.record(provider, id, m);
            }
        }
        let kimi = field(field(data, "kimi-for-coding"), "models");
        let canonical = matches!(kimi,JsValue::Object(map) if map.contains_key(&JsString::from("kimi-for-coding")));
        for entry in tool_models(data, "kimi-for-coding")? {
            let (id, m) = entry?;
            let alias = matches_id(id, &["k2p5", "k2p6", "k2p7"]);
            if alias && canonical {
                continue;
            }
            let id = if alias {
                JsString::from("kimi-for-coding")
            } else {
                id.clone()
            };
            let k3 = matches_id(&id, &["k3"]);
            let mut model = source_model(
                &id,
                m,
                "kimi-coding",
                "anthropic-messages",
                "https://api.kimi.com/coding",
            )?;
            if alias {
                set(&mut model, "name", text("Kimi For Coding"));
            }
            let mut compat = object([]);
            if k3 || matches_id(&id, &["kimi-for-coding"]) {
                set(&mut compat, "allowEmptySignature", JsValue::Bool(true));
            }
            set(&mut compat, "forceAdaptiveThinking", JsValue::Bool(true));
            insert_before(
                &mut model,
                "reasoning",
                object([
                    ("headers", literal(json!({"User-Agent":"KimiCLI/1.5"}))),
                    ("compat", compat),
                ]),
            );
            if k3 {
                set(&mut model, "reasoning", JsValue::Bool(true));
            }
            let implied = if k3 {
                [3.0, 15.0, 0.3, 0.0]
            } else if matches_id(&id, &["kimi-for-coding"]) {
                [0.95, 4.0, 0.19, 0.0]
            } else if matches_id(&id, &["kimi-for-coding-highspeed"]) {
                [1.9, 8.0, 0.38, 0.0]
            } else if matches_id(&id, &["kimi-k2-thinking"]) {
                [0.6, 2.5, 0.15, 0.0]
            } else {
                [0.0; 4]
            };
            set(&mut model, "cost", implied_cost(field(m, "cost"), implied));
            self.record("kimi-coding", &id, m);
            models.push(model);
        }
        for (provider, url) in [
            ("moonshotai", "https://api.moonshot.ai/v1"),
            ("moonshotai-cn", "https://api.moonshot.cn/v1"),
        ] {
            for entry in tool_models(data, provider)? {
                let (id, m) = entry?;
                let k3 = matches_id(id, &["kimi-k3"]);
                let mut model = source_model(id, m, provider, "openai-completions", url)?;
                let mut compat = literal(
                    json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens","supportsStrictMode":false,"thinkingFormat":"deepseek"}),
                );
                if k3 {
                    set(
                        &mut compat,
                        "requiresReasoningContentOnAssistantMessages",
                        JsValue::Bool(true),
                    );
                    set(&mut compat, "deferredToolsMode", text("kimi"));
                    set(&mut compat, "thinkingFormat", text("openai"));
                    set(&mut compat, "supportsReasoningEffort", JsValue::Bool(true));
                    set(&mut model, "reasoning", JsValue::Bool(true));
                }
                set(
                    &mut model,
                    "cost",
                    implied_cost(field(m, "cost"), if k3 { [3.0, 15.0, 0.3, 0.0] } else { [0.0; 4] }),
                );
                set(&mut model, "compat", compat);
                self.record(provider, id, m);
                models.push(model);
            }
        }
        for (provider, url) in [
            ("xiaomi", "https://api.xiaomimimo.com/v1"),
            ("xiaomi-token-plan-cn", "https://token-plan-cn.xiaomimimo.com/v1"),
            ("xiaomi-token-plan-ams", "https://token-plan-ams.xiaomimimo.com/v1"),
            ("xiaomi-token-plan-sgp", "https://token-plan-sgp.xiaomimimo.com/v1"),
        ] {
            for entry in tool_models(data, provider)? {
                let (id, m) = entry?;
                let mut model = source_model(id, m, provider, "openai-completions", url)?;
                insert_before(
                    &mut model,
                    "reasoning",
                    literal(
                        json!({"compat":{"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"}}),
                    ),
                );
                self.record(provider, id, m);
                models.push(model);
            }
        }
        for (key, provider, url, individual) in [
            (
                "alibaba-token-plan",
                "qwen-token-plan",
                "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
                false,
            ),
            (
                "alibaba-token-plan",
                "qwen-token-plan-individual",
                "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
                true,
            ),
            (
                "alibaba-token-plan-cn",
                "qwen-token-plan-cn",
                "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
                false,
            ),
        ] {
            let mut emitted = vec![];
            for entry in tool_models(data, key)? {
                let (id, m) = entry?;
                if matches_id(id, &["qwen3.8-max-preview"]) || (individual && !matches_id(id, INDIVIDUAL_IDS)) {
                    continue;
                }
                let effort = !matches_id(
                    id,
                    &[
                        "MiniMax-M2.5",
                        "deepseek-v3.2",
                        "kimi-k2.5",
                        "kimi-k2.6",
                        "kimi-k2.7-code",
                        "qwen3.6-flash",
                        "qwen3.6-plus",
                        "qwen3.7-max",
                        "qwen3.7-plus",
                    ],
                );
                let mut model = source_model(id, m, provider, "openai-completions", url)?;
                let mut extra = literal(
                    json!({"compat":{"thinkingFormat":"qwen","supportsDeveloperRole":false,"supportsStore":false,"supportsReasoningEffort":effort}}),
                );
                if effort {
                    set(
                        &mut extra,
                        "thinkingLevelMap",
                        literal(if matches_id(id, &["qwen3.8-max"]) {
                            json!({"minimal":null,"low":"low","medium":"medium","high":null,"xhigh":"xhigh","max":null})
                        } else {
                            json!({"minimal":null,"low":null,"medium":null,"high":"high","xhigh":null,"max":"max"})
                        }),
                    );
                }
                insert_before(&mut model, "reasoning", extra);
                self.record(provider, id, m);
                emitted.push(id.clone());
                models.push(model);
            }
            if individual && self.options.strict {
                crate::model_data_tools::assert_exact_model_ids(
                    &JsString::from(provider),
                    INDIVIDUAL_IDS.iter().map(|id| JsString::from(*id)),
                    emitted,
                )
                .map_err(|error| GeneratorError::new("Error", error.0))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/support/oracle.rs"]
mod normalization_oracle;

#[cfg(test)]
mod normalization_tests {
    use super::*;
    use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
    #[test]
    fn normalize_matches_pinned_source_for_every_utf16_unit_and_context_case() {
        let mut source = normalization_oracle::TsOracle::spawn(
            "model-catalog-generator-oracle.ts",
            "packages/ai/scripts/generate-models.ts",
        );
        for id in [
            JsString::from_utf16((0..=u16::MAX).collect()),
            JsString::from_utf16(
                (0..=0x10ffff)
                    .filter_map(char::from_u32)
                    .flat_map(|character| {
                        let mut units = [0; 2];
                        character.encode_utf16(&mut units).to_vec()
                    })
                    .collect(),
            ),
            JsString::from("ΟΣ_Σ_İ_𐐀_𞤀"),
            JsString::from_utf16(vec![0xd800, 0x41, 0x5f, 0xd801, 0x42, 0xdc00]),
        ] {
            let input = stringify_lossless(&JsValue::String(id.clone()));
            let response =
                source.request(&json!({"operation":"normalizeNvidiaModelId","argsJson":format!("[{input}]")}));
            let JsValue::String(expected) = parse_lossless(
                response["json"]
                    .as_str()
                    .unwrap_or_else(|| panic!("source normalization failed: {response}")),
            )
            .unwrap() else {
                panic!("source returned a non-string");
            };
            let actual = normalized_nvidia(&id);
            assert_eq!(
                actual.units().len(),
                expected.units().len(),
                "normalization output length differs"
            );
            for (index, (actual, expected)) in actual.units().iter().zip(expected.units()).enumerate() {
                assert_eq!(actual, expected, "normalized UTF-16 output unit {index}");
            }
        }
    }
}

fn implied_cost(cost: &JsValue, implied: [f64; 4]) -> JsValue {
    object([
        ("input", fallback(field(cost, "input"), JsValue::Number(implied[0]))),
        ("output", fallback(field(cost, "output"), JsValue::Number(implied[1]))),
        (
            "cacheRead",
            fallback(field(cost, "cache_read"), JsValue::Number(implied[2])),
        ),
        (
            "cacheWrite",
            fallback(field(cost, "cache_write"), JsValue::Number(implied[3])),
        ),
    ])
}
