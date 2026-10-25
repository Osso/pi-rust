use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::find::FindTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::path::Path;
use std::sync::Arc;

fn process_cases(root: &Path) -> Vec<JsValue> {
    let search = root.join("search").to_str().unwrap().to_owned();
    let ordinary = format!("{search}/one\n{search}/two\n{search}/three\n");
    let mut cases = vec![
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
        json!({"limit":3}),
        json!({"limit":4}),
        json!({"limit":1e20}),
    ];
    for case in &mut cases {
        case.as_object_mut().unwrap().insert("stdout".into(), json!(ordinary));
    }
    for pattern in ["src/**/*.ts", "**/x", "/absolute/**", "**"] {
        cases.push(json!({"stdout":ordinary,"pattern":pattern}));
    }
    let invalid_line = [ordinary.as_bytes(), format!("{search}/bad").as_bytes(), &[255, 10]].concat();
    let many: String = (0..650)
        .map(|index| format!("{search}/{index:03}-long-xxxxxxxxxxxxxxxx\n"))
        .collect();
    cases.extend([
        json!({"stdout":""}),
        json!({"stdout":"","stderr":" diagnostic \n","code":3}),
        json!({"stdout":"","stderr":"","code":3}),
        json!({"stdout":"","stderr":" \u{feff}\n","code":3}),
        json!({"stdout":"","stderr":"\u{85}","code":3}),
        json!({"stdout":ordinary,"stderr":"ignored diagnostic","code":3}),
        json!({"stdout":format!("{search}/leaf\u{feff}\n")}),
        json!({"stdout":format!("{search}/\u{85}\n")}),
        json!({"stdout":format!("{search}x/leaf\n")}),
        json!({"stdout":"relative.txt\n"}),
        json!({"stdout":format!("{search}/tail\\\n")}),
        json!({"stdout":format!("{search}/one\r{search}/two\r")}),
        json!({"stdout":"\n\n","stderr":"ignored diagnostic","code":3}),
        json!({"bytes":invalid_line}),
        json!({"pattern":JsString::from("bad\0") + JsString::from_utf16(vec![0xd800])}),
        json!({"stdout":"","stderrBytes":[255],"code":3}),
        json!({"stdout":format!("{search}/{}\n","x".repeat(12000))}),
        json!({"stdout":many,"limit":700}),
        json!({"stdout":many,"limit":600}),
        json!({"stdout":"","signal":"SIGTERM"}),
        json!({"stdout":format!("{search}/after-parent\n"),"descendant":true}),
    ]);
    cases
}

fn find_args(case: &JsValue) -> JsValue {
    let pattern = case.get("pattern").cloned().unwrap_or_else(|| json!("*.x"));
    let mut args = json!({"pattern":pattern,"path":"search"});
    let number = match case.get("number").and_then(JsValue::as_string) {
        Some(number) => Some(match number.to_utf8().unwrap().as_str() {
            "-0" => -0.0,
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => unreachable!("controlled numeric fixture"),
        }),
        None => case.get("limit").and_then(JsValue::as_f64),
    };
    if let Some(number) = number {
        args.as_object_mut()
            .unwrap()
            .insert("limit".into(), JsValue::Number(number));
    }
    args
}

fn remove_argv_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove argv fixture: {error}"),
    }
}

fn read_argv_file(path: &Path) -> JsValue {
    match std::fs::read_to_string(path) {
        Ok(value) => js_json::parse_lossless(value).unwrap(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => JsValue::Null,
        Err(error) => panic!("read argv fixture: {error}"),
    }
}

async fn execute_cases(root: &Path) -> JsValue {
    let tool = FindTool::new(root.to_str().unwrap());
    let argv_file = root.join("argv.json");
    let mut output = Vec::new();
    for case in process_cases(root) {
        remove_argv_file(&argv_file);
        pi_js_compat::process_env::set("PI_FAKE_FD_CASE", &js_json::stringify_lossless(&case));
        let result = match tool
            .execute("fd-process".into(), find_args(&case), None, Arc::new(|_| Ok(())), None)
            .await
        {
            Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        output.push(json!({"result":result,"argv":read_argv_file(&argv_file)}));
    }
    JsValue::Array(output)
}

fn create_fake_fd(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::create_dir(root.join("search")).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let script = format!(
        "#!{}\nif(process.argv[2]==='--version')process.exit(0);const fs=require('node:fs');const p=JSON.parse(process.env.PI_FAKE_FD_CASE);fs.writeFileSync({},JSON.stringify(process.argv.slice(2)));if(p.descendant){{const code='setTimeout(()=>process.stdout.write('+JSON.stringify(p.stdout||'')+'),250)';require('node:child_process').spawn(process.execPath,['-e',code],{{stdio:['ignore',1,2]}});process.exit(0)}}fs.writeSync(1,p.bytes?Buffer.from(p.bytes):(p.stdout||''));fs.writeSync(2,p.stderrBytes?Buffer.from(p.stderrBytes):(p.stderr||''));if(p.signal){{process.kill(process.pid,p.signal);setInterval(()=>{{}},1000)}}else process.exit(p.code||0);\n",
        node.trim(),
        serde_json::to_string(&root.join("argv.json")).unwrap(),
    );
    executable_script::write_executable_script(&bin.join("fd"), &script);
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

fn run_native_child(root: &Path, path: &str) -> JsValue {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fd_process_arguments_and_decoding_match_frozen",
            "--nocapture",
        ])
        .env("PI_FD_PROCESS_ROOT", root)
        .env("PATH", path)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
    let stdout = String::from_utf8(child.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("FD_PROCESS:"))
        .expect("child output");
    js_json::parse_lossless(record).unwrap()
}

#[tokio::test]
async fn fd_process_arguments_and_decoding_match_frozen() {
    if let Ok(root) = std::env::var("PI_FD_PROCESS_ROOT") {
        let result = execute_cases(Path::new(&root)).await;
        println!("FD_PROCESS:{}", js_json::stringify_lossless(&result));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let path = create_fake_fd(root.path());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "find-process-oracle.ts",
        "packages/coding-agent/src/core/tools/find.ts",
        &[("PATH", path.clone().into())],
    );
    let cases = process_cases(root.path());
    let expected: JsValue =
        source.request(&json!({"id":1,"cwd":root.path(),"argvFile":root.path().join("argv.json"),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let actual = run_native_child(root.path(), &path);
    let expected = expected["output"].as_array().unwrap();
    let actual = actual.as_array().unwrap();
    assert_eq!(actual.len(), cases.len());
    assert_eq!(expected.len(), cases.len());
    let mut failures = Vec::new();
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
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

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
