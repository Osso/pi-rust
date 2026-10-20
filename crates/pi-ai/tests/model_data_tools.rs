#[path = "support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use oracle::TsOracle;
use pi_ai::model_data_tools::*;
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::{Value, json};
fn structure(v: Value) -> ModelDataStructure {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (JsString::from(k.as_str()), strings(v.clone())))
        .collect()
}
fn strings(v: Value) -> indexmap::IndexMap<JsString, JsString> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (JsString::from(k.as_str()), JsString::from(v.as_str().unwrap())))
        .collect()
}
fn structure_serde(v: &ModelDataStructure) -> Value {
    model_data_structure_json(v).into_serde_lossy()
}
fn strings_serde(v: &indexmap::IndexMap<JsString, JsString>) -> Value {
    JsValue::Object(v.iter().map(|(k, v)| (k.clone(), JsValue::String(v.clone()))).collect()).into_serde_lossy()
}
fn encoded(v: Result<JsValue, ModelDataError>) -> String {
    let fields = match v {
        Ok(output) => vec![("ok".into(), JsValue::Bool(true)), ("output".into(), output)],
        Err(e) => vec![
            ("ok".into(), JsValue::Bool(false)),
            ("error".into(), JsValue::String(e.message().clone())),
        ],
    };
    js_json::stringify_lossless(&JsValue::Object(fields.into_iter().collect()))
}

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
            structure: structure(json!({"test-provider":{"model-a":"openai-completions"}})),
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
        let files = [(JsString::from("test-provider.json"), JsString::from(content.as_str()))]
            .into_iter()
            .collect();
        let manifest = create_model_data_manifest(&self.structure, &files, &JsString::from(GENERATED_AT));
        fs::write(self.dir.join(MODEL_DATA_MANIFEST_FILE), manifest.stringify()).unwrap();
    }
    fn manifest(&self, mutate: impl FnOnce(&mut Value)) {
        let path = self.dir.join(MODEL_DATA_MANIFEST_FILE);
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        mutate(&mut manifest);
        fs::write(path, manifest.to_string()).unwrap();
    }
    fn compare(&self, oracle: &mut TsOracle) -> Value {
        let actual = result(validate_model_data_directory(&self.structure, &self.dir).map(|()| Value::Null));
        let expected =
            oracle.request(&json!({"op":"directory","structure":structure_serde(&self.structure),"dir":self.dir}));
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
        let actual_result = result(
            assert_exact_model_ids(&JsString::from("test"), expected.clone(), actual.clone()).map(|()| Value::Null),
        );
        assert_eq!(
            actual_result,
            oracle.request(&json!({"op":"ids","label":"test","expected":expected,"actual":actual}))
        );
    }
    let structure: ModelDataStructure =
        structure(json!({"z":{"😀":"api","\u{e000}":"api","10":"b","2":"a"},"a":{"b":"x"},"10":{},"2":{}}));
    let files = strings(json!({"😀.json":"one\r\n","\u{e000}.json":"é\n","2":"x","10":"y"}));
    let manifest = create_model_data_manifest(&structure, &files, &JsString::from(GENERATED_AT));
    assert_eq!(
        json!({"ok":true,"output":manifest.to_json().into_serde_lossy()}),
        oracle.request(&json!({"op":"manifest","structure":structure_serde(&structure),"files":strings_serde(&files),"generatedAt":GENERATED_AT}))
    );
    assert_eq!(
        json!({"ok":true,"output":model_data_structure_hash(&structure)}),
        oracle.request(&json!({"op":"hash","structure":structure_serde(&structure)}))
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
            result(
                read_model_data_provider_ids(&fixture.root)
                    .map(|v| json!(v.iter().map(|s| s.to_utf8().unwrap()).collect::<Vec<_>>())),
            ),
        ),
        (
            "structure",
            result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
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
        "Jan 1 2020",
        "1/1/2020",
        "Fri, 01 Jan 1960 00:00:00 GMT",
        "Fri, 01 Jan 1960 00:00:60 GMT",
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
            result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
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
            result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
            oracle.request(&json!({"op":"structure","root":fixture.root}))
        );
    }
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/providers/extra.models.ts"), "").unwrap();
    assert_eq!(
        result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
        oracle.request(&json!({"op":"structure","root":fixture.root}))
    );
    fs::remove_file(fixture.root.join("src/models.generated.ts")).unwrap();
    assert_eq!(
        result(
            read_model_data_provider_ids(&fixture.root)
                .map(|v| json!(v.iter().map(|s| s.to_utf8().unwrap()).collect::<Vec<_>>()))
        ),
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
            result(
                read_model_data_provider_ids(&fixture.root)
                    .map(|v| json!(v.iter().map(|s| s.to_utf8().unwrap()).collect::<Vec<_>>()))
            ),
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
        &strings(json!({"10":"x","2":"y","😀":"z","\u{e000}":"q"})),
        &JsString::from(GENERATED_AT),
    );
    let expected=oracle.request(&json!({"op":"manifest","structure":{},"files":{"10":"x","2":"y","😀":"z","\u{e000}":"q"},"generatedAt":GENERATED_AT}));
    assert_eq!(
        manifest.stringify(),
        serde_json::to_string(&expected["output"]).unwrap()
    );
}

