//! Maintenance workflow for the pinned OpenRouter image-model generator.
//!
//! Raw JSON remains lossless through the native catalog boundary; this module deliberately does
//! not deserialize to `ImagesModel`, whose UTF-8 strings cannot represent lone surrogates.

use std::{fmt, io::Write, path::Path};

use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, is_truthy, to_js_string};
use pi_js_compat::number::parse_float;

use crate::request::{CallbackError, Fetch, HttpRequest, read_body_text};

pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorError {
    pub name: String,
    pub message: String,
    pub cause: Option<String>,
    filesystem_inspection: Option<String>,
}

impl GeneratorError {
    fn new(name: &str, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
            cause: None,
            filesystem_inspection: None,
        }
    }
    fn error(message: impl Into<String>) -> Self {
        Self::new("Error", message)
    }
    fn type_error(message: impl Into<String>) -> Self {
        Self::new("TypeError", message)
    }
}
impl From<CallbackError> for GeneratorError {
    fn from(error: CallbackError) -> Self {
        Self {
            name: error.name,
            message: error.message,
            cause: error.cause,
            filesystem_inspection: None,
        }
    }
}

impl fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)?;
        if let Some(fields) = &self.filesystem_inspection {
            f.write_str(fields)?;
        }
        if let Some(cause) = &self.cause {
            write!(f, "\n  [cause]: {cause}")?;
        }
        Ok(())
    }
}
impl std::error::Error for GeneratorError {}

fn property<'a>(value: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    match value {
        JsValue::Object(object) => object.get(&JsString::from(key)),
        _ => None,
    }
}
fn object(entries: impl IntoIterator<Item = (&'static str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (JsString::from(key), value))
            .collect(),
    )
}
fn text(value: &str) -> JsValue {
    JsValue::String(JsString::from(value))
}

/// `parseOpenRouterImageModels(payload, strict)`, retaining raw JSON code units and field values.
pub fn parse_openrouter_image_models(payload: &JsValue, strict: bool) -> Result<Vec<JsValue>, GeneratorError> {
    let Some(JsValue::Array(data)) =
        property(payload, "data").filter(|data| !matches!(data, JsValue::Array(items) if items.is_empty()))
    else {
        return if strict {
            Err(GeneratorError::error(
                "OpenRouter API returned a missing or empty image model list",
            ))
        } else {
            Ok(vec![])
        };
    };
    let mut models = Vec::new();
    let mut seen = Vec::new();
    for model in data {
        if !matches!(model, JsValue::Object(_)) {
            return Err(invalid_field("record", "object"));
        }
        let architecture = nullable_object_field(model, "architecture")?;
        let mut input = modalities(architecture, "input_modalities")?;
        let output = modalities(architecture, "output_modalities")?;
        if !output.contains(&text("image")) {
            continue;
        }
        let id = property(model, "id");
        if !matches!(id, Some(JsValue::String(_))) {
            return Err(invalid_field("id", "string"));
        }
        if seen.iter().any(|previous| same_id(*previous, id)) {
            continue;
        }
        seen.push(id);
        if input.is_empty() {
            input.push(text("text"));
        }
        if property(model, "name").is_some_and(|name| !matches!(name, JsValue::String(_))) {
            return Err(invalid_field("name", "string"));
        }
        let pricing = nullable_object_field(model, "pricing")?;
        let cost = object([
            ("input", price(pricing, "prompt")?),
            ("output", price(pricing, "completion")?),
            ("cacheRead", price(pricing, "input_cache_read")?),
            ("cacheWrite", price(pricing, "input_cache_write")?),
        ]);
        let mut fields = JsObject::new();
        // Undefined id/name fields exist in memory in JS but are omitted by JSON.stringify.
        for key in ["id", "name"] {
            if let Some(value) = property(model, key) {
                fields.insert(JsString::from(key), value.clone());
            }
        }
        for (key, value) in [
            ("api", text("openrouter-images")),
            ("provider", text("openrouter")),
            ("baseUrl", text(OPENROUTER_BASE_URL)),
            ("input", JsValue::Array(input)),
            ("output", JsValue::Array(output)),
            ("cost", cost),
        ] {
            fields.insert(JsString::from(key), value);
        }
        models.push(JsValue::Object(fields));
    }
    if strict && models.is_empty() {
        return Err(GeneratorError::error("OpenRouter API returned no usable image models"));
    }
    Ok(models)
}

