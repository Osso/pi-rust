use super::*;

fn merge(model: &mut JsValue, key: &str, patch: JsValue) {
    let mut value = match field(model, key) {
        JsValue::Object(map) => JsValue::Object(map.clone()),
        _ => object([]),
    };
    if let (JsValue::Object(value), JsValue::Object(patch)) = (&mut value, patch) {
        value.extend(patch);
    }
    set(model, key, value);
}
fn id_text(model: &JsValue, key: &str) -> Result<String, GeneratorError> {
    match field(model, key) {
        JsValue::String(value) => Ok(value.to_utf8_lossy()),
        _ => Err(GeneratorError::type_error(&format!(
            "model.{key}.includes is not a function"
        ))),
    }
}
fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}
fn adaptive(id: &str) -> bool {
    has_any(
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
            "fable-5",
        ],
    )
}
fn xhigh(id: &str) -> bool {
    has_any(id, &["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5", "gpt-5.6"])
}

fn default_compat() -> JsValue {
    literal(
        json!({"supportsStore":true,"supportsDeveloperRole":true,"supportsReasoningEffort":true,"supportsUsageInStreaming":true,"supportsFinishReason":true,"maxTokensField":"max_completion_tokens","requiresToolResultName":false,"requiresAssistantAfterToolResult":false,"requiresThinkingAsText":false,"requiresReasoningContentOnAssistantMessages":false,"thinkingFormat":"openai","openRouterRouting":{},"vercelGatewayRouting":{},"chatTemplateKwargs":{},"chatTemplateArgs":{},"zaiToolStream":false,"supportsStrictMode":true,"supportsOpenAIGrammarTools":false,"sendSessionAffinityHeaders":false,"supportsLongCacheRetention":true}),
    )
}

pub(super) fn detect_compat(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let provider = id_text(model, "provider")?;
    let base = id_text(model, "baseUrl")?;
    let id = id_text(model, "id")?;
    let zai = matches!(provider.as_str(), "zai" | "zai-coding-cn") || has_any(&base, &["api.z.ai", "open.bigmodel.cn"]);
    let together = provider == "together" || has_any(&base, &["api.together.ai", "api.together.xyz"]);
    let moonshot = matches!(provider.as_str(), "moonshotai" | "moonshotai-cn") || base.contains("api.moonshot.");
    let router = provider == "openrouter" || base.contains("openrouter.ai");
    let workers = provider == "cloudflare-workers-ai" || base.contains("api.cloudflare.com");
    let gateway = provider == "cloudflare-ai-gateway" || base.contains("gateway.ai.cloudflare.com");
    let nvidia = provider == "nvidia" || base.contains("integrate.api.nvidia.com");
    let ant = provider == "ant-ling" || base.contains("api.ant-ling.com");
    let only = together && matches!(id.as_str(), "deepseek-ai/DeepSeek-R1" | "MiniMaxAI/MiniMax-M2.7");
    let deepseek = provider == "deepseek" || base.to_lowercase().contains("deepseek.com");
    let grok = provider == "xai" || base.contains("api.x.ai");
    let nonstandard = nvidia
        || matches!(provider.as_str(), "cerebras" | "opencode")
        || grok
        || together
        || deepseek
        || zai
        || moonshot
        || workers
        || gateway
        || ant
        || has_any(&base, &["cerebras.ai", "chutes.ai", "opencode.ai"]);
    let max_tokens = base.contains("chutes.ai") || deepseek || moonshot || gateway || together || nvidia || ant || zai;
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
    set(
        &mut compat,
        "thinkingFormat",
        text(if deepseek {
            "deepseek"
        } else if zai {
            "zai"
        } else if together && !only {
            "together"
        } else if ant {
            "ant-ling"
        } else if router {
            "openrouter"
        } else {
            "openai"
        }),
    );
    // The source places this optional property before the two final defaults.
    if provider == "openrouter" && (id.starts_with("anthropic/") || id.starts_with("~anthropic/")) {
        if let JsValue::Object(map) = &mut compat {
            let affinity = map.shift_remove(&JsString::from("sendSessionAffinityHeaders")).unwrap();
            let retention = map.shift_remove(&JsString::from("supportsLongCacheRetention")).unwrap();
            map.insert(JsString::from("cacheControlFormat"), text("anthropic"));
            map.insert(JsString::from("sendSessionAffinityHeaders"), affinity);
            map.insert(JsString::from("supportsLongCacheRetention"), retention);
        }
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
        let default = match &defaults {
            JsValue::Object(map) => map.get(key),
            _ => None,
        };
        if default == Some(value) {
            continue;
        }
        if let JsValue::Object(map) = &mut delta {
            map.insert(key.clone(), value.clone());
        }
    }
    if let (JsValue::Object(delta), JsValue::Object(existing)) = (&mut delta, field(model, "compat")) {
        delta.extend(existing.clone());
    }
    if matches!(&delta,JsValue::Object(map) if map.is_empty()) {
        if let JsValue::Object(map) = model {
            map.shift_remove(&JsString::from("compat"));
        }
    } else {
        set(model, "compat", delta);
    }
    Ok(())
}

