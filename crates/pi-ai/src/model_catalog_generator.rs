//! Native catalog maintenance counterpart of `scripts/generate-models.ts`.
//! Provider payloads remain lossless JSON, not the runtime's narrower typed models.

use pi_js_compat::js_json::{JsString, JsValue, is_truthy, object_entries_lossless};
use serde_json::json;

use crate::models_dev_reasoning_options::{ModelsDevReasoningOption, get_effort_thinking_level_map};

pub fn run_generator_cli<'a>(
    args: &[String],
    root: &std::path::Path,
    date: &JsString,
    fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>,
    emit: &'a mut dyn FnMut(&JsValue),
) -> Result<(), GeneratorError> {
    let options = read_generator_options(args)?;
    let mut generator = Generator::new(options, fetch);
    generator.log_sink = Some(emit);
    generator.generate(root, date).map(|_| ())
}

mod artifacts;
pub use artifacts::{CatalogFileSystem, NativeCatalogFileSystem};
mod catalog;
mod emission_types;
mod metadata;
mod models_dev;
mod receiver_types;
pub use metadata::apply_model_metadata;

fn field<'a>(value: &'a JsValue, key: &str) -> &'a JsValue {
    match value {
        JsValue::Object(object) => object.get(&JsString::from(key)).unwrap_or(&JsValue::Null),
        _ => &JsValue::Null,
    }
}

fn fallback(value: &JsValue, default: JsValue) -> JsValue {
    if is_truthy(value) { value.clone() } else { default }
}
fn number(value: u64) -> JsValue {
    JsValue::from(&json!(value))
}
fn text(value: &str) -> JsValue {
    JsValue::String(JsString::from(value))
}
fn object(entries: impl IntoIterator<Item = (&'static str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (JsString::from(key), value))
            .collect(),
    )
}
fn literal(value: serde_json::Value) -> JsValue {
    JsValue::from(&value)
}
fn set(value: &mut JsValue, key: &str, item: JsValue) {
    if let JsValue::Object(map) = value {
        map.insert(JsString::from(key), item);
    }
}
fn includes(value: &JsValue, needle: &str) -> bool {
    match value {
        JsValue::Array(values) => values.contains(&text(needle)),
        JsValue::String(value) => value
            .units()
            .windows(needle.encode_utf16().count())
            .any(|part| part.iter().copied().eq(needle.encode_utf16())),
        _ => false,
    }
}
fn entries(value: &JsValue) -> Vec<(&JsString, &JsValue)> {
    match value {
        JsValue::Object(values) => object_entries_lossless(values),
        _ => vec![],
    }
}
fn require_model_id(model: &JsValue) -> Result<&JsString, GeneratorError> {
    match field(model, "id") {
        JsValue::String(value) => Ok(value),
        _ => Err(GeneratorError::type_error("Generated model.id must be a string")),
    }
}

fn equal(value: &JsValue, expected: &str) -> bool {
    value == &text(expected)
}

/// models.dev costs retain source values and only context-size tiers, in source order.
pub fn get_models_dev_cost(cost: &JsValue) -> Result<JsValue, GeneratorError> {
    receiver_types::optional_object(cost, "models.dev cost")?;
    let mut result = plain_cost(cost);
    let source_tiers = field(cost, "tiers");
    if !matches!(source_tiers, JsValue::Null | JsValue::Array(_)) {
        return Err(GeneratorError::type_error("models.dev cost.tiers must be an array"));
    }
    if let JsValue::Array(tiers) = source_tiers {
        let mut output = vec![];
        for tier in tiers {
            if !matches!(tier, JsValue::Object(_)) {
                return Err(GeneratorError::type_error(
                    "models.dev cost.tiers entries must be objects",
                ));
            }
            let context = field(tier, "tier");
            receiver_types::optional_object(context, "models.dev cost.tiers.tier")?;
            if !equal(field(context, "type"), "context") {
                continue;
            }
            let JsValue::Object(context) = context else {
                continue;
            };
            let Some(size) = context.get(&JsString::from("size")) else {
                continue;
            };
            let mut rates = object([("inputTokensAbove", size.clone())]);
            if let (JsValue::Object(rates), JsValue::Object(cost)) = (&mut rates, plain_cost(tier)) {
                rates.extend(cost);
            }
            output.push(rates);
        }
        if !output.is_empty() {
            set(&mut result, "tiers", JsValue::Array(output));
        }
    }
    Ok(result)
}

