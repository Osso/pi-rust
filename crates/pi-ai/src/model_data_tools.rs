//! Offline maintenance APIs from `packages/ai/scripts/model-data.ts`.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

use indexmap::IndexMap;
use pi_js_compat::{date::date_parse, js_json};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const MODEL_DATA_SCHEMA_VERSION: u32 = 3;
pub const MODEL_DATA_MANIFEST_FILE: &str = ".manifest.json";
pub type ModelDataStructure = IndexMap<String, IndexMap<String, String>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDataManifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub structure_hash: String,
    pub files: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDataError(pub String);
impl fmt::Display for ModelDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ModelDataError {}
type Result<T> = std::result::Result<T, ModelDataError>;

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn sort_strings(values: &mut [String]) {
    values.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

fn sorted_record<T>(entries: impl IntoIterator<Item = (String, T)>) -> IndexMap<String, T> {
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
    entries.into_iter().collect()
}

// Reuse JavaScript's integer-index enumeration, including for typed records.
fn record_keys<T: Serialize>(record: &IndexMap<String, T>) -> Vec<String> {
    let value = serde_json::to_value(record).expect("string records serialize");
    js_json::object_entries(value.as_object().expect("record is an object"))
        .into_iter()
        .map(|(key, _)| key.clone())
        .collect()
}

fn difference(expected: &[String], actual: &[String]) -> String {
    let expected_set: HashSet<_> = expected.iter().collect();
    let actual_set: HashSet<_> = actual.iter().collect();
    let missing: Vec<_> = expected
        .iter()
        .filter(|key| !actual_set.contains(key))
        .cloned()
        .collect();
    let extra: Vec<_> = actual
        .iter()
        .filter(|key| !expected_set.contains(key))
        .cloned()
        .collect();
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("missing: {}", missing.join(", ")));
    }
    if !extra.is_empty() {
        parts.push(format!("extra: {}", extra.join(", ")));
    }
    parts.join("; ")
}

pub fn assert_exact_model_ids<S: AsRef<str>>(
    label: &str,
    expected: impl IntoIterator<Item = S>,
    actual: impl IntoIterator<Item = S>,
) -> Result<()> {
    let normalize = |items: Vec<String>| {
        let mut items = items;
        sort_strings(&mut items);
        items.dedup();
        items
    };
    let expected = normalize(expected.into_iter().map(|s| s.as_ref().to_owned()).collect());
    let actual = normalize(actual.into_iter().map(|s| s.as_ref().to_owned()).collect());
    if expected == actual {
        return Ok(());
    }
    Err(ModelDataError(format!(
        "{label} model IDs do not match ({})",
        difference(&expected, &actual)
    )))
}

fn fs_error(error: std::io::Error, operation: &str, path: &Path) -> ModelDataError {
    let (code, description) = match error.raw_os_error() {
        Some(2) => ("ENOENT", "no such file or directory"),
        Some(13) => ("EACCES", "permission denied"),
        Some(20) => ("ENOTDIR", "not a directory"),
        Some(21) => return ModelDataError("EISDIR: illegal operation on a directory, read".into()),
        Some(40) => ("ELOOP", "too many symbolic links encountered"),
        _ => return ModelDataError(format!("{error}, {operation} '{}'", path.display())),
    };
    ModelDataError(format!("{code}: {description}, {operation} '{}'", path.display()))
}
fn read_text(path: &Path) -> Result<String> {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| fs_error(e, "open", path))
}
fn matching_files(dir: &Path, suffix: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| fs_error(e, "scandir", dir))? {
        let entry = entry.map_err(|e| fs_error(e, "scandir", dir))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(suffix) && name != MODEL_DATA_MANIFEST_FILE {
            names.push(name);
        }
    }
    sort_strings(&mut names);
    Ok(names)
}
fn read_json_object(path: &Path, description: &str, errors: &mut Vec<String>) -> Option<Map<String, Value>> {
    let parsed = read_text(path).and_then(|s| js_json::parse(&s).map_err(|e| ModelDataError(e.to_string())));
    match parsed {
        Ok(Value::Object(object)) => Some(object),
        Ok(_) => {
            errors.push(format!("{description} must contain a JSON object"));
            None
        }
        Err(error) => {
            errors.push(format!("{description} is not valid JSON: {error}"));
            None
        }
    }
}

pub fn read_model_data_provider_ids(package_root: &Path) -> Result<Vec<String>> {
    let path = package_root.join("src/models.generated.ts");
    let text = read_text(&path)?;
    let pattern =
        regex::Regex::new(r#"(?m)^import \{ [A-Z][A-Z0-9_]*_MODELS \} from "\./providers/([^"/]+)\.models\.ts";$"#)
            .expect("static pattern");
    let mut ids: Vec<_> = pattern.captures_iter(&text).map(|c| c[1].to_owned()).collect();
    sort_strings(&mut ids);
    if ids.is_empty() {
        return Err(ModelDataError(format!(
            "No generated provider imports found in {}",
            path.display()
        )));
    }
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ModelDataError(format!(
            "Generated model aggregator contains duplicate provider imports: {}",
            path.display()
        )));
    }
    Ok(ids)
}