fn invalid_field(field: &str, expected: &str) -> GeneratorError {
    GeneratorError::type_error(format!("Invalid OpenRouter image model {field}: expected {expected}"))
}

fn nullable_object_field<'a>(model: &'a JsValue, field: &str) -> Result<Option<&'a JsValue>, GeneratorError> {
    match property(model, field) {
        None | Some(JsValue::Null) => Ok(None),
        Some(value @ JsValue::Object(_)) => Ok(Some(value)),
        _ => Err(invalid_field(field, "object or null")),
    }
}

fn same_id(left: Option<&JsValue>, right: Option<&JsValue>) -> bool {
    // JSON arrays/objects have distinct identity even when structurally equal.
    !matches!(left, Some(JsValue::Array(_) | JsValue::Object(_))) && left == right
}
fn modalities(architecture: Option<&JsValue>, key: &str) -> Result<Vec<JsValue>, GeneratorError> {
    let Some(value) = architecture.and_then(|value| property(value, key)) else {
        return Ok(vec![]);
    };
    if matches!(value, JsValue::Null) {
        return Ok(vec![]);
    }
    let field = format!("architecture.{key}");
    let JsValue::Array(values) = value else {
        return Err(invalid_field(&field, "string array or null"));
    };
    if values.iter().any(|value| !matches!(value, JsValue::String(_))) {
        return Err(invalid_field(&field, "string array or null"));
    }
    let mut result = Vec::new();
    for value in values {
        if (value == &text("text") || value == &text("image")) && !result.contains(value) {
            result.push(value.clone());
        }
    }
    Ok(result)
}
fn price(pricing: Option<&JsValue>, key: &str) -> Result<JsValue, GeneratorError> {
    let value = pricing.and_then(|value| property(value, key));
    if value.is_some_and(|value| !matches!(value, JsValue::String(_))) {
        return Err(invalid_field(&format!("pricing.{key}"), "string"));
    }
    let value = value.filter(|value| is_truthy(value));
    let string = match value {
        Some(value) => to_js_string(value).map_err(GeneratorError::type_error)?,
        None => JsString::from("0"),
    };
    // Only the numeric prefix matters; replacing a surrogate terminator cannot change parseFloat.
    Ok(JsValue::Number(parse_float(&string.to_utf8_lossy()) * 1_000_000.0))
}

/// Accept zero or repeated `--strict` arguments, rejecting the first other argument.
pub fn read_strict_option(args: &[String]) -> Result<bool, GeneratorError> {
    for arg in args {
        if arg != "--strict" {
            return Err(GeneratorError::error(format!("Unknown argument: {arg}")));
        }
    }
    Ok(!args.is_empty())
}

