use super::*;
use crate::model_data_tools::{ModelDataStructure, create_model_data_manifest, validate_model_data_directory};
use indexmap::IndexMap;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Local filesystem boundary for deterministic artifact-transaction fixtures.
pub trait CatalogFileSystem {
    fn read(&mut self, path: &Path) -> Result<String, GeneratorError>;
    fn write(&mut self, path: &Path, content: &str) -> Result<(), GeneratorError>;
    fn mkdir(&mut self, path: &Path) -> Result<(), GeneratorError>;
    fn remove(&mut self, path: &Path) -> Result<(), GeneratorError>;
    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), GeneratorError>;
    fn exists(&mut self, path: &Path) -> bool;
    fn staging_dir(&mut self, providers: &Path) -> Result<PathBuf, GeneratorError>;
}
pub struct NativeCatalogFileSystem;
impl CatalogFileSystem for NativeCatalogFileSystem {
    fn read(&mut self, path: &Path) -> Result<String, GeneratorError> {
        read(path)
    }
    fn write(&mut self, path: &Path, content: &str) -> Result<(), GeneratorError> {
        write(path, content)
    }
    fn mkdir(&mut self, path: &Path) -> Result<(), GeneratorError> {
        mkdir(path)
    }
    fn remove(&mut self, path: &Path) -> Result<(), GeneratorError> {
        remove(path)
    }
    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), GeneratorError> {
        rename(from, to)
    }
    fn exists(&mut self, path: &Path) -> bool {
        path.exists()
    }
    fn staging_dir(&mut self, providers: &Path) -> Result<PathBuf, GeneratorError> {
        staging_dir(providers)
    }
}

