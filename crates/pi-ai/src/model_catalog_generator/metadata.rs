//! Frozen generate-models.ts:223–599 metadata mutations; native UTF-16 throughout.
use super::*;

pub(super) fn merge(model: &mut JsValue, key: &str, patch: JsValue) {
    let mut value = match field(model, key) {
        JsValue::Object(map) => JsValue::Object(map.clone()),
        _ => object([]),
    };
    if let (JsValue::Object(value), JsValue::Object(patch)) = (&mut value, patch) {
        value.extend(patch);
    }
    set(model, key, value);
}

fn model_text(model: &JsValue, key: &str) -> Result<JsString, GeneratorError> {
    field(model, key)
        .as_string()
        .cloned()
        .ok_or_else(|| type_error(&format!("model.{key}.includes is not a function")))
}
fn one_of(value: &JsString, choices: &[&str]) -> bool {
    choices.iter().any(|choice| value == *choice)
}
fn contains_any(value: &JsString, choices: &[&str]) -> bool {
    choices.iter().any(|choice| value.contains(*choice))
}
fn adaptive(id: &JsString) -> bool {
    contains_any(
        id,
        &[
            "opus-4-6",
            "opus-4.6",
            "opus-4-7",
            "opus-4.7",
            "opus-4-8",
            "opus-4.8",
            "opus-5",
            "opus.5",
            "sonnet-4-6",
            "sonnet-4.6",
            "sonnet-5",
            "sonnet.5",
            "haiku-5",
            "haiku.5",
            "fable-5",
            "mythos-5",
        ],
    )
}
fn xhigh(id: &JsString) -> bool {
    contains_any(id, &["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5", "gpt-5.6", "gpt-6"])
}
fn temperature_unsupported(id: &JsString) -> bool {
    contains_any(
        &id.to_lowercase(),
        &[
            "opus-4-7",
            "opus-4.7",
            "opus-4-8",
            "opus-4.8",
            "opus-5",
            "opus.5",
            "sonnet-5-5",
            "sonnet-5.5",
            "haiku-5-5",
            "haiku-5.5",
        ],
    )
}
fn default_compat() -> JsValue {
    literal(
        json!({"supportsStore":true,"supportsDeveloperRole":true,"supportsReasoningEffort":true,"supportsUsageInStreaming":true,"maxTokensField":"max_completion_tokens","requiresToolResultName":false,"requiresAssistantAfterToolResult":false,"requiresThinkingAsText":false,"requiresReasoningContentOnAssistantMessages":false,"thinkingFormat":"openai","openRouterRouting":{},"vercelGatewayRouting":{},"chatTemplateKwargs":{},"zaiToolStream":false,"supportsStrictMode":true,"sendSessionAffinityHeaders":false,"supportsLongCacheRetention":true}),
    )
}

pub(super) fn detect_compat(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let provider = model_text(model, "provider")?;
    let base = model_text(model, "baseUrl")?;
    let id = model_text(model, "id")?;
    let zai = one_of(&provider, &["zai", "zai-coding-cn"]) || contains_any(&base, &["api.z.ai", "open.bigmodel.cn"]);
    let together = provider == "together" || contains_any(&base, &["api.together.ai", "api.together.xyz"]);
    let moonshot = one_of(&provider, &["moonshotai", "moonshotai-cn"]) || base.contains("api.moonshot.");
    let router = provider == "openrouter" || base.contains("openrouter.ai");
    let workers = provider == "cloudflare-workers-ai" || base.contains("api.cloudflare.com");
    let gateway = provider == "cloudflare-ai-gateway" || base.contains("gateway.ai.cloudflare.com");
    let nvidia = provider == "nvidia" || base.contains("integrate.api.nvidia.com");
    let ant = provider == "ant-ling" || base.contains("api.ant-ling.com");
    let reasoning_only = together && one_of(&id, &["deepseek-ai/DeepSeek-R1", "MiniMaxAI/MiniMax-M2.7"]);
    let grok = provider == "xai" || base.contains("api.x.ai");
    let deepseek = provider == "deepseek" || base.contains("deepseek.com");
    let nonstandard = nvidia
        || one_of(&provider, &["cerebras", "opencode"])
        || grok
        || together
        || base.contains("chutes.ai")
        || base.contains("deepseek.com")
        || zai
        || moonshot
        || workers
        || gateway
        || ant
        || contains_any(&base, &["cerebras.ai", "opencode.ai"]);
    let max_tokens = base.contains("chutes.ai") || moonshot || gateway || together || nvidia || ant;
    let mut compat = default_compat();
    for (key, value) in [
        ("supportsStore", !nonstandard),
        (
            "supportsDeveloperRole",
            (router && (id.starts_with("anthropic/") || id.starts_with("openai/"))) || (!nonstandard && !router),
        ),
        (
            "supportsReasoningEffort",
            !grok && !zai && !moonshot && !together && !gateway && !nvidia && !ant,
        ),
        ("requiresReasoningContentOnAssistantMessages", deepseek),
        ("supportsStrictMode", !moonshot && !together && !gateway && !nvidia),
        (
            "supportsLongCacheRetention",
            !(together || workers || gateway || nvidia || ant),
        ),
    ] {
        set(&mut compat, key, JsValue::Bool(value));
    }
    set(
        &mut compat,
        "maxTokensField",
        text(if max_tokens {
            "max_tokens"
        } else {
            "max_completion_tokens"
        }),
    );
    let format = if deepseek {
        "deepseek"
    } else if zai {
        "zai"
    } else if together && !reasoning_only {
        "together"
    } else if ant {
        "ant-ling"
    } else if router {
        "openrouter"
    } else {
        "openai"
    };
    set(&mut compat, "thinkingFormat", text(format));
    if provider == "openrouter" && id.starts_with("anthropic/") {
        let map = compat.as_object_mut().expect("compat is an object");
        let affinity = map.shift_remove(&JsString::from("sendSessionAffinityHeaders")).unwrap();
        let retention = map.shift_remove(&JsString::from("supportsLongCacheRetention")).unwrap();
        map.insert("cacheControlFormat".into(), text("anthropic"));
        map.insert("sendSessionAffinityHeaders".into(), affinity);
        map.insert("supportsLongCacheRetention".into(), retention);
    }
    Ok(compat)
}

