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
            "pi-repin-generator-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Default)]
struct RecordingFilesystem {
    events: Vec<(String, PathBuf)>,
}
impl CatalogFileSystem for RecordingFilesystem {
    fn read_dir(&mut self, path: &Path) -> Result<Vec<JsString>, GeneratorError> {
        NativeCatalogFileSystem.read_dir(path)
    }
    fn write(&mut self, path: &Path, content: &str) -> Result<(), GeneratorError> {
        self.events.push(("write".into(), path.into()));
        NativeCatalogFileSystem.write(path, content)
    }
    fn remove(&mut self, path: &Path) -> Result<(), GeneratorError> {
        self.events.push(("remove".into(), path.into()));
        NativeCatalogFileSystem.remove(path)
    }
}

#[test]
fn artifacts_delete_old_before_writes_and_roundtrip_flat_provider_records() {
    let root = Sandbox::new();
    let providers = root.path().join("crates/pi-ai/data/providers");
    let source = root.path().join("crates/pi-ai/src/providers");
    std::fs::create_dir_all(&providers).unwrap();
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(providers.join("obsolete.json"), "old").unwrap();
    std::fs::write(providers.join(".manifest.json"), "old").unwrap();
    std::fs::write(providers.join("keep.txt"), "keep").unwrap();
    std::fs::write(source.join("model_catalogs_generated.rs"), "old").unwrap();
    let raw = JsString::from("fixture");
    let body = js_json!({"data":[{"id":raw,"name":raw,"supported_parameters":["tools"],"architecture":{"modality":"text"},"pricing":{}}]});
    let responses = js_json!({"https://models.dev/api.json":{"body":{}},"https://openrouter.ai/api/v1/models":{"body":body},"https://ai-gateway.vercel.sh/v1/models":{"body":{"data":[]}}});
    let source_root = Sandbox::new();
    std::fs::create_dir_all(source_root.path().join("src/providers")).unwrap();
    let mut oracle = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected = oracle.request(
        &js_json!({"operation":"emission","responses":responses,"filesystemRoot":source_root.path().to_str().unwrap()}),
    );
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&responses[url]["body"]),
        })
    };
    let mut generator = Generator::new(&mut fetch);
    let mut filesystem = RecordingFilesystem::default();
    let catalog = generator
        .generate_with_filesystem(root.path(), &mut filesystem)
        .unwrap();
    assert_eq!(catalog, expected["catalog"]);
    assert!(!providers.join("obsolete.json").exists());
    assert!(
        !providers.join(".manifest.json").exists(),
        "frozen generator has no manifest"
    );
    assert_eq!(std::fs::read_to_string(providers.join("keep.txt")).unwrap(), "keep");
    let first_write = filesystem
        .events
        .iter()
        .position(|(operation, _)| operation == "write")
        .unwrap();
    let last_remove = filesystem
        .events
        .iter()
        .rposition(|(operation, _)| operation == "remove")
        .unwrap();
    assert!(
        last_remove < first_write,
        "old catalogs must be deleted before any new provider/include writes: {:?}",
        filesystem.events
    );
    let grouped = parse_lossless(std::fs::read_to_string(providers.join("openrouter.json")).unwrap()).unwrap();
    let actual = grouped.as_object().unwrap().get(&raw).unwrap();
    let source_provider = source_root.path().join("src/providers/openrouter.models.ts");
    let expected_model = expected["loaded"][source_provider.to_str().unwrap()]
        .as_object()
        .unwrap()
        .get(&raw)
        .unwrap();
    assert_eq!(stringify_lossless(actual), stringify_lossless(expected_model));
    assert_eq!(actual["id"], JsValue::String(raw.clone()));
    assert_eq!(actual["name"], JsValue::String(raw));
    let expected_stats = expected["logs"]
        .as_array()
        .unwrap()
        .iter()
        .skip_while(|log| log[1] != JsValue::String(JsString::from("\nModel Statistics:")))
        .cloned()
        .collect::<Vec<_>>();
    let actual_stats = generator
        .logs
        .into_iter()
        .skip_while(|log| log[1] != JsValue::String(JsString::from("\nModel Statistics:")))
        .collect::<Vec<_>>();
    assert_eq!(actual_stats, expected_stats);
}