fn read_provider_structure(path: &Path, provider: &str) -> Result<IndexMap<String, String>> {
    let mut errors = Vec::new();
    let groups = read_json_object(path, &format!("{provider}.json"), &mut errors)
        .ok_or_else(|| ModelDataError(errors.join("\n")))?;
    let mut models = IndexMap::new();
    for (api, value) in js_json::object_entries(&groups) {
        let group = value.as_object().ok_or_else(|| {
            ModelDataError(format!(
                "{} API group {} must be an object",
                path.display(),
                json_string(Some(&Value::String(api.clone())))
            ))
        })?;
        for (id, _) in js_json::object_entries(group) {
            if models.contains_key(id) {
                return Err(ModelDataError(format!(
                    "{} contains model {id} in more than one API group",
                    path.display()
                )));
            }
            models.insert(id.clone(), api.clone());
        }
    }
    if models.is_empty() {
        return Err(ModelDataError(format!(
            "{} contains no generated model data",
            path.display()
        )));
    }
    Ok(sorted_record(models))
}

pub fn read_model_data_structure(package_root: &Path) -> Result<ModelDataStructure> {
    let providers = package_root.join("src/providers");
    let ids = read_model_data_provider_ids(package_root)?;
    let mut expected: Vec<_> = ids.iter().map(|id| format!("{id}.models.ts")).collect();
    sort_strings(&mut expected);
    let actual = matching_files(&providers, ".models.ts")?;
    if expected != actual {
        return Err(ModelDataError(format!(
            "Generated model aggregator and provider shards do not match ({})",
            difference(&expected, &actual)
        )));
    }
    let mut structure = IndexMap::new();
    for id in ids {
        let models = read_provider_structure(&providers.join("data").join(format!("{id}.json")), &id)?;
        structure.insert(id, models);
    }
    Ok(sorted_record(structure))
}

pub fn model_data_structure_hash(structure: &ModelDataStructure) -> String {
    let normalized = sorted_record(
        structure
            .iter()
            .map(|(id, models)| (id.clone(), sorted_record(models.clone()))),
    );
    sha256(&js_json::stringify(
        &serde_json::to_value(normalized).expect("structure serializes"),
    ))
}

pub fn create_model_data_manifest(
    structure: &ModelDataStructure,
    file_contents: &IndexMap<String, String>,
    generated_at: &str,
) -> ModelDataManifest {
    ModelDataManifest {
        schema_version: MODEL_DATA_SCHEMA_VERSION,
        generated_at: generated_at.to_owned(),
        structure_hash: model_data_structure_hash(structure),
        files: sorted_record(
            file_contents
                .iter()
                .map(|(file, content)| (file.clone(), sha256(content))),
        ),
    }
}

fn json_string(value: Option<&Value>) -> String {
    value.map(js_json::stringify).unwrap_or_else(|| "undefined".into())
}
fn validate_model(value: &Value, provider: &str, id: &str, api: &str, errors: &mut Vec<String>) {
    let label = format!("{provider}/{id}");
    let Some(model) = value.as_object() else {
        errors.push(format!("{label} must be an object"));
        return;
    };
    for (field, expected) in [("id", id), ("provider", provider), ("api", api)] {
        if model.get(field).and_then(Value::as_str) != Some(expected) {
            errors.push(format!(
                "{label} has {field} {}, expected {}",
                json_string(model.get(field)),
                js_json::stringify(&Value::String(expected.into()))
            ));
        }
    }
    if !model.get("name").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
        errors.push(format!("{label} has no model name"));
    }
    if !model.get("baseUrl").is_some_and(Value::is_string) {
        errors.push(format!("{label} has no baseUrl string"));
    }
    if !model.get("reasoning").is_some_and(Value::is_boolean) {
        errors.push(format!("{label} has no reasoning boolean"));
    }
    if !model
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty() && v.iter().all(|v| matches!(v.as_str(), Some("text" | "image"))))
    {
        errors.push(format!("{label} has invalid input modalities"));
    }
    for field in ["contextWindow", "maxTokens"] {
        if !model
            .get(field)
            .and_then(Value::as_f64)
            .is_some_and(|v| v.is_finite() && v > 0.0)
        {
            errors.push(format!("{label} has invalid {field}"));
        }
    }
    let Some(cost) = model.get("cost").and_then(Value::as_object) else {
        errors.push(format!("{label} has invalid cost metadata"));
        return;
    };
    for field in ["input", "output", "cacheRead", "cacheWrite"] {
        if !cost.get(field).and_then(Value::as_f64).is_some_and(f64::is_finite) {
            errors.push(format!("{label} has invalid cost.{field}"));
        }
    }
}