fn apply_completions(model: &mut JsValue) -> Result<(), GeneratorError> {
    if !equal(field(model, "api"), "openai-completions") {
        return Ok(());
    }
    let detected = detect_compat(model)?;
    let defaults = default_compat();
    let mut delta = object([]);
    for (key, value) in entries(&detected) {
        if defaults.as_object().unwrap().get(key) == Some(value) {
            continue;
        }
        delta.as_object_mut().unwrap().insert(key.clone(), value.clone());
    }
    if let (JsValue::Object(delta), JsValue::Object(existing)) = (&mut delta, field(model, "compat")) {
        delta.extend(existing.clone());
    }
    if delta.as_object().unwrap().is_empty() {
        model.as_object_mut().unwrap().shift_remove(&JsString::from("compat"));
    } else {
        set(model, "compat", delta);
    }
    Ok(())
}

fn google_pattern(id: &JsString, pattern: &str) -> bool {
    pi_js_compat::js_regex::JsRegex::new(pattern)
        .expect("frozen constant regexp")
        .is_match(id.to_lowercase())
}

fn apply_api_thinking(model: &mut JsValue, id: &JsString, provider: &JsString, api: &JsString) {
    if api == "openai-responses" && id.starts_with("gpt-5") {
        merge(model, "thinkingLevelMap", literal(json!({"off":null})));
    }
    if provider == "github-copilot" && id.starts_with("gpt-5") {
        merge(model, "thinkingLevelMap", literal(json!({"minimal":"low"})));
    }
    if api == "openai-responses"
        && provider == "openai"
        && one_of(
            id,
            &[
                "gpt-5.1",
                "gpt-5.2",
                "gpt-5.3-codex",
                "gpt-5.4",
                "gpt-5.4-mini",
                "gpt-5.4-nano",
                "gpt-5.5",
            ],
        )
    {
        merge(model, "thinkingLevelMap", literal(json!({"off":"none"})));
    }
    if xhigh(id) {
        merge(model, "thinkingLevelMap", literal(json!({"xhigh":"xhigh"})));
    }
    if provider == "openai" && id == "gpt-5.5" {
        merge(model, "thinkingLevelMap", literal(json!({"minimal":null})));
    }
    if id.ends_with("gpt-5.5-pro") {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":null})),
        );
    }
    if contains_any(id, &["opus-4-6", "opus-4.6"]) {
        merge(model, "thinkingLevelMap", literal(json!({"xhigh":"max"})));
    }
    if contains_any(id, &["opus-4-7", "opus-4.7", "opus-4-8", "opus-4.8"]) {
        merge(model, "thinkingLevelMap", literal(json!({"xhigh":"xhigh"})));
    }
    if one_of(api, &["anthropic-messages", "bedrock-converse-stream"]) && id.contains("fable-5") {
        merge(model, "thinkingLevelMap", literal(json!({"off":null,"xhigh":"xhigh"})));
    }
    if api == "anthropic-messages" && adaptive(id) {
        merge(model, "compat", literal(json!({"forceAdaptiveThinking":true})));
    }
    if api == "anthropic-messages" && temperature_unsupported(id) {
        merge(model, "compat", literal(json!({"supportsTemperature":false})));
    }
    if api == "openai-completions" && id.contains("deepseek-v4") {
        let map = if provider == "openrouter" {
            json!({"minimal":null,"low":null,"medium":null,"high":"high","xhigh":"xhigh"})
        } else {
            json!({"minimal":null,"low":null,"medium":null,"high":"high","xhigh":"max"})
        };
        merge(model, "thinkingLevelMap", literal(map));
    }
}