fn plain_cost(cost: &JsValue) -> JsValue {
    object([
        ("input", fallback(field(cost, "input"), number(0))),
        ("output", fallback(field(cost, "output"), number(0))),
        ("cacheRead", fallback(field(cost, "cache_read"), number(0))),
        ("cacheWrite", fallback(field(cost, "cache_write"), number(0))),
    ])
}

fn validate_modalities(model: &JsValue) -> Result<(), GeneratorError> {
    receiver_types::modalities(model, "input")
}

fn common_model(id: &JsString, model: &JsValue, provider: &str) -> Result<JsValue, GeneratorError> {
    receiver_types::model(model)?;
    receiver_types::optional_object(field(model, "cost"), "models.dev cost")?;
    receiver_types::optional_object(field(model, "limit"), "models.dev limit")?;
    let id = JsValue::String(id.clone());
    Ok(object([
        ("id", id.clone()),
        ("name", fallback(field(model, "name"), id)),
        ("provider", text(provider)),
        (
            "reasoning",
            JsValue::Bool(field(model, "reasoning") == &JsValue::Bool(true)),
        ),
        (
            "input",
            if includes(field(field(model, "modalities"), "input"), "image") {
                literal(json!(["text", "image"]))
            } else {
                literal(json!(["text"]))
            },
        ),
        ("cost", plain_cost(field(model, "cost"))),
        (
            "contextWindow",
            fallback(field(field(model, "limit"), "context"), number(4096)),
        ),
        (
            "maxTokens",
            fallback(field(field(model, "limit"), "output"), number(4096)),
        ),
    ]))
}

