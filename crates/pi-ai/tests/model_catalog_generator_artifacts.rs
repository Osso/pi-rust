#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse};
use pi_js_compat::js_json::JsString;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pi-native-model-generator-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

struct FaultFs {
    root: PathBuf,
    fault: &'static str,
    injected: bool,
    native: pi_ai::model_catalog_generator::NativeCatalogFileSystem,
}
impl pi_ai::model_catalog_generator::CatalogFileSystem for FaultFs {
    fn read(&mut self, path: &std::path::Path) -> Result<String, pi_ai::model_catalog_generator::GeneratorError> {
        self.native.read(path)
    }
    fn mkdir(&mut self, path: &std::path::Path) -> Result<(), pi_ai::model_catalog_generator::GeneratorError> {
        self.native.mkdir(path)
    }
    fn exists(&mut self, path: &std::path::Path) -> bool {
        self.native.exists(path)
    }
    fn staging_dir(
        &mut self,
        path: &std::path::Path,
    ) -> Result<PathBuf, pi_ai::model_catalog_generator::GeneratorError> {
        self.native.staging_dir(path)
    }
    fn write(
        &mut self,
        path: &std::path::Path,
        content: &str,
    ) -> Result<(), pi_ai::model_catalog_generator::GeneratorError> {
        let stage = path.to_string_lossy().contains(".model-generation-");
        let include = self.root.join("crates/pi-ai/src/providers/model_catalogs_generated.rs");
        if !self.injected
            && ((self.fault == "stage-write" && stage && path.ends_with("openai.json"))
                || (self.fault == "include-write" && path == include))
        {
            self.injected = true;
            self.native.remove(path)?;
            self.native.mkdir(path)?;
            let result = self.native.write(path, content);
            self.native.remove(path)?;
            return result;
        }
        if !self.injected && self.fault == "stage-validate" && stage && path.ends_with("openai.json") {
            self.injected = true;
            return self.native.write(path, "{}");
        }
        self.native.write(path, content)
    }
    fn rename(
        &mut self,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> Result<(), pi_ai::model_catalog_generator::GeneratorError> {
        let data = self.root.join("crates/pi-ai/data/providers");
        if !self.injected && to == data && from.ends_with("data") {
            if self.fault == "data-rename" {
                self.injected = true;
                self.native.mkdir(to)?;
                self.native.write(&to.join("block"), "block")?;
                let result = self.native.rename(from, to);
                self.native.remove(to)?;
                return result;
            }
            self.native.rename(from, to)?;
            match self.fault {
                "post-manifest" => {
                    self.injected = true;
                    let path = to.join(".manifest.json");
                    let mut manifest: serde_json::Value = serde_json::from_str(&self.native.read(&path)?).unwrap();
                    manifest["structureHash"] = json!("wrong");
                    self.native.write(&path, &manifest.to_string())?;
                }
                "post-provider" => {
                    self.injected = true;
                    self.native.write(&to.join("openai.json"), "{}")?;
                }
                "post-include" => {
                    self.injected = true;
                    self.native.write(
                        &self.root.join("crates/pi-ai/src/providers/model_catalogs_generated.rs"),
                        "",
                    )?;
                }
                _ => {}
            }
            return Ok(());
        }
        self.native.rename(from, to)
    }
    fn remove(&mut self, path: &std::path::Path) -> Result<(), pi_ai::model_catalog_generator::GeneratorError> {
        if !self.injected
            && self.fault == "cleanup"
            && path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".model-generation-")
        {
            self.injected = true;
            return self.native.read(path).map(|_| ());
        }
        self.native.remove(path)
    }
}
fn normalized_error(text: &str, root: &str, source: bool) -> String {
    let text = text.replace(root, "<ROOT>");
    let text = if source {
        text.replace(
            "src/models.generated.ts",
            "crates/pi-ai/src/providers/model_catalogs_generated.rs",
        )
        .replace("src/providers/data", "crates/pi-ai/data/providers")
        .replace(
            "src/providers/.model-generation-",
            "crates/pi-ai/src/providers/.model-generation-",
        )
    } else {
        text
    };
    regex::Regex::new(r"\.model-generation-[^/'\s]+")
        .unwrap()
        .replace_all(&text, ".model-generation-<STAGE>")
        .into_owned()
}
#[test]
fn source_transaction_failures_restore_values_include_and_cleanup_at_exact_boundaries() {
    use pi_ai::model_catalog_generator::{GeneratorErrorDetails, NativeCatalogFileSystem};
    use pi_ai::model_data_tools::{ModelDataStructure, create_model_data_manifest};
    use pi_js_compat::js_json::{JsValue, stringify_lossless};
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let mut failures = vec![];
    for fault in [
        "stage-write",
        "stage-validate",
        "include-write",
        "data-rename",
        "post-manifest",
        "post-provider",
        "post-include",
        "cleanup",
    ] {
        let root = Sandbox::new();
        let providers = root.0.join("crates/pi-ai/src/providers");
        fs::create_dir_all(&providers).unwrap();
        let data = root.0.join("crates/pi-ai/data/providers");
        fs::create_dir_all(&data).unwrap();
        let old = stringify_lossless(&JsValue::from(
            &json!({"openai-responses":{"old":{"id":"old","name":"Old","provider":"openai","api":"openai-responses","baseUrl":"https://example.invalid","reasoning":false,"input":["text"],"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":1024}}}),
        ));
        let structure: ModelDataStructure = [(
            JsString::from("openai"),
            [(JsString::from("old"), JsString::from("openai-responses"))]
                .into_iter()
                .collect(),
        )]
        .into_iter()
        .collect();
        let contents = [(JsString::from("openai.json"), JsString::from(old.as_str()))]
            .into_iter()
            .collect();
        let manifest =
            create_model_data_manifest(&structure, &contents, &JsString::from("2026-10-19T00:00:00.000Z")).stringify();
        let include = r#"const PROVIDER_CATALOGS: &[(&str, &str)] = &[("openai", include_str!("../../data/providers/openai.json"))];"#;
        fs::write(providers.join("model_catalogs_generated.rs"), include).unwrap();
        fs::write(data.join("openai.json"), &old).unwrap();
        fs::write(data.join(".manifest.json"), &manifest).unwrap();
        let expected=source.request(&json!({"operation":"generateModels","fault":fault,"initialFiles":{"src/models.generated.ts":"import { OPENAI_MODELS } from \"./providers/openai.models.ts\";\n","src/providers/openai.models.ts":"old shard","src/providers/data/openai.json":old,"src/providers/data/.manifest.json":manifest},"http":{"https://models.dev/api.json":{"body":"{}"},"https://openrouter.ai/api/v1/models":{"body":"{\"data\":[]}"},"https://ai-gateway.vercel.sh/v1/models":{"body":"{\"data\":[]}"}}}));
        let proof = PathBuf::from("/tmp/pi-rust-model-generator-proof");
        fs::create_dir_all(&proof).unwrap();
        fs::write(
            proof.join(format!("fault-{fault}-source.json")),
            serde_json::to_string_pretty(&expected).unwrap(),
        )
        .unwrap();
        assert!(
            expected.get("error").is_some(),
            "source fault did not fail: {fault} {expected}"
        );
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: if url == "https://models.dev/api.json" {
                    "{}".into()
                } else {
                    "{\"data\":[]}".into()
                },
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        let mut filesystem = FaultFs {
            root: root.0.clone(),
            fault,
            injected: false,
            native: NativeCatalogFileSystem,
        };
        let actual =
            generator.generate_with_filesystem(&root.0, &JsString::from("2026-10-20T00:00:00.000Z"), &mut filesystem);
        match actual {
            Ok(_) => failures.push(format!("{fault}: native unexpectedly succeeded")),
            Err(error) => {
                let native = normalized_error(&error.message.to_utf8_lossy(), root.0.to_str().unwrap(), false);
                let source = normalized_error(
                    expected["error"]["message"].as_str().unwrap(),
                    expected["root"].as_str().unwrap(),
                    true,
                );
                if error.name != expected["error"]["name"].as_str().unwrap() || native != source {
                    failures.push(format!(
                        "{fault}: native {} {native}; source {} {source}",
                        error.name, expected["error"]["name"]
                    ));
                }
                if let Some(code) = expected["error"].get("code") {
                    if let Some(GeneratorErrorDetails::FileSystem { record, destination }) = error.details {
                        if record.code != code.as_str().unwrap()
                            || record.errno != expected["error"]["errno"].as_i64().map(|v| v as i32)
                            || record.syscall != expected["error"]["syscall"].as_str().unwrap()
                        {
                            failures.push(format!("{fault}: filesystem metadata differs {record:?}"));
                        }
                        for (name, value) in [("path", record.path), ("dest", destination)] {
                            if let Some(source) = expected["error"][name].as_str() {
                                if value
                                    .as_ref()
                                    .map(|value| normalized_error(value, root.0.to_str().unwrap(), false))
                                    != Some(normalized_error(source, expected["root"].as_str().unwrap(), true))
                                {
                                    failures.push(format!("{fault}: {name} differs"));
                                }
                            }
                        }
                    } else {
                        failures.push(format!("{fault}: missing filesystem metadata"));
                    }
                }
            }
        }
        if fault != "cleanup" {
            for (name, content) in [("openai.json", old.as_str()), (".manifest.json", manifest.as_str())] {
                if fs::read_to_string(data.join(name)).unwrap() != content {
                    failures.push(format!("{fault}: previous {name} not restored"));
                }
            }
            if fs::read_to_string(providers.join("model_catalogs_generated.rs")).unwrap() != include {
                failures.push(format!("{fault}: include not restored"));
            }
        }
        let staging = fs::read_dir(&providers)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".model-generation-")
            })
            .count();
        if staging != usize::from(fault == "cleanup") {
            failures.push(format!("{fault}: unexpected staging leftovers {staging}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_cli_streams_source_logs_and_rejects_invalid_arguments_without_fetching() {
    use pi_ai::model_catalog_generator::run_generator_cli;
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let expected=source.request(&json!({"operation":"cli","argv":["--json-only","--json-output","@OUTPUT@"],"http":{"https://models.dev/api.json":{"body":"{}"},"https://openrouter.ai/api/v1/models":{"body":"{\"data\":[]}"},"https://ai-gateway.vercel.sh/v1/models":{"body":"{\"data\":[]}"}}}));
    assert_eq!(expected["exitCode"], json!(0));
    let root = Sandbox::new();
    let output = root.0.join("catalog");
    let mut logs = vec![];
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: if url == "https://models.dev/api.json" {
                "{}".into()
            } else {
                "{\"data\":[]}".into()
            },
        })
    };
    run_generator_cli(
        &[
            "--json-only".into(),
            "--json-output".into(),
            output.to_string_lossy().into_owned(),
        ],
        &root.0,
        &JsString::from("2026-10-20T00:00:00.000Z"),
        &mut fetch,
        &mut |log| logs.push(log.clone()),
    )
    .unwrap();
    let native = pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::Array(logs))
        .replace(root.0.to_str().unwrap(), "<ROOT>");
    let expected_logs = expected["logs"]
        .to_string()
        .replace(expected["root"].as_str().unwrap(), "<ROOT>");
    assert_eq!(native, expected_logs);
    let error = run_generator_cli(
        &["--json-only".into()],
        &root.0,
        &JsString::from("unused"),
        &mut |_| panic!("invalid CLI must not fetch"),
        &mut |_| panic!("invalid CLI must not log"),
    )
    .unwrap_err();
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, JsString::from("--json-only requires --json-output"));
    let binary = std::process::Command::new(env!("CARGO_BIN_EXE_generate-models"))
        .arg("--json-only")
        .output()
        .unwrap();
    assert_eq!(binary.status.code(), Some(1));
    assert!(binary.stdout.is_empty());
    assert_eq!(
        String::from_utf8(binary.stderr).unwrap(),
        "Error: --json-only requires --json-output\n"
    );
}

