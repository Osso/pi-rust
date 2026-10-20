#[path = "support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use oracle::TsOracle;
use pi_ai::model_data_tools::*;
use serde_json::{Value, json};

const MODULE: &str = "packages/ai/scripts/model-data.ts";
const GENERATED_AT: &str = "2026-07-23T10:00:00.000Z";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    dir: PathBuf,
    structure: ModelDataStructure,
    groups: Value,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "pi-model-data-tools-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("src/providers/data");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            root.join("src/models.generated.ts"),
            "import { TEST_PROVIDER_MODELS } from \"./providers/test-provider.models.ts\";\n",
        )
        .unwrap();
        fs::write(root.join("src/providers/test-provider.models.ts"), "fixture shard\n").unwrap();
        let fixture = Self {
            root,
            dir,
            structure: serde_json::from_value(json!({"test-provider":{"model-a":"openai-completions"}})).unwrap(),
            groups: json!({"openai-completions":{"model-a":{
                "id":"model-a", "provider":"test-provider", "api":"openai-completions", "name":"Model A", "baseUrl":"https://example.test/v1", "reasoning":false, "input":["text"], "contextWindow":1000, "maxTokens":100, "cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}
            }}}),
        };
        fixture.write();
        fixture
    }
    fn write(&self) {
        let content = format!("{}\n", self.groups);
        fs::write(self.dir.join("test-provider.json"), &content).unwrap();
        let files = [("test-provider.json".to_owned(), content)].into_iter().collect();
        let manifest = create_model_data_manifest(&self.structure, &files, GENERATED_AT);
        fs::write(
            self.dir.join(MODEL_DATA_MANIFEST_FILE),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
    }
    fn manifest(&self, mutate: impl FnOnce(&mut Value)) {
        let path = self.dir.join(MODEL_DATA_MANIFEST_FILE);
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        mutate(&mut manifest);
        fs::write(path, manifest.to_string()).unwrap();
    }
    fn compare(&self, oracle: &mut TsOracle) -> Value {
        let actual = result(validate_model_data_directory(&self.structure, &self.dir).map(|()| Value::Null));
        let expected = oracle.request(&json!({"op":"directory","structure":self.structure,"dir":self.dir}));
        assert_eq!(actual, expected);
        actual
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn result(value: Result<Value, ModelDataError>) -> Value {
    match value {
        Ok(output) => json!({"ok":true,"output":output}),
        Err(error) => json!({"ok":false,"error":error.to_string()}),
    }
}
fn expect_error(value: &Value, text: &str) {
    assert!(value["error"].as_str().unwrap().contains(text), "{value}");
}

#[test]
fn exact_ids_and_manifest_hashes_match_ts_without_reordering_responses() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    for (expected, actual) in [
        (vec!["a", "b"], vec!["a"]),
        (vec!["a"], vec!["a", "b"]),
        (vec!["a", "a"], vec!["a"]),
        (vec!["😀", "\u{e000}", "2", "10"], vec!["z"]),
    ] {
        let actual_result =
            result(assert_exact_model_ids("test", expected.clone(), actual.clone()).map(|()| Value::Null));
        assert_eq!(
            actual_result,
            oracle.request(&json!({"op":"ids","label":"test","expected":expected,"actual":actual}))
        );
    }
    let structure: ModelDataStructure = serde_json::from_value(
        json!({"z":{"😀":"api","\u{e000}":"api","10":"b","2":"a"},"a":{"b":"x"},"10":{},"2":{}}),
    )
    .unwrap();
    let files = serde_json::from_value(json!({"😀.json":"one\r\n","\u{e000}.json":"é\n","2":"x","10":"y"})).unwrap();
    let manifest = create_model_data_manifest(&structure, &files, GENERATED_AT);
    assert_eq!(
        json!({"ok":true,"output":manifest}),
        oracle.request(&json!({"op":"manifest","structure":structure,"files":files,"generatedAt":GENERATED_AT}))
    );
    assert_eq!(
        json!({"ok":true,"output":model_data_structure_hash(&structure)}),
        oracle.request(&json!({"op":"hash","structure":structure}))
    );
}

#[test]
fn filesystem_structure_and_checker_success_match_pinned_modules() {
    let fixture = Fixture::new();
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    assert_eq!(MODEL_DATA_SCHEMA_VERSION, 3);
    assert_eq!(read_model_data_structure(&fixture.root).unwrap(), fixture.structure);
    for (op, actual) in [
        (
            "providers",
            result(read_model_data_provider_ids(&fixture.root).map(|v| json!(v))),
        ),
        (
            "structure",
            result(read_model_data_structure(&fixture.root).map(|v| json!(v))),
        ),
        (
            "generated",
            result(validate_generated_model_data(&fixture.root).map(|()| Value::Null)),
        ),
    ] {
        assert_eq!(actual, oracle.request(&json!({"op":op,"root":fixture.root})));
    }
    assert_eq!(fixture.compare(&mut oracle), json!({"ok":true,"output":null}));
    let output = Command::new(env!("CARGO_BIN_EXE_check-model-data"))
        .arg(&fixture.root)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Generated model data is valid.\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn model_validation_branches_accumulate_in_source_order() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    for (field, values, message) in [
        ("id", vec![json!("wrong"), Value::Null], "has id"),
        ("provider", vec![json!("wrong"), Value::Null], "has provider"),
        ("api", vec![json!("wrong"), Value::Null], "has api"),
        ("name", vec![json!(""), json!(3)], "no model name"),
        ("baseUrl", vec![Value::Null], "no baseUrl string"),
        ("reasoning", vec![json!("false")], "no reasoning boolean"),
        (
            "input",
            vec![json!([]), json!(["audio"]), Value::Null],
            "invalid input modalities",
        ),
        (
            "contextWindow",
            vec![json!(0), json!(-1), json!("1")],
            "invalid contextWindow",
        ),
        ("maxTokens", vec![json!(0), Value::Null], "invalid maxTokens"),
        ("cost", vec![Value::Null, json!({"input":1})], "cost"),
    ] {
        for value in values {
            let mut fixture = Fixture::new();
            fixture.groups["openai-completions"]["model-a"][field] = value;
            fixture.write();
            expect_error(&fixture.compare(&mut oracle), message);
        }
    }
    let mut fixture = Fixture::new();
    fixture.groups["openai-completions"]["model-a"] = json!({});
    fixture.write();
    expect_error(&fixture.compare(&mut oracle), "has id undefined");
    for value in [Value::Null, json!([]), json!(4)] {
        fixture.groups["openai-completions"]["model-a"] = value;
        fixture.write();
        expect_error(&fixture.compare(&mut oracle), "must be an object");
    }
    fixture.groups = json!({"openai-completions":null,"other":{"model-a":{},"extra":{}},"duplicate":{"model-a":{}}});
    fixture.write();
    expect_error(&fixture.compare(&mut oracle), "more than one API group");
    fixture.groups = json!({"x":{}});
    for n in 0..8 {
        fixture.groups["x"]
            .as_object_mut()
            .unwrap()
            .insert(format!("m-{n}"), json!({}));
    }
    fixture.write();
    expect_error(&fixture.compare(&mut oracle), "... and ");
}

#[test]
fn manifest_directory_and_json_failures_match_ts() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    for (field, value, message) in [
        ("schemaVersion", json!(4), "model data schema"),
        ("generatedAt", json!("invalid"), "invalid generation timestamp"),
        ("generatedAt", Value::Null, "invalid generation timestamp"),
        ("structureHash", json!("stale"), "generation stamp"),
        ("files", Value::Null, "no file hashes"),
        ("files", json!({"extra.json":"x"}), "manifest file hashes do not match"),
    ] {
        let fixture = Fixture::new();
        fixture.manifest(|m| m[field] = value);
        expect_error(&fixture.compare(&mut oracle), message);
    }
    for timestamp in [
        "2026-07-23",
        "0",
        "Thu, 23 Jul 2026 10:00:00 GMT",
        "2026-02-30T00:00:00Z",
    ] {
        let fixture = Fixture::new();
        fixture.manifest(|m| m["generatedAt"] = json!(timestamp));
        fixture.compare(&mut oracle);
    }
    for content in ["[]", "null", "{", "{\"x\":}", "{} trailing"] {
        let fixture = Fixture::new();
        fs::write(fixture.dir.join("test-provider.json"), content).unwrap();
        fixture.compare(&mut oracle);
        fs::write(fixture.dir.join(MODEL_DATA_MANIFEST_FILE), content).unwrap();
        fixture.compare(&mut oracle);
    }
    let fixture = Fixture::new();
    fs::remove_file(fixture.dir.join("test-provider.json")).unwrap();
    fs::write(fixture.dir.join("extra.json"), "{}").unwrap();
    expect_error(
        &fixture.compare(&mut oracle),
        "missing: test-provider.json; extra: extra.json",
    );
    fs::remove_file(fixture.dir.join(MODEL_DATA_MANIFEST_FILE)).unwrap();
    expect_error(&fixture.compare(&mut oracle), "ENOENT");
    fs::remove_dir_all(&fixture.dir).unwrap();
    expect_error(&fixture.compare(&mut oracle), "directory does not exist");
    fs::write(&fixture.dir, "file").unwrap();
    expect_error(&fixture.compare(&mut oracle), "directory does not exist");
}

