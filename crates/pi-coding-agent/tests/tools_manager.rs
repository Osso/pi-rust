#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::utils::tools_manager::{Tool, ensure_tool, get_tool_path};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use std::path::Path;

fn cases() -> Vec<JsValue> {
    vec![
        json!({"name":"fd-first","tool":"fd","binaries":[["fd",0],["fdfind",0]],"silent":false}),
        json!({"name":"fd-alias","tool":"fd","binaries":[["fdfind",0]],"silent":false}),
        json!({"name":"nonzero-exists","tool":"fd","binaries":[["fd",3],["fdfind",0]],"silent":false}),
        json!({"name":"rg","tool":"rg","binaries":[["rg",0]],"silent":false}),
        json!({"name":"managed-ignored","tool":"rg","binaries":[],"managed":true,"silent":false}),
        json!({"name":"missing-fd","tool":"fd","binaries":[],"silent":false}),
        json!({"name":"missing-rg-silent","tool":"rg","binaries":[],"silent":true}),
        json!({"name":"managed-vs-system","tool":"fd","binaries":[["fd",0]],"managed":true,"silent":false}),
    ]
}

fn create_fixture(root: &Path, case: &JsValue, node: &str) -> JsValue {
    let name = case["name"].as_string().unwrap().to_utf8().unwrap();
    let cwd = root.join(name);
    let bin = cwd.join("path");
    let managed = cwd.join("agent").join("bin");
    let probe = cwd.join("probes.jsonl");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&managed).unwrap();
    if case.get("managed").and_then(JsValue::as_bool) == Some(true) {
        let tool = case["tool"].as_string().unwrap().to_utf8().unwrap();
        std::fs::write(managed.join(tool), "old managed binary").unwrap();
    }
    for binary in case["binaries"].as_array().unwrap() {
        let name = binary[0].as_string().unwrap().to_utf8().unwrap();
        let code = binary[1].as_f64().unwrap();
        let script = format!(
            "#!{node}\nconst fs=require('node:fs');fs.appendFileSync({},JSON.stringify([{},process.argv.slice(2)])+'\\n');process.stdout.write('suppressed stdout');process.stderr.write('suppressed stderr');process.exit({code});\n",
            serde_json::to_string(&probe).unwrap(),
            serde_json::to_string(&name).unwrap(),
        );
        executable_script::write_executable_script(&bin.join(name), &script);
    }
    let mut value = case.clone();
    let object = value.as_object_mut().unwrap();
    object.insert("path".into(), json!(bin));
    object.insert("managedPath".into(), json!(managed));
    object.insert("probe".into(), json!(probe));
    value
}

fn read_probes(case: &JsValue) -> Vec<JsValue> {
    let path = case["probe"].as_string().unwrap().to_utf8().unwrap();
    match std::fs::read_to_string(&path) {
        Ok(value) => {
            std::fs::remove_file(path).unwrap();
            value
                .lines()
                .map(|line| js_json::parse_lossless(line).unwrap())
                .collect()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("probe read: {error}"),
    }
}

fn native_case(case: &JsValue) -> JsValue {
    let path = case["path"].as_string().unwrap().to_utf8().unwrap();
    let managed = case["managedPath"].as_string().unwrap().to_utf8().unwrap();
    let agent_dir = Path::new(&managed).parent().unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "frozen_tools_manager_matches_path_resolution_and_missing_messages",
            "--nocapture",
        ])
        .env("PI_TOOLS_MANAGER_CASE", js_json::stringify_lossless(case))
        .env("PATH", path)
        .env("PI_CODING_AGENT_DIR", agent_dir)
        .env("FORCE_COLOR", "0")
        .output()
        .unwrap();
    assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
    assert!(!String::from_utf8_lossy(&child.stderr).contains("suppressed stderr"));
    let stdout = String::from_utf8(child.stdout).unwrap();
    let mut lines = stdout.lines();
    lines.find(|line| *line == "TOOL_LOG_BEGIN").expect("log marker");
    let mut messages = Vec::new();
    for line in lines {
        if let Some(result) = line.strip_prefix("TOOL_RESULT:") {
            let mut result = js_json::parse_lossless(result).unwrap();
            result
                .as_object_mut()
                .unwrap()
                .insert("messages".into(), json!(messages));
            return result;
        }
        messages.push(line.to_owned());
    }
    panic!("missing tools result: {stdout}")
}

#[tokio::test]
async fn frozen_tools_manager_matches_path_resolution_and_missing_messages() {
    if let Ok(input) = std::env::var("PI_TOOLS_MANAGER_CASE") {
        let case = js_json::parse_lossless(input).unwrap();
        let tool = match case["tool"].as_string().unwrap().to_utf8().unwrap().as_str() {
            "fd" => Tool::Fd,
            "rg" => Tool::Rg,
            _ => unreachable!("typed tool fixture"),
        };
        println!("TOOL_LOG_BEGIN");
        let found = get_tool_path(tool);
        let ensured = ensure_tool(tool, case["silent"].as_bool().unwrap()).await;
        println!(
            "TOOL_RESULT:{}",
            js_json::stringify_lossless(&json!({"found":found,"ensured":ensured}))
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "tools-manager-oracle.ts",
        "packages/coding-agent/src/utils/tools-manager.ts",
        &[("FORCE_COLOR", "0".into())],
    );
    let mut failures = Vec::new();
    for (index, case) in cases().iter().enumerate() {
        let case = create_fixture(root.path(), case, node.trim());
        let expected: JsValue = source.request(&json!({"id":index,"cases":[case.clone()]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let expected_probes = read_probes(&case);
        let actual = native_case(&case);
        let actual_probes = read_probes(&case);
        if actual != expected["output"][0] || actual_probes != expected_probes {
            failures.push(format!(
                "{}: {} != {}; probes {actual_probes:?} != {expected_probes:?}",
                js_json::stringify_lossless(&case["name"]),
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][0]),
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