#[test]
fn oracle_flag_watchdog_owner() {
    if std::env::var_os("PI_MODEL_DATA_WATCHDOG_OWNER").is_none() {
        return;
    }
    let mut oracle = TsOracle::spawn_with_environment(
        "model-data-oracle.ts",
        MODULE,
        &[("NODE_OPTIONS", "--experimental-vm-modules".into())],
    );
    let response = oracle.request(&json!({"op":"watchdog_probe"}));
    assert_eq!(response["ok"], true);
    assert_eq!(response["output"]["callerFlag"], true);
    let pid = rustix::process::Pid::from_raw(response["output"]["pid"].as_i64().unwrap() as i32).unwrap();
    let group = rustix::process::getpgid(Some(pid)).unwrap();
    println!("WATCHDOG_ORACLE_PID={}", pid.as_raw_nonzero());
    println!("WATCHDOG_GROUP_PID={}", group.as_raw_nonzero());
    // Deliberately bypass Drop: the watchdog must reap the orphan process group.
    std::process::exit(0);
}

fn process_running(pid: i32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Ok(stat) => !stat.split_once(") ").is_some_and(|(_, tail)| tail.starts_with('Z')),
        Err(error) => panic!("cannot inspect oracle lifetime: {error}"),
    }
}

#[test]
fn caller_node_flag_and_parent_death_watchdog_both_work() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "oracle_flag_watchdog_owner", "--nocapture"])
        .env("PI_MODEL_DATA_WATCHDOG_OWNER", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let pid: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("WATCHDOG_ORACLE_PID="))
        .expect("oracle pid")
        .parse()
        .unwrap();
    let group: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("WATCHDOG_GROUP_PID="))
        .expect("oracle process-group leader pid")
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while process_running(pid) || process_running(group) {
        if std::time::Instant::now() >= deadline {
            use rustix::process::{Pid, Signal, getpgid, kill_process, kill_process_group};
            let pid = Pid::from_raw(pid).unwrap();
            if let Ok(group) = getpgid(Some(pid)) {
                let _ = kill_process_group(group, Signal::KILL);
            }
            let _ = kill_process(pid, Signal::KILL);
            panic!("caller flag worked but parent watchdog failed to terminate orphan oracle");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn json_model_identity_with_unpaired_surrogate_matches_ts() {
    let mut fixture = Fixture::new();
    fixture.groups["openai-completions"]["model-a"]["id"] = json!("SURROGATE_FIXTURE");
    fixture.write();
    let path = fixture.dir.join("test-provider.json");
    let content = fs::read_to_string(&path)
        .unwrap()
        .replace("SURROGATE_FIXTURE", "\\ud800");
    fs::write(path, content).unwrap();
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    fixture.compare(&mut oracle);
}

#[test]
fn filesystem_io_failures_and_ignored_non_catalog_entries_match_ts() {
    let fixture = Fixture::new();
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    fs::write(fixture.dir.join("notes.txt"), "not catalog data").unwrap();
    fs::write(fixture.root.join("src/providers/notes.ts"), "not a shard").unwrap();
    assert_eq!(fixture.compare(&mut oracle), json!({"ok":true,"output":null}));
    let data = fixture.dir.join("test-provider.json");
    fs::remove_file(&data).unwrap();
    fs::create_dir(&data).unwrap();
    expect_error(
        &fixture.compare(&mut oracle),
        "EISDIR: illegal operation on a directory, read",
    );
    assert_eq!(
        result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
        oracle.request(&json!({"op":"structure","root":fixture.root}))
    );
    fs::remove_dir_all(fixture.root.join("src/providers")).unwrap();
    assert_eq!(
        result(read_model_data_structure(&fixture.root).map(|v| structure_serde(&v))),
        oracle.request(&json!({"op":"structure","root":fixture.root}))
    );
}

#[test]
fn numeric_provider_model_and_api_keys_keep_js_diagnostic_order() {
    let mut fixture = Fixture::new();
    fixture.structure = structure(json!({"10":{"10":"api","2":"api"},"2":{"10":"api","2":"api"}}));
    fs::remove_file(fixture.dir.join("test-provider.json")).unwrap();
    let mut contents = indexmap::IndexMap::new();
    for provider in ["10", "2"] {
        let content = r#"{"9":{"10":null,"2":null},"api":{"10":null}}"#;
        let filename = format!("{provider}.json");
        fs::write(fixture.dir.join(&filename), content).unwrap();
        contents.insert(JsString::from(filename.as_str()), JsString::from(content));
    }
    let manifest = create_model_data_manifest(&fixture.structure, &contents, &JsString::from(GENERATED_AT));
    fs::write(fixture.dir.join(MODEL_DATA_MANIFEST_FILE), manifest.stringify()).unwrap();
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    expect_error(&fixture.compare(&mut oracle), "2/2 must be an object");
}

#[test]
fn raw_surrogate_keys_and_nested_values_match_full_ts_responses() {
    let mut oracle = TsOracle::spawn("model-data-oracle.ts", MODULE);
    let cases = [
        r#"{"openai-completions":{"\ud800":null,"\ufffd":null,"\udc00":null,"\ud800\udc00":null}}"#,
        r#"{"\ud800":{"model-a":null}}"#,
        r#"{"openai-completions":{"model-a":{"id":{"nested":["\ud800","\udc00","\ud800\udc00"]}}}}"#,
    ];
    let mut failures = Vec::new();
    for content in cases {
        let fixture = Fixture::new();
        fs::write(fixture.dir.join("test-provider.json"), content).unwrap();
        let actual = encoded(read_model_data_structure(&fixture.root).map(|v| model_data_structure_json(&v)));
        let expected = oracle.request(&json!({"op":"structure","root":fixture.root,"encodeResponse":true}));
        if json!(actual) != expected {
            failures.push(format!("structure {content}: Rust {actual}; TS {expected}"));
        }
        let actual = encoded(validate_model_data_directory(&fixture.structure, &fixture.dir).map(|()| JsValue::Null));
        let expected = oracle
            .request(&json!({"op":"directory","structure":structure_serde(&fixture.structure),"dir":fixture.dir,"encodeResponse":true}));
        if json!(actual) != expected {
            failures.push(format!("directory {content}: Rust {actual}; TS {expected}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
