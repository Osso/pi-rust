use crate::shared_oracle as oracle;
use pi_ai::model_catalog_generator::{
    CatalogFileSystem, Generator, GeneratorError, HttpResponse, NativeCatalogFileSystem,
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue, parse_lossless, stringify_lossless},
};
use std::path::{Path, PathBuf};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pi-generator-fs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in [
            "src/providers",
            "crates/pi-ai/data/providers",
            "crates/pi-ai/src/providers",
        ] {
            std::fs::create_dir_all(path.join(directory)).unwrap();
        }
        Self(path)
    }
    fn source(&self) -> PathBuf {
        self.0.join("src/providers")
    }
    fn native(&self) -> PathBuf {
        self.0.join("crates/pi-ai/data/providers")
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct FaultFilesystem {
    write: usize,
    fail_write: usize,
}
impl CatalogFileSystem for FaultFilesystem {
    fn read_dir(&mut self, path: &Path) -> Result<Vec<JsString>, GeneratorError> {
        NativeCatalogFileSystem.read_dir(path)
    }
    fn remove(&mut self, path: &Path) -> Result<(), GeneratorError> {
        NativeCatalogFileSystem.remove(path)
    }
    fn write(&mut self, path: &Path, content: &str) -> Result<(), GeneratorError> {
        self.write += 1;
        if self.write == self.fail_write {
            std::fs::create_dir(path).unwrap();
        }
        NativeCatalogFileSystem.write(path, content)
    }
}
fn normalized(value: &JsValue, root: &Path) -> JsValue {
    let text = stringify_lossless(value)
        .replace(
            &format!(
                "{}/crates/pi-ai/src/providers/model_catalogs_generated.rs",
                root.display()
            ),
            "<ROOT>/src/models.generated.ts",
        )
        .replace(
            &format!("{}/crates/pi-ai/data/providers/", root.display()),
            "<ROOT>/src/providers/",
        )
        .replace(root.to_str().unwrap(), "<ROOT>")
        .replace(".json", ".models.ts");
    parse_lossless(text).unwrap()
}
fn responses() -> JsValue {
    js_json!({"https://models.dev/api.json":{"body":{}},"https://openrouter.ai/api/v1/models":{"body":{"data":[]}},"https://ai-gateway.vercel.sh/v1/models":{"body":{"data":[]}}})
}

// Frozen generateModels2190–2218 deletes stale catalogs, writes each provider,
// then the aggregator. Real EISDIR at a write leaves earlier mutations intact.
#[test]
fn real_filesystem_failures_preserve_frozen_error_fields_and_partial_writes() {
    let mut oracle = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let responses = responses();
    let success = oracle.request(&js_json!({"operation":"workflow","responses":responses}));
    assert_eq!(success["ok"], js_json!(true));
    let total_writes = success["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event[0] == js_json!("write"))
        .count();
    for boundary in [1, 3, total_writes] {
        let source = Sandbox::new();
        let native = Sandbox::new();
        std::fs::write(source.source().join("obsolete.models.ts"), "old").unwrap();
        std::fs::write(native.native().join("obsolete.json"), "old").unwrap();
        let expected=oracle.request(&js_json!({"operation":"workflow","responses":responses,"filesystemRoot":source.0.to_str().unwrap(),"failWrite":boundary}));
        assert_eq!(expected["ok"], js_json!(false), "{expected:?}");
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: stringify_lossless(&responses[url]["body"]),
            })
        };
        let mut generator = Generator::new(&mut fetch);
        let error = generator
            .generate_with_filesystem(
                &native.0,
                &mut FaultFilesystem {
                    write: 0,
                    fail_write: boundary,
                },
            )
            .unwrap_err();
        assert_eq!(
            normalized(&error.thrown_value().value(), &native.0),
            normalized(&expected["error"], &source.0)
        );
        assert!(!native.native().join("obsolete.json").exists());
        assert!(!source.source().join("obsolete.models.ts").exists());
        let completed = expected["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event[0] == js_json!("write"))
            .count();
        assert_eq!(completed, boundary - 1);
        let native_written = std::fs::read_dir(native.native())
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().path().is_file())
            .count();
        assert_eq!(native_written, boundary - 1);
        assert!(
            !generator
                .logs
                .iter()
                .any(|event| event[1] == js_json!("\nModel Statistics:"))
        );
    }
}