#[test]
fn aggregator_shards_and_provider_structure_failures_match_ts() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    for imports in [
        "",
        "import { lower_MODELS } from \"./providers/no.models.ts\";\n",
        "import { TEST_PROVIDER_MODELS } from \"./providers/test-provider.models.ts\";\nimport { TEST_PROVIDER_MODELS } from \"./providers/test-provider.models.ts\";\n",
        "import { MISSING_MODELS } from \"./providers/missing.models.ts\";\n",
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("src/models.generated.ts"), imports).unwrap();
        assert_eq!(
            result(read_model_data_structure(&fixture.root).map(|v| json!(v))),
            oracle.request(&json!({"op":"structure","root":fixture.root}))
        );
    }
    for content in [
        "{}",
        "[]",
        "{",
        "{\"api\":null}",
        "{\"api\":{\"a\":{}},\"other\":{\"a\":{}}}",
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.dir.join("test-provider.json"), content).unwrap();
        assert_eq!(
            result(read_model_data_structure(&fixture.root).map(|v| json!(v))),
            oracle.request(&json!({"op":"structure","root":fixture.root}))
        );
    }
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/providers/extra.models.ts"), "").unwrap();
    assert_eq!(
        result(read_model_data_structure(&fixture.root).map(|v| json!(v))),
        oracle.request(&json!({"op":"structure","root":fixture.root}))
    );
    fs::remove_file(fixture.root.join("src/models.generated.ts")).unwrap();
    assert_eq!(
        result(read_model_data_provider_ids(&fixture.root).map(|v| json!(v))),
        oracle.request(&json!({"op":"providers","root":fixture.root}))
    );
}

