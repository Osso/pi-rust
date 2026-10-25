use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::ls::{LsOperations, LsTool, LsToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    sync::{Arc, Mutex},
};

fn number_cases() -> Vec<JsValue> {
    vec![
        json!({}),
        json!({"limit":0}),
        json!({"number":"-0"}),
        json!({"limit":1}),
        json!({"limit":0.1}),
        json!({"limit":1.5}),
        json!({"limit":-1}),
        json!({"limit":-0.5}),
        json!({"number":"NaN"}),
        json!({"number":"Infinity"}),
        json!({"number":"-Infinity"}),
        json!({"limit":5e-324}),
        json!({"limit":-5e-324}),
        json!({"limit":3.5}),
        json!({"limit":4}),
        json!({"limit":4.5}),
        json!({"limit":1e20}),
    ]
}
fn number_args(case: &JsValue) -> JsValue {
    let mut args = json!({"path":"list"});
    let number = case
        .get("number")
        .and_then(JsValue::as_string)
        .map(|number| match number.to_utf8().unwrap().as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => unreachable!("controlled fixture"),
        })
        .or_else(|| case.get("limit").and_then(JsValue::as_f64));
    if let Some(number) = number {
        args.as_object_mut()
            .unwrap()
            .insert("limit".into(), JsValue::Number(number));
    }
    args
}
#[derive(Default)]
struct Operations {
    calls: Mutex<Vec<JsValue>>,
}
impl LsOperations for Operations {
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["exists", path]));
            Ok(true)
        })
    }
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["stat", path]));
            if path.ends_with("/ghost") {
                return Err(io::Error::other("stat denied"));
            }
            Ok(path == "/repo/list" || path.ends_with("/dir"))
        })
    }
    fn readdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["readdir", path]));
            Ok(["gamma", "alpha", "BETA", "ghost", "dir"].map(JsString::from).to_vec())
        })
    }
}
#[tokio::test]
async fn injected_ls_preserves_js_number_limits_and_stat_order() {
    let cases = number_cases();
    let mut source = oracle::TsOracle::spawn("ls-numbers-oracle.ts", "packages/coding-agent/src/core/tools/ls.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let operations = Arc::new(Operations::default());
        let tool = LsTool::with_options(
            "/repo",
            LsToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let result = tool
            .execute("ls-numbers".into(), number_args(case), None, Arc::new(|_| Ok(())), None)
            .await
            .unwrap();
        let actual =
            json!({"calls":operations.calls.lock().unwrap().clone(),"result":js_json::to_value(&result).unwrap()});
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

#[tokio::test]
async fn rtk_ls_preserves_js_slice_limits_and_number_notices() {
    if let Ok(cwd) = std::env::var("PI_LS_NUMBERS_CHILD_CWD") {
        let tool = LsTool::new(cwd);
        let mut results = Vec::new();
        for case in number_cases() {
            let result = tool
                .execute(
                    "ls-numbers".into(),
                    number_args(&case),
                    None,
                    Arc::new(|_| Ok(())),
                    None,
                )
                .await
                .unwrap();
            results.push(json!({"calls":[],"result":js_json::to_value(&result).unwrap()}));
        }
        println!("RTK_NUMBERS:{}", js_json::stringify_lossless(&JsValue::Array(results)));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    executable_script::write_executable_script(
        &bin.join("rtk"),
        &format!(
            "#!{}\nprocess.stdout.write('alpha\\nbeta\\ngamma\\ndelta\\n');\n",
            node.trim()
        ),
    );
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "ls-numbers-oracle.ts",
        "packages/coding-agent/src/core/tools/ls.ts",
        &[("PATH", path.clone().into())],
    );
    let cases: Vec<_> = number_cases()
        .into_iter()
        .map(|mut case| {
            case.as_object_mut().unwrap().insert("rtk".into(), JsValue::Bool(true));
            case
        })
        .collect();
    let expected: JsValue = source.request(&json!({"id":2,"cwd":root.path(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rtk_ls_preserves_js_slice_limits_and_number_notices",
            "--nocapture",
        ])
        .env("PI_LS_NUMBERS_CHILD_CWD", root.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
    let stdout = String::from_utf8(child.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("RTK_NUMBERS:"))
        .expect("child result");
    let actual = js_json::parse_lossless(record).unwrap();
    assert_eq!(
        actual.as_array().unwrap().len(),
        expected["output"].as_array().unwrap().len()
    );
    let mut failures = Vec::new();
    for (index, (actual, expected)) in actual
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["output"].as_array().unwrap())
        .enumerate()
    {
        if actual != expected {
            failures.push(format!(
                "rtk case {index}: {} != {}",
                js_json::stringify_lossless(actual),
                js_json::stringify_lossless(expected)
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