/// Native artifact: JSON.stringify({openrouter: Object.fromEntries(sorted models)}, null, 2).
pub fn generate_image_models_file(models: &[JsValue]) -> Result<String, GeneratorError> {
    let mut models: Vec<_> = models.iter().collect();
    let mut error = None;
    models.sort_by(|left, right| match compare_model_ids(left, right) {
        Ok(order) => order,
        Err(failure) => {
            if error.is_none() {
                error = Some(failure);
            }
            std::cmp::Ordering::Equal
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    let mut entries = JsObject::new();
    for model in models {
        let key = match property(model, "id") {
            Some(id) => to_js_string(id).map_err(GeneratorError::type_error)?,
            None => JsString::from("undefined"),
        };
        // Source emits quoted keys into a TypeScript object literal. Its special __proto__
        // setter is not an own property after evaluation; preserve that classified source bug.
        if key != JsString::from("__proto__") {
            entries.insert(key, materialize_source_literal(model));
        }
    }
    Ok(js_json::stringify_lossless_pretty(&object([(
        "openrouter",
        JsValue::Object(entries),
    )])))
}
fn materialize_source_literal(value: &JsValue) -> JsValue {
    match value {
        JsValue::Array(items) => JsValue::Array(items.iter().map(materialize_source_literal).collect()),
        JsValue::Object(object) => JsValue::Object(
            object
                .iter()
                .filter(|(key, _)| **key != JsString::from("__proto__"))
                .map(|(key, value)| (key.clone(), materialize_source_literal(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn compare_model_ids(left: &JsValue, right: &JsValue) -> Result<std::cmp::Ordering, GeneratorError> {
    let left = match property(left, "id") {
        Some(JsValue::String(id)) => id,
        Some(JsValue::Null) => {
            return Err(GeneratorError::type_error(
                "Cannot read properties of null (reading 'localeCompare')",
            ));
        }
        None => {
            return Err(GeneratorError::type_error(
                "Cannot read properties of undefined (reading 'localeCompare')",
            ));
        }
        _ => return Err(GeneratorError::type_error("a.id.localeCompare is not a function")),
    };
    let right = match property(right, "id") {
        Some(id) => to_js_string(id).map_err(GeneratorError::type_error)?,
        None => JsString::from("undefined"),
    };
    Ok(pi_js_compat::collator::locale_compare_utf16(
        left.units(),
        right.units(),
    ))
}

/// Fetch with the runtime's replaceable transport, logging before/after parsing exactly as source.
pub async fn fetch_openrouter_image_models(
    strict: bool,
    fetch: &dyn Fetch,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Vec<JsValue>, GeneratorError> {
    writeln!(stdout, "Fetching image models from OpenRouter API...").map_err(io_error)?;
    let result = fetch_and_parse(strict, fetch).await;
    match result {
        Ok(models) => {
            writeln!(stdout, "Fetched {} image models from OpenRouter", models.len()).map_err(io_error)?;
            Ok(models)
        }
        Err(error) => {
            writeln!(stderr, "Failed to fetch OpenRouter image models: {error}").map_err(io_error)?;
            if strict { Err(error) } else { Ok(vec![]) }
        }
    }
}
async fn fetch_and_parse(strict: bool, fetch: &dyn Fetch) -> Result<Vec<JsValue>, GeneratorError> {
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".into(),
                url: format!("{OPENROUTER_BASE_URL}/models?output_modalities=image"),
                headers: vec![],
                body: None,
            },
            None,
        )
        .await
        .map_err(GeneratorError::from)?;
    if !(200..300).contains(&response.status) {
        return Err(GeneratorError::error(format!(
            "OpenRouter API returned {}",
            response.status
        )));
    }
    let body = read_body_text(response.body).await.map_err(GeneratorError::from)?;
    let payload = js_json::parse_lossless(body.strip_prefix('\u{feff}').unwrap_or(&body))
        .map_err(|error| GeneratorError::new("SyntaxError", error.to_string()))?;
    parse_openrouter_image_models(&payload, strict)
}
fn write_artifact(path: &Path, artifact: &[u8]) -> Result<(), GeneratorError> {
    let mut file = std::fs::File::create(path).map_err(|error| filesystem_error(error, "open", path))?;
    file.write_all(artifact)
        .map_err(|error| filesystem_error(error, "write", path))
}

fn filesystem_error(error: std::io::Error, syscall: &str, path: &Path) -> GeneratorError {
    let record = pi_js_compat::node_fs::node_fs_error_record(&error, syscall, &path.to_string_lossy());
    let mut fields = String::from(" {\n");
    if let Some(errno) = record.errno {
        fields.push_str(&format!("  errno: {errno},\n"));
    }
    fields.push_str(&format!("  code: '{}',\n  syscall: '{}'", record.code, record.syscall));
    if let Some(path) = record.path {
        fields.push_str(&format!(",\n  path: '{path}'"));
    }
    fields.push_str("\n}");
    GeneratorError {
        name: record.name.into(),
        message: record.message,
        cause: None,
        filesystem_inspection: Some(fields),
    }
}

fn io_error(error: std::io::Error) -> GeneratorError {
    GeneratorError::error(error.to_string())
}

/// Run strict options → fetch/log → sort → write. Output path is an explicit filesystem boundary,
/// not a CLI flag; the maintenance binary always selects the existing native catalog path.
pub async fn run(
    args: &[String],
    fetch: &dyn Fetch,
    output: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), GeneratorError> {
    let strict = read_strict_option(args)?;
    let models = fetch_openrouter_image_models(strict, fetch, stdout, stderr).await?;
    let artifact = generate_image_models_file(&models)?;
    write_artifact(output, artifact.as_bytes())?;
    writeln!(stdout, "Generated {}", output.display()).map_err(io_error)?;
    Ok(())
}