#[test]
fn checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures() {
    let fixture = Fixture::new();
    let mut oracle = TsOracle::spawn_with_environment(
        "model-data-oracle.ts",
        MODULE,
        &[("NODE_OPTIONS", "--experimental-vm-modules".into())],
    );
    for valid in [true, false] {
        if !valid {
            fixture.manifest(|m| m["schemaVersion"] = json!(99));
        }
        let output = Command::new(env!("CARGO_BIN_EXE_check-model-data"))
            .arg(&fixture.root)
            .output()
            .unwrap();
        let actual = json!({"ok":true,"output":{"code":output.status.code(),"stdout":String::from_utf8(output.stdout).unwrap(),"stderr":String::from_utf8(output.stderr).unwrap()}});
        assert_eq!(actual, oracle.request(&json!({"op":"check","root":fixture.root})));
        println!("fixture check valid={valid}: {}", actual["output"]);
    }
}

#[test]
fn import_line_terminators_match_js() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    for newline in ["\n", "\r\n", "\r", "\u{2028}", "\u{2029}"] {
        let fixture = Fixture::new();
        let imports = format!(
            "// fixture{newline}import {{ TEST_PROVIDER_MODELS }} from \"./providers/test-provider.models.ts\";{newline}"
        );
        fs::write(fixture.root.join("src/models.generated.ts"), imports).unwrap();
        assert_eq!(
            result(read_model_data_provider_ids(&fixture.root).map(|v| json!(v))),
            oracle.request(&json!({"op":"providers","root":fixture.root})),
            "terminator {newline:?}"
        );
    }
}

#[test]
fn manifest_record_order_matches_js() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    let manifest = create_model_data_manifest(
        &ModelDataStructure::new(),
        &serde_json::from_value(json!({"10":"x","2":"y","😀":"z","\u{e000}":"q"})).unwrap(),
        GENERATED_AT,
    );
    let expected=oracle.request(&json!({"op":"manifest","structure":{},"files":{"10":"x","2":"y","😀":"z","\u{e000}":"q"},"generatedAt":GENERATED_AT}));
    assert_eq!(
        serde_json::to_string(&manifest).unwrap(),
        serde_json::to_string(&expected["output"]).unwrap()
    );
}