#[test]
fn native_include_move_preserves_every_embedded_catalog_value() {
    use pi_ai::{providers::models_data, types::Model};
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/providers");
    let mut expected_ids = vec![];
    for entry in fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if name == ".manifest.json" {
            continue;
        }
        let provider = name.strip_suffix(".json").unwrap();
        expected_ids.push(provider.to_owned());
        let groups: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut expected = indexmap::IndexMap::new();
        for group in groups.values() {
            for (id, value) in group.as_object().unwrap() {
                expected.insert(id.clone(), serde_json::from_value::<Model>(value.clone()).unwrap());
            }
        }
        assert_eq!(
            models_data::provider_models(provider).unwrap(),
            expected.into_values().collect::<Vec<_>>(),
            "{provider}"
        );
    }
    expected_ids.sort();
    assert_eq!(models_data::builtin_provider_ids(), expected_ids);
}

#[test]
fn source_artifacts_manifest_pretty_modes_and_native_include_compile_match() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for (data_only, json_only, public, pretty) in [
        (false, false, false, false),
        (false, false, true, true),
        (false, true, true, false),
        (true, false, false, true),
    ] {
        let root = Sandbox::new();
        let providers = root.0.join("crates/pi-ai/src/providers");
        fs::create_dir_all(&providers).unwrap();
        let include = "const PROVIDER_CATALOGS: &[(&str, &str)] = &[(\"openai\", include_str!(\"../../data/providers/openai.json\"))];\n";
        fs::write(providers.join("model_catalogs_generated.rs"), include).unwrap();
        let data = root.0.join("crates/pi-ai/data/providers");
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("old.json"), "old data").unwrap();
        let mut argv = vec![];
        if data_only {
            argv.push("--data-only");
        }
        if json_only {
            argv.push("--json-only");
        }
        if public {
            argv.extend(["--json-output", "@OUTPUT@"]);
        }
        if pretty {
            argv.push("--pretty");
        }
        let expected=source.request(&json!({"operation":"generateModels","argv":argv,"initialFiles":{"src/models.generated.ts":"import { OPENAI_MODELS } from \"./providers/openai.models.ts\";\n","src/providers/openai.models.ts":"old shard","src/providers/data/old.json":"old data"},"http":{"https://models.dev/api.json":{"body":"{}"},"https://openrouter.ai/api/v1/models":{"body":"{\"data\":[]}"},"https://ai-gateway.vercel.sh/v1/models":{"body":"{\"data\":[]}"}}}));
        assert!(expected.get("error").is_none(), "source artifacts failed: {expected}");
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: if url == "https://models.dev/api.json" {
                    "{}".into()
                } else {
                    "{\"data\":[]}".into()
                },
            })
        };
        let mut generator = Generator::new(
            GeneratorOptions {
                data_only,
                json_only,
                pretty,
                json_output_dir: public.then(|| root.0.join("catalog")),
                ..Default::default()
            },
            &mut fetch,
        );
        generator
            .generate(&root.0, &JsString::from("2026-10-20T00:00:00.000Z"))
            .unwrap();
        for (path, content) in expected["files"].as_object().unwrap() {
            let native = if let Some(path) = path.strip_prefix("src/providers/data/") {
                data.join(path)
            } else if path.starts_with("catalog/") {
                root.0.join(path)
            } else {
                continue;
            };
            assert_eq!(
                fs::read_to_string(&native).unwrap_or_else(|error| panic!("{}: {error}", native.display())),
                content.as_str().unwrap(),
                "{path}"
            );
        }
        assert_eq!(data.join("old.json").exists(), json_only);
        if data_only || json_only {
            assert_eq!(
                fs::read_to_string(providers.join("model_catalogs_generated.rs")).unwrap(),
                include
            );
        } else {
            let wrapper = providers.join("probe.rs");
            fs::write(&wrapper,"include!(\"model_catalogs_generated.rs\"); fn main() { for (id, data) in PROVIDER_CATALOGS { println!(\"{}\\t{}\", id, data); } }").unwrap();
            let binary = root.0.join("probe");
            let compiled = std::process::Command::new("rustc")
                .args(["--edition=2024"])
                .arg(&wrapper)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let output = std::process::Command::new(binary).output().unwrap();
            assert!(output.status.success());
            let output = String::from_utf8(output.stdout).unwrap();
            let mut ids = vec![];
            for line in output.lines() {
                if let Some((id, _)) = line.split_once('\t') {
                    ids.push(id.to_owned());
                }
            }
            let expected_ids = fs::read_dir(&data)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name != ".manifest.json")
                .map(|name| name.trim_end_matches(".json").to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(ids.into_iter().collect::<std::collections::BTreeSet<_>>(), expected_ids);
        }
    }
}
