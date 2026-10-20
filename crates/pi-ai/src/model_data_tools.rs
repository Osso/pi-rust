//! Offline maintenance APIs from `packages/ai/scripts/model-data.ts`.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

use indexmap::IndexMap;
use pi_js_compat::{date::date_parse, js_json};

use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use sha2::{Digest, Sha256};

pub const MODEL_DATA_SCHEMA_VERSION: u32 = 3;
pub const MODEL_DATA_MANIFEST_FILE: &str = ".manifest.json";
pub type ModelDataStructure = IndexMap<JsString, IndexMap<JsString, JsString>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDataManifest {
    pub schema_version: u32,
    pub generated_at: JsString,
    pub structure_hash: String,
    pub files: IndexMap<JsString, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDataError(pub JsString);
impl ModelDataError {
    pub fn message(&self) -> &JsString {
        &self.0
    }
}
impl fmt::Display for ModelDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_utf8_lossy())
    }
}
impl std::error::Error for ModelDataError {}
type Result<T> = std::result::Result<T, ModelDataError>;

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn sort_strings(values: &mut [JsString]) {
    values.sort();
}
fn sorted_record<T>(entries: impl IntoIterator<Item = (JsString, T)>) -> IndexMap<JsString, T> {
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut record: IndexMap<_, _> = entries.into_iter().collect();
    record_keys(&record)
        .into_iter()
        .map(|key| {
            let value = record.swap_remove(&key).expect("enumerated key exists");
            (key, value)
        })
        .collect()
}
fn record_keys<T>(record: &IndexMap<JsString, T>) -> Vec<JsString> {
    let object = record.keys().map(|key| (key.clone(), Value::Null)).collect();
    js_json::object_entries_lossless(&object)
        .into_iter()
        .map(|(key, _)| key.clone())
        .collect()
}
fn concat(parts: &[JsString]) -> JsString {
    JsString::from_utf16(parts.iter().flat_map(|s| s.units().iter().copied()).collect())
}
fn join(parts: &[JsString], separator: &str) -> JsString {
    let mut units = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            units.extend(separator.encode_utf16());
        }
        units.extend(part.units());
    }
    JsString::from_utf16(units)
}
trait ToJs {
    fn to_js(&self) -> JsString;
}
impl ToJs for JsString {
    fn to_js(&self) -> JsString {
        self.clone()
    }
}
impl<T: ToJs + ?Sized> ToJs for &T {
    fn to_js(&self) -> JsString {
        (*self).to_js()
    }
}
impl ToJs for str {
    fn to_js(&self) -> JsString {
        self.into()
    }
}
impl ToJs for String {
    fn to_js(&self) -> JsString {
        self.as_str().into()
    }
}
impl ToJs for usize {
    fn to_js(&self) -> JsString {
        self.to_string().as_str().into()
    }
}
impl ToJs for u32 {
    fn to_js(&self) -> JsString {
        self.to_string().as_str().into()
    }
}
impl ToJs for std::path::Display<'_> {
    fn to_js(&self) -> JsString {
        self.to_string().as_str().into()
    }
}
impl ToJs for std::io::Error {
    fn to_js(&self) -> JsString {
        self.to_string().as_str().into()
    }
}
impl ToJs for ModelDataError {
    fn to_js(&self) -> JsString {
        self.0.clone()
    }
}
macro_rules! jformat { ($template:literal $(,$value:expr)* $(,)?) => {{
    let args: Vec<JsString> = vec![$($value.to_js()),*];
    let mut pieces=Vec::new(); let mut args=args.into_iter();
    for (i,part) in $template.split("{}").enumerate() { if i>0 {pieces.push(args.next().expect("format argument"));} pieces.push(part.into()); }
    concat(&pieces)
}}; }
fn string(value: &Value) -> Option<&JsString> {
    if let Value::String(s) = value { Some(s) } else { None }
}
fn object(value: &Value) -> Option<&JsObject> {
    if let Value::Object(s) = value { Some(s) } else { None }
}
fn number(value: &Value) -> Option<f64> {
    if let Value::Number(s) = value { s.as_f64() } else { None }
}
fn array(value: &Value) -> Option<&Vec<Value>> {
    if let Value::Array(s) = value { Some(s) } else { None }
}
fn get<'a>(map: &'a JsObject, key: &str) -> Option<&'a Value> {
    map.get(&JsString::from(key))
}
pub fn model_data_structure_json(structure: &ModelDataStructure) -> Value {
    Value::Object(
        structure
            .iter()
            .map(|(key, models)| {
                (
                    key.clone(),
                    Value::Object(
                        models
                            .iter()
                            .map(|(id, api)| (id.clone(), Value::String(api.clone())))
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}
impl ModelDataManifest {
    pub fn to_json(&self) -> Value {
        Value::Object(
            [
                ("schemaVersion".into(), Value::Number(self.schema_version.into())),
                ("generatedAt".into(), Value::String(self.generated_at.clone())),
                (
                    "structureHash".into(),
                    Value::String(self.structure_hash.as_str().into()),
                ),
                (
                    "files".into(),
                    Value::Object(
                        self.files
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::String(v.as_str().into())))
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
    pub fn stringify(&self) -> String {
        js_json::stringify_lossless(&self.to_json())
    }
}

fn difference(expected: &[JsString], actual: &[JsString]) -> JsString {
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
        parts.push(jformat!("missing: {}", join(&missing, ", ")));
    }
    if !extra.is_empty() {
        parts.push(jformat!("extra: {}", join(&extra, ", ")));
    }
    join(&parts, "; ")
}

pub fn assert_exact_model_ids<S: Into<JsString>>(
    label: &JsString,
    expected: impl IntoIterator<Item = S>,
    actual: impl IntoIterator<Item = S>,
) -> Result<()> {
    let normalize = |items: Vec<JsString>| {
        let mut items = items;
        sort_strings(&mut items);
        items.dedup();
        items
    };
    let expected = normalize(expected.into_iter().map(|s| s.into()).collect());
    let actual = normalize(actual.into_iter().map(|s| s.into()).collect());
    if expected == actual {
        return Ok(());
    }
    Err(ModelDataError(jformat!(
        "{} model IDs do not match ({})",
        label,
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
        _ => return ModelDataError(jformat!("{}, {} '{}'", error, operation, path.display())),
    };
    ModelDataError(jformat!(
        "{}: {}, {} '{}'",
        code,
        description,
        operation,
        path.display()
    ))
}
fn read_text(path: &Path) -> Result<String> {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| fs_error(e, "open", path))
}
fn matching_files(dir: &Path, suffix: &str) -> Result<Vec<JsString>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| fs_error(e, "scandir", dir))? {
        let entry = entry.map_err(|e| fs_error(e, "scandir", dir))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(suffix) && name != MODEL_DATA_MANIFEST_FILE {
            names.push(name.as_str().into());
        }
    }
    sort_strings(&mut names);
    Ok(names)
}
fn read_json_object(path: &Path, description: &JsString, errors: &mut Vec<JsString>) -> Option<JsObject> {
    let parsed = read_text(path)
        .and_then(|s| js_json::parse_lossless(&s).map_err(|e| ModelDataError(e.to_string().as_str().into())));
    match parsed {
        Ok(Value::Object(object)) => Some(object),
        Ok(_) => {
            errors.push(jformat!("{} must contain a JSON object", description));
            None
        }
        Err(error) => {
            errors.push(jformat!("{} is not valid JSON: {}", description, error));
            None
        }
    }
}

pub fn read_model_data_provider_ids(package_root: &Path) -> Result<Vec<JsString>> {
    let path = package_root.join("src/models.generated.ts");
    let text = read_text(&path)?;
    let pattern = regex::Regex::new(
        r#"(?:^|[\n\r\u{2028}\u{2029}])import \{ [A-Z][A-Z0-9_]*_MODELS \} from "\./providers/([^"/]+)\.models\.ts";"#,
    )
    .expect("static pattern");
    let mut ids: Vec<_> = pattern
        .captures_iter(&text)
        .filter(|capture| {
            text[capture.get(0).expect("whole match").end()..]
                .chars()
                .next()
                .is_none_or(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        })
        .map(|capture| JsString::from(&capture[1]))
        .collect();
    sort_strings(&mut ids);
    if ids.is_empty() {
        return Err(ModelDataError(jformat!(
            "No generated provider imports found in {}",
            path.display()
        )));
    }
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ModelDataError(jformat!(
            "Generated model aggregator contains duplicate provider imports: {}",
            path.display()
        )));
    }
    Ok(ids)
}

fn read_provider_structure(path: &Path, provider: &JsString) -> Result<IndexMap<JsString, JsString>> {
    let mut errors = Vec::new();
    let groups = read_json_object(path, &jformat!("{}.json", provider), &mut errors)
        .ok_or_else(|| ModelDataError(join(&errors, "\n")))?;
    let mut models = IndexMap::new();
    for (api, value) in js_json::object_entries_lossless(&groups) {
        let group = object(value).ok_or_else(|| {
            ModelDataError(jformat!(
                "{} API group {} must be an object",
                path.display(),
                json_string(Some(&Value::String(api.clone())))
            ))
        })?;
        for (id, _) in js_json::object_entries_lossless(group) {
            if models.contains_key(id) {
                return Err(ModelDataError(jformat!(
                    "{} contains model {} in more than one API group",
                    path.display(),
                    id
                )));
            }
            models.insert(id.clone(), api.clone());
        }
    }
    if models.is_empty() {
        return Err(ModelDataError(jformat!(
            "{} contains no generated model data",
            path.display()
        )));
    }
    Ok(sorted_record(models))
}

pub fn read_model_data_structure(package_root: &Path) -> Result<ModelDataStructure> {
    let providers = package_root.join("src/providers");
    let ids = read_model_data_provider_ids(package_root)?;
    let mut expected: Vec<_> = ids.iter().map(|id| jformat!("{}.models.ts", id)).collect();
    sort_strings(&mut expected);
    let actual = matching_files(&providers, ".models.ts")?;
    if expected != actual {
        return Err(ModelDataError(jformat!(
            "Generated model aggregator and provider shards do not match ({})",
            difference(&expected, &actual)
        )));
    }
    let mut structure = IndexMap::new();
    for id in ids {
        let models = read_provider_structure(
            &providers.join("data").join(jformat!("{}.json", id).to_utf8_lossy()),
            &id,
        )?;
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
    sha256(&js_json::stringify_lossless(&model_data_structure_json(&normalized)))
}

pub fn create_model_data_manifest(
    structure: &ModelDataStructure,
    file_contents: &IndexMap<JsString, JsString>,
    generated_at: &JsString,
) -> ModelDataManifest {
    ModelDataManifest {
        schema_version: MODEL_DATA_SCHEMA_VERSION,
        generated_at: generated_at.clone(),
        structure_hash: model_data_structure_hash(structure),
        files: sorted_record(
            file_contents
                .iter()
                .map(|(file, content)| (file.clone(), sha256(&content.to_utf8_lossy()))),
        ),
    }
}

fn json_string(value: Option<&Value>) -> JsString {
    value
        .map(js_json::stringify_lossless)
        .map(|s| JsString::from(s.as_str()))
        .unwrap_or_else(|| "undefined".into())
}
fn validate_model(value: &Value, provider: &JsString, id: &JsString, api: &JsString, errors: &mut Vec<JsString>) {
    let label = jformat!("{}/{}", provider, id);
    let Some(model) = object(value) else {
        errors.push(jformat!("{} must be an object", label));
        return;
    };
    for (field, expected) in [("id", id), ("provider", provider), ("api", api)] {
        if get(model, field).and_then(string) != Some(expected) {
            errors.push(jformat!(
                "{} has {} {}, expected {}",
                label,
                field,
                json_string(get(model, field)),
                js_json::stringify_lossless(&Value::String(expected.clone()))
            ));
        }
    }
    if !get(model, "name")
        .and_then(string)
        .is_some_and(|s| !s.units().is_empty())
    {
        errors.push(jformat!("{} has no model name", label));
    }
    if !get(model, "baseUrl").is_some_and(|v| matches!(v, Value::String(_))) {
        errors.push(jformat!("{} has no baseUrl string", label));
    }
    if !get(model, "reasoning").is_some_and(|v| matches!(v, Value::Bool(_))) {
        errors.push(jformat!("{} has no reasoning boolean", label));
    }
    if !get(model, "input").and_then(array).is_some_and(|v| {
        !v.is_empty()
            && v.iter()
                .all(|v| string(v).is_some_and(|s| s == &JsString::from("text") || s == &JsString::from("image")))
    }) {
        errors.push(jformat!("{} has invalid input modalities", label));
    }
    for field in ["contextWindow", "maxTokens"] {
        if !get(model, field)
            .and_then(number)
            .is_some_and(|v| v.is_finite() && v > 0.0)
        {
            errors.push(jformat!("{} has invalid {}", label, field));
        }
    }
    let Some(cost) = get(model, "cost").and_then(object) else {
        errors.push(jformat!("{} has invalid cost metadata", label));
        return;
    };
    for field in ["input", "output", "cacheRead", "cacheWrite"] {
        if !get(cost, field).and_then(number).is_some_and(f64::is_finite) {
            errors.push(jformat!("{} has invalid cost.{}", label, field));
        }
    }
}

fn validate_manifest(
    structure: &ModelDataStructure,
    manifest: Option<&JsObject>,
    expected: &[JsString],
    errors: &mut Vec<JsString>,
) -> Option<JsObject> {
    let field = |name| manifest.and_then(|m| get(m, name));
    if field("schemaVersion").and_then(number) != Some(f64::from(MODEL_DATA_SCHEMA_VERSION)) {
        errors.push(jformat!(
            "model data schema is {}, expected {}",
            json_string(field("schemaVersion")),
            MODEL_DATA_SCHEMA_VERSION
        ));
    }
    if !field("generatedAt")
        .and_then(string)
        .is_some_and(|s| !s.to_utf8().map(|s| date_parse(&s)).unwrap_or(f64::NAN).is_nan())
    {
        errors.push("model data manifest has an invalid generation timestamp".into());
    }
    if field("structureHash").and_then(string) != Some(&JsString::from(model_data_structure_hash(structure).as_str())) {
        errors.push("model data generation stamp does not match the generated catalog".into());
    }
    let files = field("files").and_then(object);
    let Some(files) = files else {
        errors.push("model data manifest has no file hashes".into());
        return None;
    };
    let mut actual: Vec<_> = files.keys().cloned().collect();
    sort_strings(&mut actual);
    if expected != actual {
        errors.push(jformat!(
            "manifest file hashes do not match provider data files ({})",
            difference(expected, &actual)
        ));
    }
    Some(files.clone())
}

fn validate_provider(
    path: &Path,
    provider: &JsString,
    expected: &IndexMap<JsString, JsString>,
    hashes: Option<&JsObject>,
    errors: &mut Vec<JsString>,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let filename = jformat!("{}.json", provider);
    let content = read_text(path)?;
    if hashes.is_some_and(|h| h.get(&filename).and_then(string) != Some(&JsString::from(sha256(&content).as_str()))) {
        errors.push(jformat!("{} does not match its manifest hash", filename));
    }
    let Some(groups) = read_json_object(path, &filename, errors) else {
        return Ok(());
    };
    let mut actual = IndexMap::new();
    for (api, value) in js_json::object_entries_lossless(&groups) {
        let Some(group) = object(value) else {
            errors.push(jformat!(
                "{} API group {} must be an object",
                filename,
                json_string(Some(&Value::String(api.clone())))
            ));
            continue;
        };
        for (id, model) in js_json::object_entries_lossless(group) {
            if actual.contains_key(id) {
                errors.push(jformat!("{}/{} appears in more than one API group", provider, id));
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
        errors.push(jformat!(
            "{} model IDs do not match the generated catalog ({})",
            filename,
            difference(&expected_ids, &actual_ids)
        ));
    }
    for id in record_keys(expected) {
        if let Some(api) = actual.get(&id).filter(|api| **api != expected[&id]) {
            errors.push(jformat!(
                "{}/{} is grouped under API {}, expected {}",
                provider,
                id,
                json_string(Some(&Value::String(api.clone()))),
                json_string(Some(&Value::String(expected[&id].clone())))
            ));
        }
    }
    Ok(())
}

pub fn validate_model_data_directory(structure: &ModelDataStructure, data_dir: &Path) -> Result<()> {
    if !data_dir.is_dir() {
        return Err(ModelDataError(jformat!(
            "Generated model data directory does not exist: {}",
            data_dir.display()
        )));
    }
    let mut errors = Vec::new();
    let mut expected: Vec<_> = structure.keys().map(|id| jformat!("{}.json", id)).collect();
    sort_strings(&mut expected);
    let actual = matching_files(data_dir, ".json")?;
    if expected != actual {
        errors.push(jformat!(
            "provider data files do not match the generated catalog ({})",
            difference(&expected, &actual)
        ));
    }
    let manifest = read_json_object(
        &data_dir.join(MODEL_DATA_MANIFEST_FILE),
        &JsString::from("model data manifest"),
        &mut errors,
    );
    let hashes = validate_manifest(structure, manifest.as_ref(), &expected, &mut errors);
    for provider in record_keys(structure) {
        validate_provider(
            &data_dir.join(jformat!("{}.json", provider).to_utf8_lossy()),
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
        jformat!("\n  ... and {} more", errors.len() - 30)
    } else {
        JsString::from("")
    };
    Err(ModelDataError(jformat!(
        "Invalid generated model data:\n{}{}",
        join(
            &errors
                .iter()
                .take(30)
                .map(|e| jformat!("  - {}", e))
                .collect::<Vec<_>>(),
            "\n"
        ),
        suffix
    )))
}

pub fn validate_generated_model_data(package_root: &Path) -> Result<()> {
    let structure = read_model_data_structure(package_root)?;
    validate_model_data_directory(&structure, &package_root.join("src/providers/data"))
}
