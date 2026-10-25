use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn cases() -> Vec<JsValue> {
    let numbers = vec![
        json!({"absent":true}),
        json!({"value":0}),
        json!({"number":"-0"}),
        json!({"value":1}),
        json!({"value":0.1}),
        json!({"value":1.5}),
        json!({"value":-1}),
        json!({"value":-0.5}),
        json!({"number":"NaN"}),
        json!({"number":"Infinity"}),
        json!({"number":"-Infinity"}),
        json!({"value":5e-324}),
        json!({"value":-5e-324}),
        json!({"value":3.5}),
        json!({"value":3}),
        json!({"value":4}),
        json!({"value":1e20}),
    ];
    ["limit", "context"]
        .into_iter()
        .flat_map(|slot| {
            numbers.iter().map(move |number| {
                let mut value = number.clone();
                value.as_object_mut().unwrap().insert("slot".into(), json!(slot));
                value
            })
        })
        .collect()
}

fn args(case: &JsValue) -> JsValue {
    let mut args = json!({"pattern":"x","path":"search"});
    if case["absent"] == JsValue::Bool(true) {
        return args;
    }
    let value = match case.get("number").and_then(JsValue::as_string) {
        Some(number) => match number.to_utf8().unwrap().as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => unreachable!("controlled number fixture"),
        },
        None => case["value"].as_f64().unwrap(),
    };
    args.as_object_mut()
        .unwrap()
        .insert(case["slot"].as_string().unwrap().clone(), JsValue::Number(value));
    args
}

#[derive(Default)]
struct Operations {
    calls: Mutex<Vec<JsValue>>,
    reads: AtomicUsize,
}
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["isDirectory", path]));
            Ok(true)
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["readFile", path]));
            let reads = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(format!("one\ntwo\nthird-version-{reads}\nfour\nfive").into())
        })
    }
}

fn fake_rg(root: &std::path::Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let body = r#"if(process.argv[2]==='--version')process.exit(0);
const fs=require('node:fs');
const event={type:'match',data:{path:{text:'/repo/search/a.txt'},line_number:3,lines:{text:'from-rg\n'}}};
fs.writeSync(1,Array(3).fill(JSON.stringify(event)).join('\n')+'\n');process.exit(0);
"#;
    executable_script::write_executable_script(&bin.join("rg"), &format!("#!{}\n{body}", node.trim()));
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

#[tokio::test]
async fn grep_binary64_context_limits_and_file_cache_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rg(root.path());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-numbers-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("PATH", path.clone().into())],
    );
    let cases = cases();
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &path);
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let operations = Arc::new(Operations::default());
        let tool = GrepTool::with_options(
            "/repo",
            GrepToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let execution = tool.execute("grep-number".into(), args(case), None, Arc::new(|_| Ok(())), None);
        let result = match tokio::spawn(execution).await {
            Ok(Ok(result)) => js_json::to_value(&result).unwrap(),
            Ok(Err(error)) => json!({"error":error.message}),
            Err(error) => json!({"panic":error.is_panic()}),
        };
        let actual = json!({"result":result,"calls":operations.calls.lock().unwrap().clone()});
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
