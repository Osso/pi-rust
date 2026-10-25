use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::ls::LsTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use std::sync::Arc;

fn ordinary_cases() -> Vec<JsValue> {
    vec![
        json!({"stdout":"alpha\nbeta\n","stderr":"","code":0}),
        json!({"stdout":"alpha\nbeta\u{feff}","stderr":"","code":0}),
        json!({"stdout":"","stderr":"","code":3}),
        json!({"stdout":" fallback \n","stderr":" \u{feff}\n","code":3}),
        json!({"stdout":"\u{85}","stderr":"","code":3}),
        json!({"stdout":" ignored \n","stderr":" diagnostic \n","code":3}),
        json!({"stdout":" out \n","stderr":"\n","code":3}),
        json!({"stdout":"","stderr":"","code":0,"path":"bad\0argument"}),
        json!({"stdout":"","stderr":"","code":0,"cwd":"bad\0cwd"}),
        json!({"stdout":"","stderr":"","code":0,"path":"bad\0argument","cwd":"bad\0cwd"}),
        json!({"stdout":"x".repeat(12_000),"stderr":"","code":0}),
        json!({"stdout":"long-entry-xxxxxxxxxxxx\n".repeat(650),"stderr":"","code":0,"limit":700}),
        json!({"stdout":"long-entry-xxxxxxxxxxxx\n".repeat(650),"stderr":"","code":0,"limit":600}),
        json!({"stdout":" ".repeat(21_000),"stderr":"","code":0}),
        json!({"stdout":"","stderr":" ".repeat(21_000),"code":0}),
    ]
}
fn fake_rtk(root: &std::path::Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let script = format!(
        "#!{}\nconst fs=require('node:fs');const p=JSON.parse(process.env.PI_FAKE_RTK_IO_CASE);\nfs.writeSync(1,p.stdout.repeat(p.repeat||1));fs.writeSync(2,p.stderr.repeat(p.repeat||1));\nif(p.marker)fs.writeFileSync(p.marker,'completed');process.exit(p.code);\n",
        node.trim()
    );
    executable_script::write_executable_script(&bin.join("rtk"), &script);
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}
async fn execute_case(tool: &LsTool, case: &JsValue) -> JsValue {
    pi_js_compat::process_env::set("PI_FAKE_RTK_IO_CASE", &js_json::stringify_lossless(case));
    let override_tool = case.get("cwd").and_then(JsValue::as_string).map(LsTool::new);
    let tool = override_tool.as_ref().unwrap_or(tool);
    let path = case.get("path").cloned().unwrap_or_else(|| json!("list"));
    let mut args = json!({"path":path});
    if let Some(limit) = case.get("limit") {
        args.as_object_mut().unwrap().insert("limit".into(), limit.clone());
    }
    match tool
        .execute("rtk-io".into(), args, None, Arc::new(|_| Ok(())), None)
        .await
    {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}
fn native_child(root: &std::path::Path, path: &str, mode: &str, case: Option<&JsValue>) -> JsValue {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "rtk_process_outputs_and_bounded_capture", "--nocapture"])
        .env("PI_RTK_IO_CHILD", mode)
        .env("PI_RTK_IO_CWD", root)
        .env("PATH", path);
    if let Some(case) = case {
        command.env("PI_FAKE_RTK_IO_CASE", js_json::stringify_lossless(case));
    }
    let child = command.output().unwrap();
    assert!(
        child.status.success(),
        "native RTK child: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    let stdout = String::from_utf8(child.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("RTK_IO:"))
        .expect("child result");
    js_json::parse_lossless(record).unwrap()
}
#[tokio::test]
async fn rtk_process_outputs_and_bounded_capture() {
    if let Ok(mode) = std::env::var("PI_RTK_IO_CHILD") {
        let tool = LsTool::new(std::env::var("PI_RTK_IO_CWD").unwrap());
        let output = if mode == "ordinary" {
            let mut results = Vec::new();
            for case in ordinary_cases() {
                results.push(execute_case(&tool, &case).await);
            }
            JsValue::Array(results)
        } else {
            let case = js_json::parse_lossless(std::env::var("PI_FAKE_RTK_IO_CASE").unwrap()).unwrap();
            execute_case(&tool, &case).await
        };
        println!("RTK_IO:{}", js_json::stringify_lossless(&output));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let path = fake_rtk(root.path());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "ls-rtk-io-oracle.ts",
        "packages/coding-agent/src/core/tools/ls.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":ordinary_cases()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let actual = native_child(root.path(), &path, "ordinary", None);
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
                "case {index}: {} != {}",
                js_json::stringify_lossless(actual),
                js_json::stringify_lossless(expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn rtk_overflow_is_bounded_utf8_safe_and_terminates_the_child() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rtk(root.path());
    for (index, (stdout, stderr)) in [("€", ""), ("", "€")].into_iter().enumerate() {
        let marker = root.path().join(format!("overflow-completed-{index}"));
        let input = json!({"stdout":stdout,"stderr":stderr,"repeat":2_000_000,"code":0,"marker":marker});
        let result = native_child(root.path(), &path, "overflow", Some(&input));
        assert_eq!(result["ok"], JsValue::Bool(false), "overflow stream {index}");
        let error = result["error"].as_string().unwrap().to_utf8().unwrap();
        assert!(error.len() <= 20 * 1024, "decoded UTF-8 output remains bounded");
        assert!(!marker.exists(), "overflow must terminate before producer completion");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