fn io_error(error: io::Error, syscall: &str, path: &Path) -> GeneratorError {
    let record = pi_js_compat::node_fs::node_fs_error_record(&error, syscall, &path.to_string_lossy());
    let mut error = GeneratorError::new(record.name, JsString::from(record.message.as_str()));
    error.details = Some(GeneratorErrorDetails::FileSystem {
        record,
        destination: None,
    });
    error
}
fn read(path: &Path) -> Result<String, GeneratorError> {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| {
            let syscall = if error.kind() == io::ErrorKind::IsADirectory {
                "read"
            } else {
                "open"
            };
            io_error(error, syscall, path)
        })
}
fn write(path: &Path, content: &str) -> Result<(), GeneratorError> {
    fs::write(path, content).map_err(|error| io_error(error, "open", path))
}
fn mkdir(path: &Path) -> Result<(), GeneratorError> {
    fs::create_dir_all(path).map_err(|error| io_error(error, "mkdir", path))
}
fn remove(path: &Path) -> Result<(), GeneratorError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error, "lstat", path)),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| io_error(error, "rm", path))
    } else {
        fs::remove_file(path).map_err(|error| io_error(error, "unlink", path))
    }
}
fn rename(from: &Path, to: &Path) -> Result<(), GeneratorError> {
    fs::rename(from, to).map_err(|error| {
        let mut error = io_error(error, "rename", from);
        error.message = JsString::from(format!("{} -> '{}'", error.message.to_utf8_lossy(), to.display()).as_str());
        if let Some(GeneratorErrorDetails::FileSystem { destination, .. }) = &mut error.details {
            *destination = Some(to.to_string_lossy().into_owned());
        }
        error
    })
}
fn serialize(value: &JsValue, pretty: bool) -> String {
    let value = if pretty {
        pi_js_compat::js_json::stringify_lossless_pretty(value)
    } else {
        pi_js_compat::js_json::stringify_lossless(value)
    };
    format!("{value}\n")
}
fn checked(error: crate::model_data_tools::ModelDataError) -> GeneratorError {
    GeneratorError::new("Error", error.0)
}
fn include_path(root: &Path) -> PathBuf {
    root.join("crates/pi-ai/src/providers/model_catalogs_generated.rs")
}
fn native_provider_ids(root: &Path, filesystem: &mut dyn CatalogFileSystem) -> Result<Vec<JsString>, GeneratorError> {
    let path = include_path(root);
    let content = filesystem.read(&path)?;
    let pattern =
        regex::Regex::new(r#"\(\s*("[^"\n]+"),\s*include_str!\("../../data/providers/[^"\n]+\.json"\)\s*,?\s*\)"#)
            .unwrap();
    let mut ids = vec![];
    for captures in pattern.captures_iter(&content) {
        if let JsValue::String(id) = pi_js_compat::js_json::parse_lossless(&captures[1])
            .map_err(|error| GeneratorError::error(&error.to_string()))?
        {
            ids.push(id);
        }
    }
    ids.sort();
    if ids.is_empty() {
        return Err(GeneratorError::error(&format!(
            "No generated provider imports found in {}",
            path.display()
        )));
    }
    if ids.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err(GeneratorError::error(&format!(
            "Generated model aggregator contains duplicate provider imports: {}",
            path.display()
        )));
    }
    Ok(ids)
}
fn include_source(ids: &[JsString]) -> Result<String, GeneratorError> {
    let mut output =
        String::from("// Generated by generate-models; do not edit.\nconst PROVIDER_CATALOGS: &[(&str, &str)] = &[\n");
    for id in ids {
        let id = id
            .to_utf8()
            .map_err(|error| GeneratorError::error(&error.to_string()))?;
        let quoted = serde_json::to_string(&id).expect("provider ID serializes");
        let file = serde_json::to_string(&format!("../../data/providers/{id}.json")).expect("provider path serializes");
        let row = format!("({quoted}, include_str!({file}))");
        if row.len() <= 72 {
            output.push_str(&format!("    {row},\n"));
        } else {
            output.push_str(&format!(
                "    (\n        {quoted},\n        include_str!({file}),\n    ),\n"
            ));
        }
    }
    output.push_str("];\n");
    Ok(output)
}
fn group_catalog(catalog: &JsValue, ids: &[JsString]) -> Result<(JsValue, ModelDataStructure), GeneratorError> {
    let mut grouped = IndexMap::new();
    let mut structure = IndexMap::new();
    for provider in ids {
        let models = match catalog {
            JsValue::Object(providers) => providers.get(provider),
            _ => None,
        }
        .ok_or_else(|| GeneratorError::error("Missing provider during generation"))?;
        let mut apis = vec![];
        for (_, model) in entries(models) {
            if let JsValue::String(api) = field(model, "api") {
                if !apis.contains(api) {
                    apis.push(api.clone());
                }
            }
        }
        apis.sort();
        let mut groups = IndexMap::new();
        let mut provider_structure = IndexMap::new();
        for api in apis {
            let mut group = IndexMap::new();
            for (id, model) in entries(models) {
                if field(model, "api") == &JsValue::String(api.clone()) {
                    group.insert(id.clone(), model.clone());
                    provider_structure.insert(id.clone(), api.clone());
                }
            }
            groups.insert(api, JsValue::Object(group));
        }
        grouped.insert(provider.clone(), JsValue::Object(groups));
        structure.insert(provider.clone(), provider_structure);
    }
    Ok((JsValue::Object(grouped), structure))
}
fn staging_dir(providers: &Path) -> Result<PathBuf, GeneratorError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| GeneratorError::error(&error.to_string()))?;
    let suffix = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let root = providers.join(format!(".model-generation-{suffix}"));
    fs::create_dir(&root).map_err(|error| io_error(error, "mkdtemp", &root))?;
    Ok(root)
}