fn thinking(model: &mut JsValue) -> Result<(), GeneratorError> {
    let id = id_text(model, "id")?;
    let api = id_text(model, "api")?;
    let provider = id_text(model, "provider")?;
    let mut apply = |patch: serde_json::Value| merge(model, "thinkingLevelMap", literal(patch));
    if matches!(api.as_str(), "openai-responses" | "azure-openai-responses") && id.starts_with("gpt-5") {
        apply(json!({"off":null}));
    }
    if provider == "github-copilot" && id.starts_with("gpt-5") {
        apply(json!({"minimal":"low"}));
    }
    if api == "openai-responses"
        && provider == "openai"
        && [
            "gpt-5.1",
            "gpt-5.2",
            "gpt-5.3-codex",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.4-nano",
            "gpt-5.5",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
        ]
        .contains(&id.as_str())
    {
        apply(json!({"off":"none"}));
    }
    if provider == "xai" && api == "openai-responses" && id == "grok-4.5" {
        apply(json!({"off":null,"minimal":null}));
    }
    if xhigh(&id) {
        apply(json!({"xhigh":"xhigh"}));
    }
    if id.contains("gpt-5.6")
        && matches!(
            api.as_str(),
            "openai-responses" | "azure-openai-responses" | "openai-codex-responses" | "openai-completions"
        )
    {
        apply(json!({"max":"max"}));
    }
    if provider == "openai" && id == "gpt-5.5" {
        apply(json!({"minimal":null}));
    }
    if id.ends_with("gpt-5.5-pro") {
        apply(json!({"off":null,"minimal":null,"low":null}));
    }
    if has_any(&id, &["opus-4-6", "opus-4.6", "sonnet-4-6", "sonnet-4.6"]) {
        apply(json!({"max":"max"}));
    }
    if has_any(
        &id,
        &[
            "opus-4-7", "opus-4.7", "opus-4-8", "opus-4.8", "opus-5", "opus.5", "sonnet-5", "sonnet.5",
        ],
    ) {
        apply(json!({"xhigh":"xhigh","max":"max"}));
    }
    if id.contains("fable-5") {
        apply(json!({"off":null,"xhigh":"xhigh","max":"max"}));
    }
    // Compatibility writes occur here in source order, before remaining thinking branches.
    if api == "anthropic-messages" && adaptive(&id) {
        merge(model, "compat", literal(json!({"forceAdaptiveThinking":true})));
    }
    if api == "anthropic-messages"
        && has_any(
            &id.to_lowercase(),
            &["opus-4-7", "opus-4.7", "opus-4-8", "opus-4.8", "opus-5", "opus.5"],
        )
    {
        merge(model, "compat", literal(json!({"supportsTemperature":false})));
    }
    let mut apply = |patch: serde_json::Value| merge(model, "thinkingLevelMap", literal(patch));
    if api == "openai-completions" && id.contains("deepseek-v4") {
        let mut map = json!({"minimal":null,"low":null,"medium":null,"high":"high","max":"max"});
        if provider == "openrouter" {
            map["xhigh"] = json!("xhigh");
            map["max"] = serde_json::Value::Null;
        } else if provider == "deepseek" && id == "deepseek-v4-flash" {
            map["low"] = json!("low");
        }
        apply(map);
    }
    let google = matches!(api.as_str(), "google-generative-ai" | "google-vertex");
    let lower = id.to_lowercase();
    if google && regex::Regex::new(r"gemini-3(?:\.\d+)?-pro").unwrap().is_match(&lower) {
        apply(json!({"off":null,"minimal":null,"low":"LOW","medium":null,"high":"HIGH"}));
    }
    if google
        && (regex::Regex::new(r"gemini-3(?:\.\d+)?-flash").unwrap().is_match(&lower)
            || matches!(lower.as_str(), "gemini-flash-latest" | "gemini-flash-lite-latest"))
    {
        apply(json!({"off":null}));
    }
    if google && regex::Regex::new(r"gemma-?4").unwrap().is_match(&lower) {
        apply(json!({"off":null,"minimal":"MINIMAL","low":null,"medium":null,"high":"HIGH"}));
    }
    if provider == "groq" && id == "qwen/qwen3.6-27b" {
        apply(json!({"minimal":null,"low":null,"medium":null,"high":"default"}));
    }
    if provider == "openai-codex" && xhigh(&id) {
        apply(json!({"minimal":"low"}));
    }
    let kimi = (provider == "kimi-coding" && id == "k3")
        || (matches!(provider.as_str(), "moonshotai" | "moonshotai-cn") && id == "kimi-k3")
        || (provider == "openrouter" && matches!(id.as_str(), "moonshotai/kimi-k3" | "~moonshotai/kimi-latest"))
        || (provider == "vercel-ai-gateway" && id == "moonshotai/kimi-k3");
    if kimi {
        apply(json!({"off":null,"minimal":null,"low":"low","medium":null,"high":"high","xhigh":null,"max":"max"}));
    }
    if matches!(provider.as_str(), "openai-codex" | "openai-codex-gc") && id == "gpt-5.6-sol" {
        apply(json!({"max":"max","ultra":"max"}));
    }
    if matches!(provider.as_str(), "moonshotai" | "moonshotai-cn")
        && matches!(id.as_str(), "kimi-k2.7-code" | "kimi-k2.7-code-highspeed")
    {
        apply(json!({"off":null}));
    }
    if provider == "openrouter" && id.starts_with("inception/mercury-2") {
        apply(json!({"off":null}));
    }
    if provider == "openrouter" && id == "z-ai/glm-5.2" {
        apply(json!({"xhigh":"xhigh"}));
    }
    if provider == "fireworks" && id.contains("glm-5p2") {
        apply(json!({"off":"none","minimal":null,"low":"high","medium":"high","max":"max"}));
    }
    if provider == "opencode-go" && id == "glm-5.2" {
        apply(json!({"off":null,"minimal":null,"low":null,"medium":null,"high":"high","max":"max"}));
    }
    if provider == "opencode-go" && id == "kimi-k2.6" {
        apply(json!({"minimal":null,"low":null,"medium":null}));
    }
    if provider == "opencode" && id == "grok-build-0.1" {
        apply(json!({"off":null,"minimal":null,"low":null,"medium":null}));
    }
    if provider == "ant-ling" && is_truthy(field(model, "reasoning")) {
        merge(
            model,
            "thinkingLevelMap",
            literal(json!({"off":null,"minimal":null,"low":null,"medium":null,"high":"high","xhigh":"xhigh"})),
        );
    }
    if provider == "github-copilot" {
        if matches!(id.as_str(), "claude-opus-4.7" | "claude-opus-4.8" | "claude-opus-5") {
            merge(model, "thinkingLevelMap", literal(json!({"minimal":"low"})));
        }
        if id == "claude-sonnet-4.6" {
            merge(model, "thinkingLevelMap", literal(json!({"minimal":"low","max":"max"})));
        }
    }
    Ok(())
}