/// Baseten accepts non-tool models and excludes only deprecated entries.
pub fn process_baseten_models(provider: &JsValue) -> Result<JsValue, GeneratorError> {
    let mut models = Vec::new();
    for (id, model) in receiver_types::model_entries(provider)? {
        receiver_types::model(model)?;
        if equal(field(model, "status"), "deprecated") {
            continue;
        }
        let is_glm = ["zai-org/GLM-5.2", "zai-org/GLM-5.2-Fast"]
            .iter()
            .any(|name| id == &JsString::from(*name));
        let options = field(model, "reasoning_options");
        let has_option = |kind| -> Result<bool, GeneratorError> {
            for option in receiver_types::options(options)? {
                if equal(receiver_types::option_kind(option)?, kind) {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let toggle = has_option("toggle")? || is_glm;
        let effort = has_option("effort")? || is_glm;
        validate_modalities(model)?;
        let mut compat = literal(
            json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":effort,"supportsUsageInStreaming":true,"maxTokensField":"max_tokens","supportsStrictMode":true,"supportsLongCacheRetention":false}),
        );
        if effort {
            set(&mut compat, "thinkingFormat", text("openai"));
        }
        if toggle {
            set(&mut compat, "thinkingFormat", text("baseten"));
            set(
                &mut compat,
                "chatTemplateArgs",
                literal(json!({"enable_thinking":{"$var":"thinking.enabled"}})),
            );
        }
        let map = if is_glm {
            Some(literal(
                json!({"off":"none","minimal":null,"low":null,"medium":null,"high":"high","xhigh":null,"max":"max"}),
            ))
        } else if toggle {
            Some(literal(
                json!({"off":"off","minimal":null,"low":null,"medium":null,"high":"high","xhigh":null,"max":null}),
            ))
        } else {
            reasoning_map(options)?
        };
        // Preserve source insertion order, including its API/baseUrl placement.
        let common = common_model(id, model, "baseten")?;
        let mut result = object([
            ("id", field(&common, "id").clone()),
            ("name", field(&common, "name").clone()),
            ("api", text("openai-completions")),
            ("provider", text("baseten")),
            ("baseUrl", text("https://inference.baseten.co/v1")),
            ("reasoning", field(&common, "reasoning").clone()),
        ]);
        if let Some(map) = map {
            set(&mut result, "thinkingLevelMap", map);
        }
        for key in ["input", "cost"] {
            set(&mut result, key, field(&common, key).clone());
        }
        set(&mut result, "compat", compat);
        for key in ["contextWindow", "maxTokens"] {
            set(&mut result, key, field(&common, key).clone());
        }
        models.push(result);
    }
    Ok(JsValue::Array(models))
}

fn reasoning_map(options: &JsValue) -> Result<Option<JsValue>, GeneratorError> {
    let mut typed = vec![];
    for option in receiver_types::options(options)? {
        if !equal(receiver_types::option_kind(option)?, "effort") {
            continue;
        }
        let values = match option {
            JsValue::Object(option) => option.get(&JsString::from("values")),
            _ => unreachable!("checked reasoning object"),
        };
        let Some(values) = values else {
            continue;
        };
        let JsValue::Array(values) = values else {
            return Err(GeneratorError::type_error("models.dev effort values must be an array"));
        };
        let mut supported = vec![];
        for value in values {
            match value {
                JsValue::Null => supported.push(None),
                JsValue::String(value) => {
                    // Unknown strings do not select Pi efforts in the source Set-membership algorithm.
                    if let Some(value) = value
                        .to_utf8()
                        .ok()
                        .and_then(|value| serde_json::from_value(json!(value)).ok())
                    {
                        supported.push(Some(value));
                    }
                }
                _ => {
                    return Err(GeneratorError::type_error(
                        "models.dev effort values must contain strings or null",
                    ));
                }
            }
        }
        typed.push(ModelsDevReasoningOption::Effort { values: supported });
    }
    Ok(get_effort_thinking_level_map(&typed)
        .map(|map| literal(serde_json::to_value(map).expect("reasoning map serializes"))))
}

/// Fireworks uses Anthropic except GLM-5p2/Kimi-K3 completions families.
pub fn process_fireworks_models(provider: &JsValue) -> Result<JsValue, GeneratorError> {
    let mut models = Vec::new();
    for (id, model) in receiver_types::model_entries(provider)? {
        receiver_types::model(model)?;
        if field(model, "tool_call") != &JsValue::Bool(true) {
            continue;
        }
        validate_modalities(model)?;
        let mut result = common_model(id, model, "fireworks")?;
        let id = JsValue::String(id.clone());
        let completions = includes(&id, "glm-5p2") || includes(&id, "kimi-k3");
        set(
            &mut result,
            "api",
            text(if completions {
                "openai-completions"
            } else {
                "anthropic-messages"
            }),
        );
        set(
            &mut result,
            "baseUrl",
            text(if completions {
                "https://api.fireworks.ai/inference/v1"
            } else {
                "https://api.fireworks.ai/inference"
            }),
        );
        let mut compat = if completions {
            literal(
                json!({"supportsStore":false,"supportsDeveloperRole":false,"sendSessionAffinityHeaders":true,"supportsLongCacheRetention":false}),
            )
        } else {
            literal(
                json!({"sendSessionAffinityHeaders":true,"supportsEagerToolInputStreaming":false,"supportsCacheControlOnTools":false,"supportsLongCacheRetention":false}),
            )
        };
        if !includes(&id, "glm-5p2") && includes(&id, "kimi-k3") {
            set(
                &mut compat,
                "requiresReasoningContentOnAssistantMessages",
                JsValue::Bool(true),
            );
            set(&mut compat, "thinkingFormat", text("openai"));
            set(&mut compat, "deferredToolsMode", text("kimi"));
        }
        set(&mut result, "compat", compat);
        models.push(result);
    }
    Ok(JsValue::Array(models))
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorOptions {
    pub strict: bool,
    pub data_only: bool,
    pub json_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_output_dir: Option<std::path::PathBuf>,
    pub pretty: bool,
}
pub fn read_generator_options(args: &[String]) -> Result<GeneratorOptions, GeneratorError> {
    let mut options = GeneratorOptions::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--strict" => options.strict = true,
            "--data-only" => options.data_only = true,
            "--json-only" => options.json_only = true,
            "--pretty" => options.pretty = true,
            "--json-output" => {
                let value = args
                    .next()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| GeneratorError::error("--json-output requires a directory"))?;
                let path = std::path::PathBuf::from(value);
                let path = if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .map_err(|error| GeneratorError::error(&error.to_string()))?
                        .join(path)
                };
                let mut resolved = std::path::PathBuf::new();
                for component in path.components() {
                    match component {
                        std::path::Component::CurDir => {}
                        std::path::Component::ParentDir => {
                            resolved.pop();
                        }
                        component => resolved.push(component.as_os_str()),
                    }
                }
                options.json_output_dir = Some(resolved);
            }
            _ => return Err(GeneratorError::error(&format!("Unknown argument: {arg}"))),
        }
    }
    if options.json_only && options.json_output_dir.is_none() {
        return Err(GeneratorError::error("--json-only requires --json-output"));
    }
    if options.data_only && (options.json_only || options.json_output_dir.is_some()) {
        return Err(GeneratorError::error(
            "--data-only cannot be combined with JSON catalog output",
        ));
    }
    Ok(options)
}
#[derive(Debug, Clone)]
pub struct GeneratorError {
    pub name: String,
    pub message: JsString,
    pub details: Option<GeneratorErrorDetails>,
}
#[derive(Debug, Clone)]
pub enum GeneratorErrorDetails {
    Transport(String),
    FileSystem {
        record: pi_js_compat::node_fs::NodeFsError,
        destination: Option<String>,
    },
}
impl GeneratorError {
    pub fn new(name: &str, message: JsString) -> Self {
        Self {
            name: name.into(),
            message,
            details: None,
        }
    }
    fn error(message: &str) -> Self {
        Self::new("Error", JsString::from(message))
    }
    fn type_error(message: &str) -> Self {
        Self::new("TypeError", JsString::from(message))
    }
    pub fn console_message(&self) -> String {
        let mut output = self.to_string();
        match &self.details {
            Some(GeneratorErrorDetails::Transport(cause)) => output.push_str(&format!("\n  cause: {cause}")),
            Some(GeneratorErrorDetails::FileSystem { record, destination }) => {
                output.push_str(" {\n");
                if let Some(errno) = record.errno {
                    output.push_str(&format!("  errno: {errno},\n"));
                }
                output.push_str(&format!("  code: '{}',\n  syscall: '{}'", record.code, record.syscall));
                if let Some(path) = &record.path {
                    output.push_str(&format!(",\n  path: '{path}'"));
                }
                if let Some(destination) = destination {
                    output.push_str(&format!(",\n  dest: '{destination}'"));
                }
                output.push_str("\n}");
            }
            None => {}
        }
        output
    }
}
impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message.to_utf8_lossy())
    }
}
impl std::error::Error for GeneratorError {}
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
/// Native maintenance transport, using the shared fetch error identity and causes.
pub async fn fetch_native(url: &str) -> Result<HttpResponse, GeneratorError> {
    let response = crate::http::default_fetch()
        .fetch(
            crate::request::HttpRequest {
                method: "GET".to_owned(),
                url: url.to_owned(),
                headers: vec![],
                body: None,
            },
            None,
        )
        .await
        .map_err(transport_error)?;
    let status = response.status;
    // Source fetchers inspect status before consuming JSON; a broken error body must not mask it.
    let body = if (200..300).contains(&status) {
        crate::request::read_body_text(response.body)
            .await
            .map_err(transport_error)?
    } else {
        String::new()
    };
    Ok(HttpResponse { status, body })
}
fn transport_error(error: crate::request::CallbackError) -> GeneratorError {
    GeneratorError {
        name: error.name,
        message: JsString::from(error.message.as_str()),
        details: error.cause.map(GeneratorErrorDetails::Transport),
    }
}
pub struct Generator<'a> {
    pub options: GeneratorOptions,
    pub logs: Vec<JsValue>,
    log_sink: Option<&'a mut dyn FnMut(&JsValue)>,
    reasoning_options: indexmap::IndexMap<JsString, JsValue>,
    total_models: usize,
    reasoning_models: usize,
    provider_counts: indexmap::IndexMap<JsString, usize>,
    fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>,
}
impl<'a> Generator<'a> {
    pub fn new(
        options: GeneratorOptions,
        fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>,
    ) -> Self {
        Self {
            options,
            fetch,
            logs: vec![],
            log_sink: None,
            reasoning_options: indexmap::IndexMap::new(),
            total_models: 0,
            reasoning_models: 0,
            provider_counts: indexmap::IndexMap::new(),
        }
    }
    fn emit(&mut self, log: JsValue) {
        if let Some(emit) = self.log_sink.as_mut() {
            emit(&log);
        }
        self.logs.push(log);
    }
    fn log(&mut self, level: &str, message: &str) {
        self.emit(literal(json!([level, message])));
    }
    fn fetch_json(&mut self, url: &str, label: &str) -> Result<JsValue, GeneratorError> {
        let response = (self.fetch)(url)?;
        if !(200..300).contains(&response.status) {
            return Err(GeneratorError::error(&format!(
                "{label} API returned {}",
                response.status
            )));
        }
        pi_js_compat::js_json::parse_lossless(&response.body)
            .map_err(|error| GeneratorError::new("SyntaxError", JsString::from(error.to_string().as_str())))
    }
    fn fetch_models(&mut self, gateway: bool) -> Result<JsValue, GeneratorError> {
        let label = if gateway { "Vercel AI Gateway" } else { "OpenRouter" };
        let url = if gateway {
            "https://ai-gateway.vercel.sh/v1/models"
        } else {
            "https://openrouter.ai/api/v1/models"
        };
        self.log("log", &format!("Fetching models from {label} API..."));
        let result = self
            .fetch_json(url, label)
            .and_then(|data| transform_http_models(&data, gateway));
        match result {
            Ok(models) => {
                let count = match &models {
                    JsValue::Array(models) => models.len(),
                    _ => 0,
                };
                self.log("log", &format!("Fetched {count} tool-capable models from {label}"));
                Ok(models)
            }
            Err(error) => {
                self.emit(JsValue::Array(vec![
                    text("error"),
                    text(&format!("Failed to fetch {label} models:")),
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
    pub fn fetch_open_router_models(&mut self) -> Result<JsValue, GeneratorError> {
        self.fetch_models(false)
    }
    pub fn fetch_ai_gateway_models(&mut self) -> Result<JsValue, GeneratorError> {
        self.fetch_models(true)
    }
}

fn round_cost(value: f64) -> f64 {
    pi_js_compat::number::parse_number(&pi_js_compat::number::to_fixed(value, 6))
}
fn pricing_number(value: &JsValue, gateway: bool) -> Result<f64, GeneratorError> {
    let value = if gateway {
        value.clone()
    } else {
        fallback(value, text("0"))
    };
    let numeric = match &value {
        JsValue::Number(value) if gateway => *value,
        JsValue::Null if gateway => 0.0,
        value => {
            let value = pi_js_compat::js_json::to_js_string(value).map_err(GeneratorError::type_error)?;
            pi_js_compat::number::parse_float(&value.to_utf8_lossy())
        }
    };
    Ok(if gateway && !numeric.is_finite() { 0.0 } else { numeric })
}
fn transform_http_models(data: &JsValue, gateway: bool) -> Result<JsValue, GeneratorError> {
    if matches!(data, JsValue::Null) {
        return Err(GeneratorError::type_error(
            "Cannot read properties of null (reading 'data')",
        ));
    }
    let items = match field(data, "data") {
        JsValue::Array(items) => items.as_slice(),
        _ if gateway => &[],
        JsValue::String(_) => return Ok(JsValue::Array(vec![])),
        _ => return Err(GeneratorError::type_error("data.data is not iterable")),
    };
    let mut models = Vec::new();
    for model in items {
        let tag_key = if gateway { "tags" } else { "supported_parameters" };
        if matches!(model, JsValue::Null) {
            return Err(GeneratorError::type_error(&format!(
                "Cannot read properties of null (reading '{tag_key}')"
            )));
        }
        let mut tags = field(model, tag_key);
        if gateway && !matches!(tags, JsValue::Array(_)) {
            tags = &JsValue::Null;
        }
        if !gateway && !matches!(tags, JsValue::Array(_) | JsValue::String(_) | JsValue::Null) {
            return Err(GeneratorError::type_error(
                "model.supported_parameters?.includes is not a function",
            ));
        }
        if !includes(tags, if gateway { "tool-use" } else { "tools" }) {
            continue;
        }
        let modality = field(field(model, "architecture"), "modality");
        if !gateway && !matches!(modality, JsValue::Null | JsValue::Array(_) | JsValue::String(_)) {
            return Err(GeneratorError::type_error(
                "model.architecture?.modality?.includes is not a function",
            ));
        }
        let image = if gateway {
            includes(tags, "vision")
        } else {
            includes(modality, "image")
        };
        let mut cost = object([]);
        for (output, input) in [
            ("input", if gateway { "input" } else { "prompt" }),
            ("output", if gateway { "output" } else { "completion" }),
            ("cacheRead", "input_cache_read"),
            ("cacheWrite", "input_cache_write"),
        ] {
            let numeric = round_cost(pricing_number(field(field(model, "pricing"), input), gateway)? * 1_000_000.0);
            set(&mut cost, output, JsValue::Number(numeric));
        }
        let mut result = object([]);
        for key in ["id", "name"] {
            let value = field(model, key);
            if gateway && key == "name" {
                if is_truthy(value) {
                    set(&mut result, key, value.clone());
                } else if matches!(model,JsValue::Object(model) if model.contains_key(&JsString::from("id"))) {
                    set(&mut result, key, field(model, "id").clone());
                }
            } else if let JsValue::Object(object) = model {
                if object.contains_key(&JsString::from(key)) {
                    set(&mut result, key, value.clone());
                }
            }
        }
        set(
            &mut result,
            "api",
            text(if gateway {
                "anthropic-messages"
            } else {
                "openai-completions"
            }),
        );
        set(
            &mut result,
            "baseUrl",
            text(if gateway {
                "https://ai-gateway.vercel.sh"
            } else {
                "https://openrouter.ai/api/v1"
            }),
        );
        set(
            &mut result,
            "provider",
            text(if gateway { "vercel-ai-gateway" } else { "openrouter" }),
        );
        set(&mut result, "reasoning", JsValue::Bool(includes(tags, "reasoning")));
        set(
            &mut result,
            "input",
            literal(if image {
                json!(["text", "image"])
            } else {
                json!(["text"])
            }),
        );
        set(&mut result, "cost", cost);
        let context = if gateway {
            fallback(field(model, "context_window"), number(4096))
        } else {
            fallback(
                field(field(model, "top_provider"), "context_length"),
                fallback(field(model, "context_length"), number(4096)),
            )
        };
        set(&mut result, "contextWindow", context);
        set(
            &mut result,
            "maxTokens",
            fallback(
                if gateway {
                    field(model, "max_tokens")
                } else {
                    field(field(model, "top_provider"), "max_completion_tokens")
                },
                number(4096),
            ),
        );
        models.push(result);
    }
    Ok(JsValue::Array(models))
}
