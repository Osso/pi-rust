use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};
const SCRIPT: &str = include_str!("support/grep_argv_fixture.cjs");
struct Operations {
    case: JsValue,
    calls: Mutex<Vec<JsValue>>,
}
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["isDirectory", path]));
            if self.case["kind"].as_string().unwrap() == "stat-error" {
                return Err(io::Error::other("opaque stat failure"));
            }
            Ok(self.case.get("isDirectory").and_then(JsValue::as_bool).unwrap_or(true))
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["readFile", path]));
            Ok("one\ntwo\nthree".into())
        })
    }
}
fn cases(root: &Path) -> Vec<JsValue> {
    let raw = JsString::from_utf16(vec![0xd800]);
    let variants = vec![
        json!({}),
        json!({"args":{"pattern":"x","path":"search","glob":""}}),
        json!({"args":{"pattern":"x","path":"search","glob":"*.rs","ignoreCase":true,"literal":true}}),
        json!({"args":{"pattern":JsString::from("raw-")+&raw,"path":"search","glob":JsString::from("g-")+&raw}}),
        json!({"args":{"pattern":JsString::from("x\0")+&raw,"path":"search"}}),
        json!({"args":{"pattern":"x","path":"search","glob":JsString::from("g\0")+&raw}}),
        json!({"args":{"pattern":"x\0","path":"search","glob":"first\0","ignoreCase":true,"literal":true}}),
        json!({"args":{"pattern":"x","path":JsString::from("search/\0")+&raw}}),
        json!({"local":true,"args":{"pattern":"x","path":"missing"}}),
        json!({"local":true,"args":{"pattern":"x","path":"search\0"}}),
        json!({"kind":"stat-error","message":JsString::from("hidden-")+&raw}),
        json!({"kind":"missing"}),
        json!({"kind":"vanished"}),
        json!({"kind":"permission"}),
        json!({"kind":"signal"}),
        json!({"isDirectory":false,"args":{"pattern":"x","path":"search/a.txt"}}),
        json!({"filePath":root.join("search/a\\b.txt")}),
        json!({"filePath":root.join("outside/a\\b.txt")}),
        json!({"filePath":root.join("search")}),
        json!({"filePath":JsString::from(root.join("search/raw-").to_str().unwrap())+raw}),
        json!({"args":{"pattern":"x","path":""}}),
    ];
    variants
        .into_iter()
        .enumerate()
        .map(|(index, mut case)| {
            let object = case.as_object_mut().unwrap();
            object.entry("kind".into()).or_insert(json!("normal"));
            object
                .entry("args".into())
                .or_insert(json!({"pattern":"x","path":"search"}));
            object
                .entry("filePath".into())
                .or_insert(json!(root.join("search/a.txt")));
            object.insert("directory".into(), json!(root.join(format!("case-{index}"))));
            case
        })
        .collect()
}
async fn execute_case(root: &Path, case: &JsValue, node: &str) -> JsValue {
    let directory = case["directory"].as_string().unwrap().to_utf8().unwrap();
    let directory = Path::new(&directory);
    let bin = directory.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let argv_file = directory.join("argv.json");
    match std::fs::remove_file(&argv_file) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove fixture argv: {error}"),
    }
    if case["kind"].as_string().unwrap() != "missing" {
        executable_script::write_executable_script(&bin.join("rg"), &format!("#!{node}\n{SCRIPT}"));
    }
    let mut fixture = case.clone();
    fixture["argvFile"] = json!(argv_file);
    pi_js_compat::process_env::set("PI_FAKE_RG_ARGV", &js_json::stringify_lossless(&fixture));
    pi_js_compat::process_env::set("PATH", bin.to_str().unwrap());
    let operations = Arc::new(Operations {
        case: case.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let tool = GrepTool::with_options(
        root.to_str().unwrap(),
        GrepToolOptions {
            operations: if case["local"] == JsValue::Bool(true) {
                None
            } else {
                Some(operations.clone())
            },
        },
    );
    let result = match tool
        .execute(
            "grep-argv".into(),
            case["args"].clone(),
            None,
            Arc::new(|_| Ok(())),
            None,
        )
        .await
    {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    let argv = match std::fs::read(argv_file) {
        Ok(bytes) => serde_json::from_slice::<JsValue>(&bytes).unwrap(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => JsValue::Null,
        Err(error) => panic!("read fixture argv: {error}"),
    };
    json!({"result":result,"calls":operations.calls.lock().unwrap().clone(),"argv":argv})
}
#[tokio::test]
async fn grep_argument_validation_spawn_failures_and_path_labels_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let cases = cases(root.path());
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let mut source = oracle::TsOracle::spawn("grep-argv-oracle.ts", "packages/coding-agent/src/core/tools/grep.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":cases,"script":SCRIPT}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let actual = execute_case(root.path(), case, node.trim()).await;
        if actual != expected["output"][index] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    pi_js_compat::process_env::set("PATH", &old_path);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