impl Generator<'_> {
    /// Generate native values/include glue and optional public catalogs under the supplied repository.
    pub fn generate(&mut self, root: &Path, generated_at: &JsString) -> Result<JsValue, GeneratorError> {
        self.generate_with_filesystem(root, generated_at, &mut NativeCatalogFileSystem)
    }
    pub fn generate_with_filesystem(
        &mut self,
        root: &Path,
        generated_at: &JsString,
        filesystem: &mut dyn CatalogFileSystem,
    ) -> Result<JsValue, GeneratorError> {
        let catalog = self.generate_catalog()?;
        let ids = entries(&catalog)
            .into_iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let generated_ids = if self.options.data_only {
            native_provider_ids(root, filesystem)?
        } else {
            ids.clone()
        };
        let missing = generated_ids
            .iter()
            .filter(|id| !matches!(&catalog,JsValue::Object(catalog) if catalog.contains_key(*id)))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let mut units = "Cannot hydrate missing providers: ".encode_utf16().collect::<Vec<_>>();
            for (index, id) in missing.iter().enumerate() {
                if index != 0 {
                    units.extend(", ".encode_utf16());
                }
                units.extend_from_slice(id.units());
            }
            return Err(GeneratorError::new("Error", JsString::from_utf16(units)));
        }
        let (groups, structure) = group_catalog(&catalog, &generated_ids)?;
        if !self.options.json_only {
            self.write_native(root, &groups, &structure, &generated_ids, generated_at, filesystem)?;
        }
        if let Some(output) = self.options.json_output_dir.clone() {
            let providers = output.join("providers");
            filesystem.remove(&output)?;
            filesystem.mkdir(&providers)?;
            emission_types::public_catalog(&catalog)?;
            filesystem.write(&output.join("models.json"), &serialize(&catalog, self.options.pretty))?;
            filesystem.write(
                &output.join("providers.json"),
                &serialize(
                    &JsValue::Array(ids.iter().cloned().map(JsValue::String).collect()),
                    self.options.pretty,
                ),
            )?;
            for (provider, models) in entries(&catalog) {
                let file = provider.to_utf8_lossy();
                filesystem.write(
                    &providers.join(format!("{file}.json")),
                    &serialize(models, self.options.pretty),
                )?;
            }
            self.log(
                "log",
                &format!("Generated JSON model catalog under {}", output.display()),
            );
        }
        self.log("log", "\nModel Statistics:");
        self.log("log", &format!("  Total tool-capable models: {}", self.total_models));
        self.log("log", &format!("  Reasoning-capable models: {}", self.reasoning_models));
        for (provider, count) in self.provider_counts.clone() {
            self.log("log", &format!("  {}: {count} models", provider.to_utf8_lossy()));
        }
        Ok(catalog)
    }
    fn write_native(
        &mut self,
        root: &Path,
        groups: &JsValue,
        structure: &ModelDataStructure,
        ids: &[JsString],
        generated_at: &JsString,
        filesystem: &mut dyn CatalogFileSystem,
    ) -> Result<(), GeneratorError> {
        let providers = root.join("crates/pi-ai/src/providers");
        let data = root.join("crates/pi-ai/data/providers");
        let staging = filesystem.staging_dir(&providers)?;
        let staged = staging.join("data");
        let previous = staging.join("previous-data");
        let mut restore_include: Option<String> = None;
        let result = (|| {
            filesystem.mkdir(&staged)?;
            let mut files = IndexMap::new();
            for (provider, models) in entries(groups) {
                let filename = format!("{}.json", provider.to_utf8_lossy());
                let content = serialize(models, self.options.pretty);
                filesystem.write(&staged.join(&filename), &content)?;
                files.insert(JsString::from(filename.as_str()), JsString::from(content.as_str()));
            }
            let manifest = create_model_data_manifest(structure, &files, generated_at);
            filesystem.write(
                &staged.join(".manifest.json"),
                &serialize(&manifest.to_json(), self.options.pretty),
            )?;
            validate_model_data_directory(structure, &staged).map_err(checked)?;
            emission_types::grouped_catalog(groups)?;
            if !self.options.data_only {
                let path = include_path(root);
                restore_include = Some(filesystem.read(&path)?);
                filesystem.write(&path, &include_source(ids)?)?;
                self.log(
                    "log",
                    "Generated provider catalogs and crates/pi-ai/src/providers/model_catalogs_generated.rs",
                );
            }
            if let Some(parent) = data.parent() {
                filesystem.mkdir(parent)?;
            }
            let had_previous = filesystem.exists(&data);
            if had_previous {
                filesystem.rename(&data, &previous)?;
            }
            let replacement = (|| {
                filesystem.rename(&staged, &data)?;
                let actual_ids = native_provider_ids(root, filesystem)?;
                crate::model_data_tools::assert_exact_model_ids(
                    &JsString::from("Generated model aggregator"),
                    ids.iter().cloned(),
                    actual_ids.iter().cloned(),
                )
                .map_err(checked)?;
                let mut actual_structure = ModelDataStructure::new();
                for provider in actual_ids {
                    let path = data.join(format!("{}.json", provider.to_utf8_lossy()));
                    let models = crate::model_data_tools::read_provider_structure(&path, &provider).map_err(checked)?;
                    actual_structure.insert(provider, models);
                }
                validate_model_data_directory(&actual_structure, &data).map_err(checked)
            })();
            if let Err(error) = replacement {
                filesystem.remove(&data)?;
                if had_previous && filesystem.exists(&previous) {
                    filesystem.rename(&previous, &data)?;
                }
                return Err(error);
            }
            restore_include = None;
            self.log(
                "log",
                if self.options.data_only {
                    "Hydrated JSON model values under crates/pi-ai/data/providers/"
                } else {
                    "Generated JSON model values under crates/pi-ai/data/providers/"
                },
            );
            Ok(())
        })();
        let result = (|| match result {
            Err(error) => {
                if let Some(previous) = restore_include {
                    filesystem.write(&include_path(root), &previous)?;
                }
                Err(error)
            }
            Ok(()) => Ok(()),
        })();
        let cleanup = filesystem.remove(&staging);
        cleanup?;
        result
    }
}