fn validate_manifest(
    structure: &ModelDataStructure,
    manifest: Option<&Map<String, Value>>,
    expected: &[String],
    errors: &mut Vec<String>,
) -> Option<Map<String, Value>> {
    let field = |name| manifest.and_then(|m| m.get(name));
    if field("schemaVersion").and_then(Value::as_f64) != Some(f64::from(MODEL_DATA_SCHEMA_VERSION)) {
        errors.push(format!(
            "model data schema is {}, expected {MODEL_DATA_SCHEMA_VERSION}",
            json_string(field("schemaVersion"))
        ));
    }
    if !field("generatedAt")
        .and_then(Value::as_str)
        .is_some_and(|s| !date_parse(s).is_nan())
    {
        errors.push("model data manifest has an invalid generation timestamp".into());
    }
    if field("structureHash").and_then(Value::as_str) != Some(model_data_structure_hash(structure).as_str()) {
        errors.push("model data generation stamp does not match the generated catalog".into());
    }
    let files = field("files").and_then(Value::as_object);
    let Some(files) = files else {
        errors.push("model data manifest has no file hashes".into());
        return None;
    };
    let mut actual: Vec<_> = files.keys().cloned().collect();
    sort_strings(&mut actual);
    if expected != actual {
        errors.push(format!(
            "manifest file hashes do not match provider data files ({})",
            difference(expected, &actual)
        ));
    }
    Some(files.clone())
}

fn validate_provider(
    path: &Path,
    provider: &str,
    expected: &IndexMap<String, String>,
    hashes: Option<&Map<String, Value>>,
    errors: &mut Vec<String>,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let filename = format!("{provider}.json");
    let content = read_text(path)?;
    if hashes.is_some_and(|h| h.get(&filename).and_then(Value::as_str) != Some(sha256(&content).as_str())) {
        errors.push(format!("{filename} does not match its manifest hash"));
    }
    let Some(groups) = read_json_object(path, &filename, errors) else {
        return Ok(());
    };
    let mut actual = IndexMap::new();
    for (api, value) in js_json::object_entries(&groups) {
        let Some(group) = value.as_object() else {
            errors.push(format!(
                "{filename} API group {} must be an object",
                json_string(Some(&Value::String(api.clone())))
            ));
            continue;
        };
        for (id, model) in js_json::object_entries(group) {
            if actual.contains_key(id) {
                errors.push(format!("{provider}/{id} appears in more than one API group"));
                continue;
            }
            actual.insert(id.clone(), api.clone());
            validate_model(model, provider, id, api, errors);
        }
    }
    let mut expected_ids: Vec<_> = expected.keys().cloned().collect();
    let mut actual_ids: Vec<_> = actual.keys().cloned().collect();
    sort_strings(&mut expected_ids);
    sort_strings(&mut actual_ids);
    if expected_ids != actual_ids {
        errors.push(format!(
            "{filename} model IDs do not match the generated catalog ({})",
            difference(&expected_ids, &actual_ids)
        ));
    }
    for id in record_keys(expected) {
        if let Some(api) = actual.get(&id).filter(|api| **api != expected[&id]) {
            errors.push(format!(
                "{provider}/{id} is grouped under API {}, expected {}",
                json_string(Some(&Value::String(api.clone()))),
                json_string(Some(&Value::String(expected[&id].clone())))
            ));
        }
    }
    Ok(())
}

pub fn validate_model_data_directory(structure: &ModelDataStructure, data_dir: &Path) -> Result<()> {
    if !data_dir.is_dir() {
        return Err(ModelDataError(format!(
            "Generated model data directory does not exist: {}",
            data_dir.display()
        )));
    }
    let mut errors = Vec::new();
    let mut expected: Vec<_> = structure.keys().map(|id| format!("{id}.json")).collect();
    sort_strings(&mut expected);
    let actual = matching_files(data_dir, ".json")?;
    if expected != actual {
        errors.push(format!(
            "provider data files do not match the generated catalog ({})",
            difference(&expected, &actual)
        ));
    }
    let manifest = read_json_object(
        &data_dir.join(MODEL_DATA_MANIFEST_FILE),
        "model data manifest",
        &mut errors,
    );
    let hashes = validate_manifest(structure, manifest.as_ref(), &expected, &mut errors);
    for provider in record_keys(structure) {
        validate_provider(
            &data_dir.join(format!("{provider}.json")),
            &provider,
            &structure[&provider],
            hashes.as_ref(),
            &mut errors,
        )?;
    }
    if errors.is_empty() {
        return Ok(());
    }
    let suffix = if errors.len() > 30 {
        format!("\n  ... and {} more", errors.len() - 30)
    } else {
        String::new()
    };
    Err(ModelDataError(format!(
        "Invalid generated model data:\n{}{suffix}",
        errors
            .iter()
            .take(30)
            .map(|e| format!("  - {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    )))
}

pub fn validate_generated_model_data(package_root: &Path) -> Result<()> {
    let structure = read_model_data_structure(package_root)?;
    validate_model_data_directory(&structure, &package_root.join("src/providers/data"))
}
