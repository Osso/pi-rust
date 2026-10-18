//! Port of `packages/ai/src/providers/*.models.ts`.
//!
//! Every generated catalog module is the same two statements
//! (`flattenModelCatalog(provider, data/<provider>.json)`), so one Rust module
//! embeds all catalogs instead of 40 identical files.

use serde_json::Value;

use crate::types::Model;

/// Catalog modules in `models.generated.ts` order.
const PROVIDER_CATALOGS: &[(&str, &str)] = &[
    (
        "amazon-bedrock",
        include_str!("../../data/providers/amazon-bedrock.json"),
    ),
    ("ant-ling", include_str!("../../data/providers/ant-ling.json")),
    ("anthropic", include_str!("../../data/providers/anthropic.json")),
    (
        "azure-openai-responses",
        include_str!("../../data/providers/azure-openai-responses.json"),
    ),
    ("baseten", include_str!("../../data/providers/baseten.json")),
    ("cerebras", include_str!("../../data/providers/cerebras.json")),
    (
        "cloudflare-ai-gateway",
        include_str!("../../data/providers/cloudflare-ai-gateway.json"),
    ),
    (
        "cloudflare-workers-ai",
        include_str!("../../data/providers/cloudflare-workers-ai.json"),
    ),
    ("deepseek", include_str!("../../data/providers/deepseek.json")),
    ("fireworks", include_str!("../../data/providers/fireworks.json")),
    (
        "github-copilot",
        include_str!("../../data/providers/github-copilot.json"),
    ),
    ("google", include_str!("../../data/providers/google.json")),
    ("google-vertex", include_str!("../../data/providers/google-vertex.json")),
    ("groq", include_str!("../../data/providers/groq.json")),
    ("huggingface", include_str!("../../data/providers/huggingface.json")),
    ("kimi-coding", include_str!("../../data/providers/kimi-coding.json")),
    ("minimax", include_str!("../../data/providers/minimax.json")),
    ("minimax-cn", include_str!("../../data/providers/minimax-cn.json")),
    ("mistral", include_str!("../../data/providers/mistral.json")),
    ("moonshotai", include_str!("../../data/providers/moonshotai.json")),
    ("moonshotai-cn", include_str!("../../data/providers/moonshotai-cn.json")),
    ("nvidia", include_str!("../../data/providers/nvidia.json")),
    ("openai", include_str!("../../data/providers/openai.json")),
    ("openai-codex", include_str!("../../data/providers/openai-codex.json")),
    (
        "openai-codex-gc",
        include_str!("../../data/providers/openai-codex-gc.json"),
    ),
    ("opencode", include_str!("../../data/providers/opencode.json")),
    ("opencode-go", include_str!("../../data/providers/opencode-go.json")),
    ("openrouter", include_str!("../../data/providers/openrouter.json")),
    (
        "qwen-token-plan",
        include_str!("../../data/providers/qwen-token-plan.json"),
    ),
    (
        "qwen-token-plan-cn",
        include_str!("../../data/providers/qwen-token-plan-cn.json"),
    ),
    (
        "qwen-token-plan-individual",
        include_str!("../../data/providers/qwen-token-plan-individual.json"),
    ),
    ("together", include_str!("../../data/providers/together.json")),
    (
        "vercel-ai-gateway",
        include_str!("../../data/providers/vercel-ai-gateway.json"),
    ),
    ("xai", include_str!("../../data/providers/xai.json")),
    ("xiaomi", include_str!("../../data/providers/xiaomi.json")),
    (
        "xiaomi-token-plan-ams",
        include_str!("../../data/providers/xiaomi-token-plan-ams.json"),
    ),
    (
        "xiaomi-token-plan-cn",
        include_str!("../../data/providers/xiaomi-token-plan-cn.json"),
    ),
    (
        "xiaomi-token-plan-sgp",
        include_str!("../../data/providers/xiaomi-token-plan-sgp.json"),
    ),
    ("zai", include_str!("../../data/providers/zai.json")),
    ("zai-coding-cn", include_str!("../../data/providers/zai-coding-cn.json")),
];

const MODEL_DATA_MANIFEST: &str = include_str!("../../data/providers/.manifest.json");

/// `Object.keys(MODELS)`.
pub fn builtin_provider_ids() -> Vec<String> {
    PROVIDER_CATALOGS
        .iter()
        .map(|(provider, _)| (*provider).to_owned())
        .collect()
}

/// `Object.values(MODELS[provider])`, preserving generated insertion order.
pub fn provider_models(provider: &str) -> Option<Vec<Model>> {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == provider)?;
    Some(flatten_catalog(json))
}

/// `MODELS[provider][modelId]`.
pub fn builtin_model(provider: &str, model_id: &str) -> Option<Model> {
    provider_models(provider)?
        .into_iter()
        .find(|model| model.id == model_id)
}

/// `getBuiltinModelDataGeneratedAt()`.
pub fn model_data_generated_at() -> Option<f64> {
    let manifest: Value = serde_json::from_str(MODEL_DATA_MANIFEST).ok()?;
    let generated_at = manifest.get("generatedAt")?.as_str()?;
    let millis = pi_js_compat::date::date_parse(generated_at);
    (!millis.is_nan()).then_some(millis)
}

/// `flattenModelCatalog`: group values in file order, later duplicates replace
/// earlier ones without moving their position.
fn flatten_catalog(json: &str) -> Vec<Model> {
    let groups: serde_json::Map<String, Value> =
        serde_json::from_str(json).expect("embedded provider catalogs must be valid JSON");
    let mut catalog: Vec<(String, Model)> = Vec::new();
    for group in groups.into_values() {
        let Value::Object(models) = group else { continue };
        for (id, value) in models {
            let model: Model =
                serde_json::from_value(value).expect("embedded provider catalogs must match the model contract");
            match catalog.iter_mut().find(|(existing, _)| *existing == id) {
                Some(entry) => entry.1 = model,
                None => catalog.push((id, model)),
            }
        }
    }
    catalog.into_iter().map(|(_, model)| model).collect()
}
