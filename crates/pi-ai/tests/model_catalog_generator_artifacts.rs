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
