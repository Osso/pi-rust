#![cfg(unix)]
use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::HandlerError;
use pi_coding_agent::core::tools::find::{FindOperations, FindTool, FindToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};

fn failure_cases(root: &Path) -> Vec<JsValue> {
    let search = root.join("search").to_str().unwrap().to_owned();
    let empty = root.join("empty-bin");
    std::fs::create_dir(&empty).unwrap();
    let bin = root.join("vanishing-bin");
    let combined: Vec<_> = (0..650)
        .map(|index| format!("{search}/{index:03}-long-xxxxxxxxxxxxxxxx"))
        .collect();
    vec![
        json!({"kind":"missing"}),
        json!({"kind":"exists-error","message":"exists denied\n"}),
        json!({"kind":"exists-error","message":JsString::from("raw-")+JsString::from_utf16(vec![0xd800])}),
        json!({"kind":"glob-error","message":"glob denied\n"}),
        json!({"kind":"glob-error","message":JsString::from("raw-")+JsString::from_utf16(vec![0xdfff])}),
        json!({"kind":"empty","paths":[],"limit":0}),
        json!({"kind":"byte-cap","paths":[format!("{search}/{}","x".repeat(12000))]}),
        json!({"kind":"combined-cap","limit":600,"paths":combined}),
        json!({"kind":"deleted-cwd","paths":["relative.txt"]}),
        json!({"kind":"missing-fd","pathEnv":empty}),
        json!({"kind":"spawn-failure","pathEnv":bin,"bin":bin}),
    ]
}

struct Operations {
    case: JsValue,
    calls: Mutex<Vec<JsValue>>,
}
impl Operations {
    fn error(&self) -> io::Error {
        io::Error::other(HandlerError::new(self.case["message"].as_string().unwrap().clone()))
    }
}
impl FindOperations for Operations {
    fn supports_glob(&self) -> bool {
        true
    }
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["exists", path]));
            if self.case["kind"].as_string().unwrap() == "exists-error" {
                return Err(self.error());
            }
            Ok(self.case["kind"].as_string().unwrap() != "missing")
        })
    }
    fn glob<'a>(
        &'a self,
        pattern: &'a JsString,
        cwd: &'a JsString,
        ignore: &'a [JsString],
        limit: f64,
    ) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!([
                "glob",
                pattern,
                cwd,
                ignore,
                pi_js_compat::number::js_number_to_string(limit)
            ]));
            if self.case["kind"].as_string().unwrap() == "glob-error" {
                return Err(self.error());
            }
            Ok(self
                .case
                .get("paths")
                .and_then(JsValue::as_array)
                .map(|paths| paths.iter().map(|path| path.as_string().unwrap().clone()).collect())
                .unwrap_or_default())
        })
    }
}

fn prepare_environment(root: &Path, case: &JsValue, node: &str) {
    if case["kind"].as_string().unwrap() == "deleted-cwd" {
        let directory = root.join("native-deleted-cwd");
        std::fs::create_dir(&directory).unwrap();
        std::env::set_current_dir(&directory).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    if case["kind"].as_string().unwrap() == "spawn-failure" {
        let bin = case["bin"].as_string().unwrap().to_utf8().unwrap();
        let bin = Path::new(&bin);
        std::fs::create_dir_all(bin).unwrap();
        let script = format!(
            "#!{node}\nconst fs=require('node:fs');if(process.argv[2]==='--version'){{fs.unlinkSync(__filename);process.exit(0)}}process.exit(1);\n"
        );
        executable_script::write_executable_script(&bin.join("fd"), &script);
    }
    if let Some(path) = case.get("pathEnv").and_then(JsValue::as_string) {
        pi_js_compat::process_env::set("PATH", &path.to_utf8().unwrap());
    }
}

async fn execute_case(root: &Path, case: &JsValue) -> JsValue {
    let operations = Arc::new(Operations {
        case: case.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let custom = !matches!(
        case["kind"].as_string().unwrap().to_utf8().unwrap().as_str(),
        "missing-fd" | "spawn-failure"
    );
    let tool = FindTool::with_options(
        root.to_str().unwrap(),
        FindToolOptions {
            operations: custom.then_some(operations.clone()),
        },
    );
    let mut args = json!({"pattern":"*.x","path":"search"});
    if let Some(limit) = case.get("limit") {
        args.as_object_mut().unwrap().insert("limit".into(), limit.clone());
    }
    let result = match tool
        .execute("find-failure".into(), args, None, Arc::new(|_| Ok(())), None)
        .await
    {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    json!({"result":result,"calls":operations.calls.lock().unwrap().clone()})
}

#[tokio::test]
async fn find_failure_boundaries_and_combined_caps_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let cases = failure_cases(root.path());
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "find-failures-oracle.ts",
        "packages/coding-agent/src/core/tools/find.ts",
    );
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let old_cwd = std::env::current_dir().unwrap();
        let old_path = pi_js_compat::process_env::get("PATH").unwrap();
        prepare_environment(root.path(), case, node.trim());
        let actual = execute_case(root.path(), case).await;
        std::env::set_current_dir(old_cwd).unwrap();
        pi_js_compat::process_env::set("PATH", &old_path);
        if actual != expected["output"][index] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