/// Source metadata pass except deferred models.dev reasoning records, inserted by Generator.
pub fn apply_model_metadata(model: &mut JsValue) -> Result<(), GeneratorError> {
    apply_completions(model)?;
    apply_after_reasoning(model)
}

pub(super) fn apply_after_reasoning(model: &mut JsValue) -> Result<(), GeneratorError> {
    thinking(model)?;
    let id = id_text(model, "id")?;
    let api = id_text(model, "api")?;
    let provider = id_text(model, "provider")?;
    if matches!(provider.as_str(), "openai" | "cloudflare-ai-gateway") && api == "openai-responses" {
        merge(model, "compat", literal(json!({"supportsStrictMode":true})));
    } else if provider == "anthropic" && api == "anthropic-messages" {
        merge(model, "compat", literal(json!({"supportsStrictTools":true})));
    }
    let grammar_api = matches!(
        api.as_str(),
        "openai-responses" | "azure-openai-responses" | "openai-codex-responses"
    );
    let grammar_provider = matches!(
        provider.as_str(),
        "openai" | "openai-codex" | "azure-openai-responses" | "github-copilot" | "opencode" | "cloudflare-ai-gateway"
    );
    if grammar_api && grammar_provider {
        let digits = id
            .strip_prefix("gpt-")
            .unwrap_or("")
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if pi_js_compat::number::parse_number(&digits) >= 5.0 {
            merge(model, "compat", literal(json!({"supportsOpenAIGrammarTools":true})));
        }
    }
    let responses = provider == "openai" && api == "openai-responses";
    let codex = provider == "openai-codex" && api == "openai-codex-responses";
    if (responses || codex)
        && [
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.4-pro",
            "gpt-5.5",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
        ]
        .contains(&id.as_str())
    {
        if responses || matches!(id.as_str(), "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna") {
            merge(model, "compat", literal(json!({"supportsAdditionalTools":true})));
        }
        merge(model, "compat", literal(json!({"supportsToolSearch":true})));
    }
    if responses && matches!(field(field(model,"cost"),"cacheWrite"),JsValue::Number(number) if *number>0.0) {
        merge(
            model,
            "compat",
            literal(json!({"supportsExplicitPromptCacheMode":true})),
        );
    }
    Ok(())
}
