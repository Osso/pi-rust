use super::*;
use indexmap::IndexMap;

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

fn matches_id(id: &JsString, choices: &[&str]) -> bool {
    choices.iter().any(|choice| id == &JsString::from(*choice))
}
fn starts(id: &JsString, prefix: &str) -> bool {
    id.units().starts_with(JsString::from(prefix).units())
}
fn normalized_nvidia(id: &JsString) -> JsString {
    JsString::from_utf16(
        id.to_lowercase()
            .units()
            .iter()
            .map(|unit| {
                if *unit == u16::from(b'_') {
                    u16::from(b'.')
                } else {
                    *unit
                }
            })
            .collect(),
    )
}
fn insert_before(model: &mut JsValue, key: &str, extra: JsValue) {
    let (JsValue::Object(model), JsValue::Object(extra)) = (model, extra) else {
        return;
    };
    let mut result = IndexMap::new();
    for (existing, value) in std::mem::take(model) {
        if existing == key {
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
    let expression = if matches!(provider, "google" | "google-vertex") {
        "source.modalities?.input?.includes"
    } else {
        "m.modalities?.input?.includes"
    };
    let image = values::image_input(source, expression)?;
    let id = JsValue::String(id.clone());
    Ok(object([
        ("id", id.clone()),
        ("name", fallback(field(source, "name"), id)),
        ("api", text(api)),
        ("provider", text(provider)),
        ("baseUrl", text(url)),
        (
            "reasoning",
            JsValue::Bool(field(source, "reasoning") == &JsValue::Bool(true)),
        ),
        (
            "input",
            literal(if image {
                json!(["text", "image"])
            } else {
                json!(["text"])
            }),
        ),
        ("cost", plain_cost(field(source, "cost"))),
        (
            "contextWindow",
            fallback(field(field(source, "limit"), "context"), number(4096)),
        ),
        (
            "maxTokens",
            fallback(field(field(source, "limit"), "output"), number(4096)),
        ),
    ]))
}
fn base_openai_compat() -> JsValue {
    literal(
        json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens","supportsStrictMode":false,"supportsLongCacheRetention":false}),
    )
}

fn non_iterable_models(value: &JsValue) -> GeneratorError {
    let receiver = match value {
        JsValue::Bool(value) => format!("boolean {value}"),
        JsValue::Number(value) => format!("number {}", pi_js_compat::number::js_number_to_string(*value)),
        _ => "object".into(),
    };
    type_error(&format!(
        "{receiver} is not iterable (cannot read property Symbol(Symbol.iterator))"
    ))
}

impl Generator<'_> {
    pub fn fetch_nvidia_nim_model_ids(&mut self) -> Result<IndexMap<JsString, JsString>, GeneratorError> {
        self.log("log", "Fetching models from NVIDIA NIM API...");
        let result = self
            .fetch_json("https://integrate.api.nvidia.com/v1/models", "NVIDIA NIM")
            .and_then(|data| {
                if matches!(data, JsValue::Null) {
                    return Err(type_error("Cannot read properties of null (reading 'data')"));
                }
                let items = match field(&data, "data") {
                    JsValue::Null => &[][..],
                    JsValue::Array(items) => items.as_slice(),
                    JsValue::String(value) if value.is_empty() => &[][..],
                    // Every iterated string element has no `id`; normalization
                    // fails on the first element, before the next iteration.
                    JsValue::String(_) => {
                        return Err(type_error(
                            "Cannot read properties of undefined (reading 'toLowerCase')",
                        ));
                    }
                    value => return Err(non_iterable_models(value)),
                };
                let mut ids = IndexMap::new();
                for model in items {
                    let id = match values::property(Some(model), "id")? {
                        Some(JsValue::String(id)) => id,
                        None => {
                            return Err(type_error(
                                "Cannot read properties of undefined (reading 'toLowerCase')",
                            ));
                        }
                        Some(JsValue::Null) => {
                            return Err(type_error("Cannot read properties of null (reading 'toLowerCase')"));
                        }
                        _ => return Err(type_error("modelId.toLowerCase is not a function")),
                    };
                    ids.insert(id.clone(), id.clone());
                    ids.insert(normalized_nvidia(&id), id);
                }
                self.log("log", &format!("Fetched {} model IDs from NVIDIA NIM", items.len()));
                Ok(ids)
            });
        match result {
            Ok(ids) => Ok(ids),
            Err(error) => {
                self.log_error("Failed to fetch NVIDIA NIM models:", &error);
                Ok(IndexMap::new())
            }
        }
    }
    pub fn load_models_dev_data(&mut self) -> Result<JsValue, GeneratorError> {
        self.log("log", "Fetching models from models.dev API...");
        let result = self
            .fetch_json("https://models.dev/api.json", "models.dev")
            .and_then(|data| {
                let nvidia = if values::provider_models(&data, "nvidia")?
                    .as_ref()
                    .is_some_and(is_truthy)
                {
                    self.fetch_nvidia_nim_model_ids()?
                } else {
                    IndexMap::new()
                };
                let models = transform_models_dev(&data, &nvidia)?;
                self.log(
                    "log",
                    &format!("Loaded {} tool-capable models from models.dev", models.len()),
                );
                Ok(JsValue::Array(models))
            });
        match result {
            Ok(models) => Ok(models),
            Err(error) => {
                self.log_error("Failed to load models.dev data:", &error);
                Ok(JsValue::Array(vec![]))
            }
        }
    }
}

fn transform_models_dev(data: &JsValue, nvidia: &IndexMap<JsString, JsString>) -> Result<Vec<JsValue>, GeneratorError> {
    let mut models = Vec::new();
    append_primary(data, &mut models)?;
    append_zai(data, &mut models)?;
    append_mistral_huggingface_fireworks(data, &mut models)?;
    append_nvidia(data, nvidia, &mut models)?;
    append_together(data, &mut models)?;
    append_opencode(data, &mut models)?;
    append_copilot(data, &mut models)?;
    append_minimax_kimi(data, &mut models)?;
    append_moonshot_xiaomi(data, &mut models)?;
    Ok(models)
}
fn append_primary(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (provider, api, url) in [
        ("amazon-bedrock", "bedrock-converse-stream", ""),
        ("anthropic", "anthropic-messages", "https://api.anthropic.com"),
        (
            "google",
            "google-generative-ai",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        (
            "google-vertex",
            "google-vertex",
            "https://{location}-aiplatform.googleapis.com",
        ),
        ("openai", "openai-responses", "https://api.openai.com/v1"),
        ("groq", "openai-completions", "https://api.groq.com/openai/v1"),
        ("cerebras", "openai-completions", "https://api.cerebras.ai/v1"),
        (
            "cloudflare-workers-ai",
            "openai-completions",
            crate::api::cloudflare::CLOUDFLARE_WORKERS_AI_BASE_URL,
        ),
        ("cloudflare-ai-gateway", "", ""),
        ("xai", "openai-completions", "https://api.x.ai/v1"),
    ] {
        for entry in values::tool_models(data, provider)? {
            let (owned_id, owned_model) = entry?;
            let (id, original) = (&owned_id, &owned_model);
            if provider == "amazon-bedrock"
                && (starts(id, "ai21.jamba") || starts(id, "mistral.mistral-7b-instruct-v0"))
            {
                continue;
            }
            if provider == "google-vertex" && (!starts(id, "gemini-") || id == "gemini-3.1-flash-lite-preview") {
                continue;
            }
            if provider == "cloudflare-ai-gateway" {
                if let Some(model) = gateway_model(id, original)? {
                    models.push(model);
                }
                continue;
            }
            let base = if provider == "amazon-bedrock" {
                if starts(id, "eu.") {
                    "https://bedrock-runtime.eu-central-1.amazonaws.com"
                } else {
                    "https://bedrock-runtime.us-east-1.amazonaws.com"
                }
            } else {
                url
            };
            let source = google_alias_source(data, provider, id, original);
            let mut model = source_model(id, source, provider, api, base)?;
            set(
                &mut model,
                "name",
                fallback(field(original, "name"), JsValue::String(id.clone())),
            );
            if matches!(provider, "google" | "google-vertex") && id.to_lowercase() == "gemini-3.7-flash" {
                insert_before(
                    &mut model,
                    "input",
                    literal(json!({"thinkingLevelMap":{"minimal":null}})),
                );
            }
            if provider == "google-vertex" {
                let mut cost = field(&model, "cost").clone();
                if id == "gemini-2.5-flash" {
                    set(&mut cost, "cacheRead", JsValue::Number(0.03));
                }
                set(&mut cost, "cacheWrite", number(0));
                set(&mut model, "cost", cost);
            }
            if provider == "cloudflare-workers-ai" {
                set(
                    &mut model,
                    "compat",
                    literal(json!({"sendSessionAffinityHeaders":true})),
                );
            }
            models.push(model);
        }
    }
    Ok(())
}
fn google_alias_source<'a>(data: &'a JsValue, provider: &str, id: &JsString, original: &'a JsValue) -> &'a JsValue {
    if !matches!(provider, "google" | "google-vertex") {
        return original;
    }
    let target = if id == "gemini-flash-latest" {
        "gemini-3.5-flash"
    } else if id == "gemini-flash-lite-latest" {
        "gemini-3.1-flash-lite"
    } else {
        return original;
    };
    let source = field(field(field(data, provider), "models"), target);
    if matches!(source, JsValue::Null) {
        original
    } else {
        source
    }
}
fn gateway_model(id: &JsString, source: &JsValue) -> Result<Option<JsValue>, GeneratorError> {
    let Some(slash) = id.units().iter().position(|unit| *unit == u16::from(b'/')) else {
        return Ok(None);
    };
    let upstream = JsString::from_utf16(id.units()[..slash].to_vec());
    let native = JsString::from_utf16(id.units()[slash + 1..].to_vec());
    let (model_id, api, base) = if upstream == "openai" {
        (
            native,
            "openai-responses",
            crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL,
        )
    } else if upstream == "anthropic" {
        (
            native,
            "anthropic-messages",
            crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL,
        )
    } else if upstream == "workers-ai" {
        (
            id.clone(),
            "openai-completions",
            crate::api::cloudflare::CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL,
        )
    } else {
        return Ok(None);
    };
    let mut model = source_model(&model_id, source, "cloudflare-ai-gateway", api, base)?;
    if upstream != "openai" {
        set(
            &mut model,
            "compat",
            literal(json!({"sendSessionAffinityHeaders":true})),
        );
    }
    Ok(Some(model))
}
fn append_zai(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (provider, url) in [
        ("zai", "https://api.z.ai/api/coding/paas/v4"),
        ("zai-coding-cn", "https://open.bigmodel.cn/api/coding/paas/v4"),
    ] {
        for entry in values::tool_models(data, "zai-coding-plan")? {
            let (owned_id, owned_model) = entry?;
            let (id, source) = (&owned_id, &owned_model);
            let glm52 = id == "glm-5.2";
            let mut model = source_model(id, source, provider, "openai-completions", url)?;
            if glm52 {
                insert_before(
                    &mut model,
                    "input",
                    literal(
                        json!({"thinkingLevelMap":{"minimal":null,"low":"high","medium":"high","high":"high","xhigh":"max"}}),
                    ),
                );
            }
            let mut compat = literal(json!({"supportsDeveloperRole":false,"thinkingFormat":"zai"}));
            if glm52 {
                set(&mut compat, "supportsReasoningEffort", JsValue::Bool(true));
            }
            if !matches_id(id, &["glm-4.5", "glm-4.5-air", "glm-4.5-flash", "glm-4.5v"]) {
                set(&mut compat, "zaiToolStream", JsValue::Bool(true));
            }
            insert_before(&mut model, "contextWindow", object([("compat", compat)]));
            models.push(model);
        }
    }
    Ok(())
}
fn append_mistral_huggingface_fireworks(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (key, provider, api, url) in [
        ("mistral", "mistral", "mistral-conversations", "https://api.mistral.ai"),
        (
            "huggingface",
            "huggingface",
            "openai-completions",
            "https://router.huggingface.co/v1",
        ),
        (
            "fireworks-ai",
            "fireworks",
            "anthropic-messages",
            "https://api.fireworks.ai/inference",
        ),
    ] {
        for entry in values::tool_models(data, key)? {
            let (owned_id, owned_model) = entry?;
            let (id, source) = (&owned_id, &owned_model);
            let mut model = source_model(id, source, provider, api, url)?;
            if key == "mistral" {
                let input = field(field(source, "cost"), "input");
                let cache = field(field(source, "cost"), "cache_read");
                let cache = if matches!(cache, JsValue::Null) {
                    let value = if is_truthy(input) {
                        round_cost(catalog::numeric(input)? * 0.1)
                    } else {
                        0.0
                    };
                    JsValue::Number(value)
                } else {
                    cache.clone()
                };
                let mut cost = field(&model, "cost").clone();
                set(&mut cost, "cacheRead", cache);
                set(&mut model, "cost", cost);
            } else if key == "huggingface" {
                insert_before(
                    &mut model,
                    "contextWindow",
                    literal(json!({"compat":{"supportsDeveloperRole":false}})),
                );
            } else {
                set(
                    &mut model,
                    "compat",
                    literal(
                        json!({"sendSessionAffinityHeaders":true,"supportsEagerToolInputStreaming":false,"supportsCacheControlOnTools":false,"supportsLongCacheRetention":false}),
                    ),
                );
            }
            models.push(model);
        }
    }
    Ok(())
}
fn append_nvidia(
    data: &JsValue,
    live_ids: &IndexMap<JsString, JsString>,
    models: &mut Vec<JsValue>,
) -> Result<(), GeneratorError> {
    for entry in values::tool_models(data, "nvidia")? {
        let (owned_id, owned_model) = entry?;
        let (id, source) = (&owned_id, &owned_model);
        let modalities = values::property(Some(source), "modalities")?;
        let input = values::optional_property(modalities.as_ref(), "input")?;
        if !values::optional_includes(input.as_ref(), "text", "m.modalities?.input?.includes")? {
            continue;
        }
        let output = values::optional_property(modalities.as_ref(), "output")?;
        if !values::optional_includes(output.as_ref(), "text", "m.modalities?.output?.includes")? {
            continue;
        }
        let Some(live) = live_ids.get(id).or_else(|| live_ids.get(&normalized_nvidia(id))) else {
            continue;
        };
        if matches_id(live, NVIDIA_UNSUPPORTED) {
            continue;
        }
        let mut model = source_model(
            live,
            source,
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
        models.push(model);
    }
    Ok(())
}
fn append_together(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    let Some(key) = ["together", "togetherai", "together-ai"]
        .into_iter()
        .find(|key| !matches!(field(data, key), JsValue::Null))
    else {
        return Ok(());
    };
    for entry in values::tool_models(data, key)? {
        let (owned_id, owned_model) = entry?;
        let (id, source) = (&owned_id, &owned_model);
        if equal(field(source, "status"), "deprecated") {
            continue;
        }
        let mut model = source_model(
            id,
            source,
            "together",
            "openai-completions",
            "https://api.together.ai/v1",
        )?;
        let mut compat = base_openai_compat();
        if field(source, "reasoning") == &JsValue::Bool(true) {
            let effort = matches_id(id, &["openai/gpt-oss-20b", "openai/gpt-oss-120b"]);
            let both = id == "deepseek-ai/DeepSeek-V4-Pro";
            let fixed = matches_id(id, &["deepseek-ai/DeepSeek-R1", "MiniMaxAI/MiniMax-M2.7"]);
            let map = if effort {
                json!({"off":null,"minimal":null})
            } else if both {
                json!({"minimal":null,"low":null,"medium":null,"high":"high","xhigh":null})
            } else if fixed {
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
            } else if !fixed {
                set(&mut compat, "thinkingFormat", text("together"));
            }
        }
        insert_before(&mut model, "contextWindow", object([("compat", compat)]));
        models.push(model);
    }
    Ok(())
}
fn append_opencode(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (provider, path) in [
        ("opencode", "https://opencode.ai/zen"),
        ("opencode-go", "https://opencode.ai/zen/go"),
    ] {
        for entry in values::tool_models(data, provider)? {
            let (owned_id, owned_model) = entry?;
            let (id, source) = (&owned_id, &owned_model);
            if equal(field(source, "status"), "deprecated") {
                continue;
            }
            let npm = field(field(source, "provider"), "npm");
            let mut api = "openai-completions";
            let mut base = format!("{path}/v1");
            let mut compat = object([]);
            if equal(npm, "@ai-sdk/openai") {
                api = "openai-responses";
            } else if equal(npm, "@ai-sdk/anthropic") {
                api = "anthropic-messages";
                base = path.to_owned();
            } else if equal(npm, "@ai-sdk/google") {
                api = "google-generative-ai";
            } else if equal(npm, "@ai-sdk/alibaba") {
                set(&mut compat, "cacheControlFormat", text("anthropic"));
            }
            if provider == "opencode" && id == "grok-build-0.1" {
                set(&mut compat, "supportsReasoningEffort", JsValue::Bool(false));
            }
            if id == "kimi-k2.6" {
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
                let unsupported = if provider == "opencode" {
                    matches_id(
                        id,
                        &[
                            "deepseek-v4-flash",
                            "deepseek-v4-pro",
                            "kimi-k2.5",
                            "kimi-k2.6",
                            "minimax-m2.7",
                        ],
                    )
                } else {
                    id == "kimi-k2.6"
                };
                if unsupported {
                    set(&mut compat, "supportsLongCacheRetention", JsValue::Bool(false));
                }
            }
            let mut model = source_model(id, source, provider, api, &base)?;
            if matches!(&compat,JsValue::Object(map) if !map.is_empty()) {
                insert_before(&mut model, "contextWindow", object([("compat", compat)]));
            }
            models.push(model);
        }
    }
    Ok(())
}
fn append_copilot(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    let claude_pattern = pi_js_compat::js_regex::JsRegex::new(r"^claude-(haiku|sonnet|opus)-[45]([.\-]|$)")
        .expect("frozen constant regexp");
    for entry in values::tool_models(data, "github-copilot")? {
        let (owned_id, owned_model) = entry?;
        let (id, source) = (&owned_id, &owned_model);
        if equal(field(source, "status"), "deprecated") {
            continue;
        }
        let claude = claude_pattern.is_match(id.clone());
        let api = if claude {
            "anthropic-messages"
        } else if starts(id, "gpt-5") || starts(id, "oswe") {
            "openai-responses"
        } else {
            "openai-completions"
        };
        let mut model = source_model(
            id,
            source,
            "github-copilot",
            api,
            "https://api.individual.githubcopilot.com",
        )?;
        set(
            &mut model,
            "contextWindow",
            fallback(field(field(source, "limit"), "context"), number(128000)),
        );
        set(
            &mut model,
            "maxTokens",
            fallback(field(field(source, "limit"), "output"), number(8192)),
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
                literal(json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false})),
            );
        }
        models.push(model);
    }
    Ok(())
}
fn append_minimax_kimi(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (provider, url) in [
        ("minimax", "https://api.minimax.io/anthropic"),
        ("minimax-cn", "https://api.minimaxi.com/anthropic"),
    ] {
        for entry in values::tool_models(data, provider)? {
            let (id, source) = entry?;
            models.push(source_model(&id, &source, provider, "anthropic-messages", url)?);
        }
    }
    let kimi = field(field(data, "kimi-for-coding"), "models");
    let canonical = matches!(kimi,JsValue::Object(map) if map.contains_key(&JsString::from("kimi-for-coding")));
    for entry in values::tool_models(data, "kimi-for-coding")? {
        let (owned_id, owned_model) = entry?;
        let (id, source) = (&owned_id, &owned_model);
        let alias = matches_id(id, &["k2p5", "k2p6"]);
        if alias && canonical {
            continue;
        }
        let model_id = if alias {
            JsString::from("kimi-for-coding")
        } else {
            id.clone()
        };
        let mut model = source_model(
            &model_id,
            source,
            "kimi-coding",
            "anthropic-messages",
            "https://api.kimi.com/coding",
        )?;
        if alias {
            set(&mut model, "name", text("Kimi For Coding"));
        }
        insert_before(
            &mut model,
            "reasoning",
            literal(json!({"headers":{"User-Agent":"KimiCLI/1.5"}})),
        );
        models.push(model);
    }
    Ok(())
}
fn append_moonshot_xiaomi(data: &JsValue, models: &mut Vec<JsValue>) -> Result<(), GeneratorError> {
    for (provider, url) in [
        ("moonshotai", "https://api.moonshot.ai/v1"),
        ("moonshotai-cn", "https://api.moonshot.cn/v1"),
    ] {
        for entry in values::tool_models(data, provider)? {
            let (owned_id, owned_model) = entry?;
            let (id, source) = (&owned_id, &owned_model);
            let mut model = source_model(id, source, provider, "openai-completions", url)?;
            set(
                &mut model,
                "compat",
                literal(
                    json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens","supportsStrictMode":false,"thinkingFormat":"deepseek"}),
                ),
            );
            models.push(model);
        }
    }
    for (provider, url) in [
        ("xiaomi", "https://api.xiaomimimo.com/v1"),
        ("xiaomi-token-plan-cn", "https://token-plan-cn.xiaomimimo.com/v1"),
        ("xiaomi-token-plan-ams", "https://token-plan-ams.xiaomimimo.com/v1"),
        ("xiaomi-token-plan-sgp", "https://token-plan-sgp.xiaomimimo.com/v1"),
    ] {
        for entry in values::tool_models(data, "xiaomi")? {
            let (owned_id, owned_model) = entry?;
            let (id, source) = (&owned_id, &owned_model);
            if starts(&JsString::from(provider), "xiaomi-token-plan-") && id == "mimo-v2-flash" {
                continue;
            }
            let mut model = source_model(id, source, provider, "openai-completions", url)?;
            insert_before(
                &mut model,
                "reasoning",
                literal(
                    json!({"compat":{"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"}}),
                ),
            );
            models.push(model);
        }
    }
    Ok(())
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
            "generator-repin-oracle.mts",
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
