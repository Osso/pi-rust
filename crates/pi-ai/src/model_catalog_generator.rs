//! Frozen catalog maintenance workflow over lossless provider JSON.
use pi_js_compat::{
    abort::AbortReason,
    js_json::{JsString, JsValue, is_truthy, object_entries_lossless},
    node_error::NativeNodeError,
};
use serde_json::json;

mod artifacts;
mod catalog;
mod emission;
mod metadata;
mod models_dev;
mod values;
pub use artifacts::{CatalogFileSystem, NativeCatalogFileSystem};
pub use metadata::apply_model_metadata;
pub type GeneratorError = pi_js_compat::callback_error::CallbackError;
fn type_error(message: &str) -> GeneratorError {
    NativeNodeError::new("TypeError", message).into()
}
fn named_error(name: &str, message: impl Into<JsString>) -> GeneratorError {
    NativeNodeError::new(name, message).into()
}

/// Console arguments keep Error branding independently from JSON lookalikes.
pub struct ConsoleLog {
    pub is_error: bool,
    pub args: Vec<AbortReason>,
}
impl ConsoleLog {
    pub fn render(&self) -> JsString {
        let args = self
            .args
            .iter()
            .map(|argument| match argument {
                AbortReason::Value(JsValue::String(value)) => value.clone(),
                AbortReason::Value(value) => pi_js_compat::node_fs::inspect_console_value(value),
                AbortReason::Error(error) | AbortReason::DomException(error) => {
                    let mut output = error.string_value();
                    let mut properties = error.value().as_object().expect("native error object").clone();
                    properties.shift_remove(&JsString::from("name"));
                    properties.shift_remove(&JsString::from("message"));
                    if !properties.is_empty() {
                        output.push_str(" ");
                        output.push_str(pi_js_compat::node_fs::inspect_console_value(&JsValue::Object(
                            properties,
                        )));
                    }
                    output
                }
            })
            .collect::<Vec<_>>();
        JsString::join(args, " ")
    }
}
pub fn run_generator<'a>(
    root: &std::path::Path,
    fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>,
    emit: &'a mut dyn FnMut(&ConsoleLog),
) -> Result<(), GeneratorError> {
    let mut generator = Generator::new(fetch);
    generator.log_sink = Some(emit);
    generator.generate(root).map(|_| ())
}
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
    JsValue::Number(value as f64)
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
        JsValue::String(value) => value.contains(needle),
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
        _ => Err(type_error("model.id.startsWith is not a function")),
    }
}
fn equal(value: &JsValue, expected: &str) -> bool {
    value == &text(expected)
}
fn plain_cost(cost: &JsValue) -> JsValue {
    object([
        ("input", fallback(field(cost, "input"), number(0))),
        ("output", fallback(field(cost, "output"), number(0))),
        ("cacheRead", fallback(field(cost, "cache_read"), number(0))),
        ("cacheWrite", fallback(field(cost, "cache_write"), number(0))),
    ])
}
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
/// All four frozen fetchers consume the body even for unsuccessful HTTP status.
pub async fn fetch_native(url: &str) -> Result<HttpResponse, GeneratorError> {
    let response = crate::http::default_fetch()
        .fetch(
            crate::request::HttpRequest {
                method: "GET".into(),
                url: url.into(),
                headers: vec![],
                body: None,
            },
            None,
        )
        .await?;
    let status = response.status;
    let body = crate::request::read_body_text(response.body).await?;
    Ok(HttpResponse { status, body })
}
pub struct Generator<'a> {
    pub logs: Vec<JsValue>,
    log_sink: Option<&'a mut dyn FnMut(&ConsoleLog)>,
    total_models: usize,
    reasoning_models: usize,
    provider_counts: indexmap::IndexMap<JsString, usize>,
    fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>,
}
impl<'a> Generator<'a> {
    pub fn new(fetch: &'a mut dyn FnMut(&str) -> Result<HttpResponse, GeneratorError>) -> Self {
        Self {
            fetch,
            logs: vec![],
            log_sink: None,
            total_models: 0,
            reasoning_models: 0,
            provider_counts: indexmap::IndexMap::new(),
        }
    }
    fn emit(&mut self, record: ConsoleLog) {
        let level = text(if record.is_error { "error" } else { "log" });
        let snapshot = JsValue::Array(
            std::iter::once(level)
                .chain(record.args.iter().map(AbortReason::value))
                .collect(),
        );
        if let Some(emit) = self.log_sink.as_mut() {
            emit(&record);
        }
        self.logs.push(snapshot);
    }
    fn log(&mut self, level: &str, message: &str) {
        self.emit(ConsoleLog {
            is_error: level == "error",
            args: vec![text(message).into()],
        });
    }
    fn log_error(&mut self, message: &str, error: &GeneratorError) {
        self.emit(ConsoleLog {
            is_error: true,
            args: vec![text(message).into(), error.thrown_value()],
        });
    }
    fn fetch_json(&mut self, url: &str, _label: &str) -> Result<JsValue, GeneratorError> {
        let response = (self.fetch)(url)?;
        pi_js_compat::js_json::parse_lossless(&response.body)
            .map_err(|error| named_error("SyntaxError", error.to_string()))
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
                let count = models.as_array().expect("model array").len();
                self.log("log", &format!("Fetched {count} tool-capable models from {label}"));
                Ok(models)
            }
            Err(error) => {
                self.log_error(&format!("Failed to fetch {label} models:"), &error);
                Ok(JsValue::Array(vec![]))
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
        JsValue::Number(number) if gateway => *number,
        JsValue::Null if gateway => 0.0,
        value => {
            let string = pi_js_compat::js_json::to_js_string(value).map_err(type_error)?;
            pi_js_compat::number::parse_float_utf16(string.units())
        }
    };
    Ok(if gateway && !numeric.is_finite() { 0.0 } else { numeric })
}
fn transform_http_models(data: &JsValue, gateway: bool) -> Result<JsValue, GeneratorError> {
    if matches!(data, JsValue::Null) {
        return Err(type_error("Cannot read properties of null (reading 'data')"));
    }
    let items = match field(data, "data") {
        JsValue::Array(items) => items.as_slice(),
        _ if gateway => &[],
        JsValue::String(_) => return Ok(JsValue::Array(vec![])),
        _ => return Err(type_error("data.data is not iterable")),
    };
    let mut models = Vec::new();
    for model in items {
        let tag_key = if gateway { "tags" } else { "supported_parameters" };
        if matches!(model, JsValue::Null) {
            return Err(type_error(&format!(
                "Cannot read properties of null (reading '{tag_key}')"
            )));
        }
        let mut tags = field(model, tag_key);
        if gateway && !matches!(tags, JsValue::Array(_)) {
            tags = &JsValue::Null;
        }
        if !gateway && !matches!(tags, JsValue::Array(_) | JsValue::String(_) | JsValue::Null) {
            return Err(type_error("model.supported_parameters?.includes is not a function"));
        }
        if !includes(tags, if gateway { "tool-use" } else { "tools" }) {
            continue;
        }
        let modality = field(field(model, "architecture"), "modality");
        if !gateway && !matches!(modality, JsValue::Null | JsValue::Array(_) | JsValue::String(_)) {
            return Err(type_error("model.architecture?.modality?.includes is not a function"));
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
            set(
                &mut cost,
                output,
                JsValue::Number(round_cost(
                    pricing_number(field(field(model, "pricing"), input), gateway)? * 1_000_000.0,
                )),
            );
        }
        let mut result = object([]);
        for key in ["id", "name"] {
            let value = field(model, key);
            if gateway && key == "name" {
                if is_truthy(value) {
                    set(&mut result, key, value.clone());
                } else if matches!(model,JsValue::Object(map) if map.contains_key(&JsString::from("id"))) {
                    set(&mut result, key, field(model, "id").clone());
                }
            } else if let JsValue::Object(map) = model
                && let Some(value) = map.get(&JsString::from(key))
            {
                set(&mut result, key, value.clone());
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
        set(
            &mut result,
            "contextWindow",
            fallback(
                field(model, if gateway { "context_window" } else { "context_length" }),
                number(4096),
            ),
        );
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