fn apply_google_thinking(model: &mut JsValue, id: &JsString, api: &JsString) {
    if !one_of(api, &["google-generative-ai", "google-vertex"]) {
        return;
    }
    if google_pattern(id, r"gemini-3(?:\.\d+)?-pro") {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":"LOW","medium":null,"high":"HIGH"})),
        );
    }
    if google_pattern(id, r"gemini-3(?:\.\d+)?-flash")
        || one_of(&id.to_lowercase(), &["gemini-flash-latest", "gemini-flash-lite-latest"])
    {
        merge(model, "thinkingLevelMap", literal(json!({"off":null})));
    }
    if google_pattern(id, r"gemma-?4") {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":"MINIMAL","low":null,"medium":null,"high":"HIGH"})),
        );
    }
}

fn apply_provider_thinking(model: &mut JsValue, id: &JsString, provider: &JsString) {
    if provider == "groq" && id == "qwen/qwen3-32b" {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"minimal":null,"low":null,"medium":null,"high":"default"})),
        );
    }
    if one_of(provider, &["openai-codex", "openai-codex-gc", "openai-codex-team"]) {
        if xhigh(id) {
            merge(model, "thinkingLevelMap", literal(json!({"minimal":"low"})));
        }
        if id == "gpt-5.6-sol" {
            merge(model, "thinkingLevelMap", literal(json!({"max":"max","ultra":"max"})));
        }
        if id == "gpt-6-astra" {
            merge(model, "thinkingLevelMap", literal(json!({"off":null,"max":"max"})));
        }
    }
    if one_of(provider, &["moonshotai", "moonshotai-cn"]) && one_of(id, &["kimi-k2.7-code", "kimi-k2.7-code-highspeed"])
    {
        merge(model, "thinkingLevelMap", literal(json!({"off":null})));
    }
    if provider == "openrouter" && id.starts_with("inception/mercury-2") {
        merge(model, "thinkingLevelMap", literal(json!({"off":null})));
    }
    if provider == "openrouter" && id == "z-ai/glm-5.2" {
        merge(model, "thinkingLevelMap", literal(json!({"xhigh":"xhigh"})));
    }
    if provider == "fireworks" && id.contains("glm-5p2") {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":"none","minimal":null,"low":"high","medium":"high","xhigh":"max"})),
        );
    }
    if provider == "opencode-go" && id == "glm-5.2" {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":null,"medium":null,"high":"high","xhigh":"max"})),
        );
    }
    if provider == "opencode-go" && id == "kimi-k2.6" {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"minimal":null,"low":null,"medium":null})),
        );
    }
    if provider == "opencode" && id == "grok-build-0.1" {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":null,"medium":null})),
        );
    }
    if provider == "ant-ling" && is_truthy(field(model, "reasoning")) {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":null,"medium":null,"high":"high","xhigh":"xhigh"})),
        );
    }
    if provider == "github-copilot" {
        if one_of(id, &["claude-opus-4.7", "claude-opus-4.8"]) {
            merge(model, "thinkingLevelMap", literal(json!({"minimal":"low"})));
        }
        if id == "claude-sonnet-4.6" {
            merge(
                model,
                "thinkingLevelMap",
                literal(json!({"minimal":"low","xhigh":"max"})),
            );
        }
    }
}

fn thinking_model_id(model: &JsValue) -> Result<JsString, GeneratorError> {
    let first_starts_with =
        equal(field(model, "api"), "openai-responses") || equal(field(model, "provider"), "github-copilot");
    let first_method = if first_starts_with { "startsWith" } else { "includes" };
    let missing_receiver = match model.get("id") {
        Some(JsValue::String(id)) => return Ok(id.clone()),
        None => "undefined",
        Some(JsValue::Null) => "null",
        Some(JsValue::Array(_)) if !first_starts_with => {
            // Arrays implement includes, but not the next call at frozen480.
            return Err(type_error("model.id.endsWith is not a function"));
        }
        _ => {
            let expression = if first_starts_with {
                "model.id.startsWith"
            } else {
                "modelId.includes"
            };
            return Err(type_error(&format!("{expression} is not a function")));
        }
    };
    Err(type_error(&format!(
        "Cannot read properties of {missing_receiver} (reading '{first_method}')"
    )))
}

/// The frozen final metadata pass: thinking first, then completions compatibility.
pub fn apply_model_metadata(model: &mut JsValue) -> Result<(), GeneratorError> {
    let id = thinking_model_id(model)?;
    let provider = model_text(model, "provider")?;
    let api = model_text(model, "api")?;
    apply_api_thinking(model, &id, &provider, &api);
    apply_google_thinking(model, &id, &api);
    apply_provider_thinking(model, &id, &provider);
    apply_completions(model)
}
