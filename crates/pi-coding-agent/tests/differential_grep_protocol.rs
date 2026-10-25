use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    sync::{Arc, Mutex},
};

fn event(path: JsString, line: f64, text: Option<JsString>) -> JsValue {
    let mut value = json!({"type":"match","data":{"path":{"text":path},"line_number":line,"lines":{}}});
    if let Some(text) = text {
        value["data"]["lines"]
            .as_object_mut()
            .unwrap()
            .insert("text".into(), json!(text));
    }
    value
}
fn wire(events: &[JsValue], separator: &str) -> String {
    events
        .iter()
        .map(js_json::stringify_lossless)
        .collect::<Vec<_>>()
        .join(separator)
        + separator
}
fn event_values(path: JsValue, text: JsValue) -> JsValue {
    json!({"type":"match","data":{"path":{"text":path},"line_number":3,"lines":{"text":text}}})
}
fn cases() -> Vec<JsValue> {
    let normal = event("/repo/search/a.txt".into(), 3.0, Some("normal\n".into()));
    let missing_text = event("/repo/search/a.txt".into(), 3.0, None);
    let invalid_bytes = [
        br#"{"type":"match","data":{"path":{"text":"/repo/search/a.txt"},"line_number":3,"lines":{"text":"bad"#
            .as_slice(),
        &[255],
        b"\"}}}\n",
    ]
    .concat();
    let infinity = js_json::stringify_lossless(&normal).replace("\"line_number\":3", "\"line_number\":1e309");
    let mut cases = vec![
        json!({"stdout":wire(&[json!({"type":"match","data":{"path":{"bytes":"YQ=="},"line_number":3,"lines":{"text":"ignored"}}})],"\n")}),
        json!({"stdout":wire(&[event("".into(),3.0,Some("ignored".into()))],"\n")}),
        json!({"stdout":wire(&[json!({"type":"match"})],"\n")}),
        json!({"stdout":wire(&[json!({"type":"begin"})],"\n")}),
        json!({"stdout":"not-json\n"}),
        json!({"stdout":format!("\u{feff}{}",wire(std::slice::from_ref(&normal),"\n"))}),
        json!({"stdout":wire(&[event("/repo/search/a.txt".into(),3.0,Some(JsString::from("raw-")+JsString::from_utf16(vec![0xd800])))],"\n")}),
        json!({"stdout":wire(&[event(JsString::from("/repo/search/p-")+JsString::from_utf16(vec![0xdfff]),3.0,Some("path".into()))],"\n")}),
        json!({"stdout":wire(&[event("/repo/search/a.txt".into(),1.5,Some("fraction".into()))],"\n")}),
        json!({"stdout":infinity+"\n"}),
        json!({"stdout":wire(&[event("/repo/search/a.txt".into(),0.0,None)],"\n")}),
        json!({"stdout":wire(&[missing_text.clone(),missing_text.clone()],"\n"),"context":1,"readError":true}),
        json!({"stdout":wire(&[missing_text.clone(),missing_text.clone()],"\n"),"context":1,"content":""}),
        json!({"stdout":wire(&[missing_text],"\n"),"content":JsString::from("one\ntwo\nraw-")+JsString::from_utf16(vec![0xd800])}),
        json!({"stdoutBytes":invalid_bytes}),
        json!({"stdout":wire(&[normal.clone(),normal.clone()],"\r")}),
        json!({"stdout":js_json::stringify_lossless(&normal)}),
        json!({"stdout":"","stderr":" \u{feff}\n","code":2}),
        json!({"stdout":"","stderr":"\u{85}","code":2}),
        json!({"stdout":"","stderrBytes":[255],"code":2}),
        json!({"stdout":wire(&(0..50).map(|_|event("/repo/search/a.txt".into(),3.0,Some("long ".to_owned().repeat(120).into()))).collect::<Vec<_>>(),"\n")}),
        json!({"stdout":wire(&(0..50).map(|_|event("/repo/search/a.txt".into(),3.0,Some("long ".to_owned().repeat(120).into()))).collect::<Vec<_>>(),"\n"),"limit":25}),
        json!({"stdout":wire(&[event("/repo/search/a.txt".into(),3.0,Some("🦀".repeat(300).into()))],"\n")}),
        json!({"stdout":wire(&[event("/repo/search/a.txt".into(),3.0,Some("one\r\nembedded\0\rline\n\n".into()))],"\n")}),
        json!({"stdout":"null\n"}),
        json!({"stdout":wire(&[event_values(json!({"constructor":null}),json!("inline"))],"\n")}),
        json!({"stdout":wire(&[event_values(json!({"constructor":"bad"}),json!("inline"))],"\n")}),
        json!({"stdout":wire(&[event_values(json!({"constructor":{"name":"Named"}}),json!("inline"))],"\n")}),
        json!({"stdout":wire(&[event_values(json!({"constructor":{"name":""}}),json!("inline"))],"\n")}),
        json!({"stdout":wire(&[event_values(json!(42),json!("inline"))],"\n").replace("\"text\":42","\"text\":1e309")}),
        json!({"stdout":"42\nfalse\n\"text\"\n"}),
        json!({"stdout":wire(&[event_values(json!(42),json!("inline"))],"\n"),"isDirectory":false}),
        json!({"stdout":wire(&[event_values(json!(42),json!("inline"))],"\n"),"code":2}),
        json!({"stdout":wire(&[event_values(json!("/repo/search/a.txt"),JsValue::Null)],"\n"),"context":1}),
    ];
    for path in [
        json!(42),
        json!(true),
        json!([]),
        json!({}),
        JsValue::Null,
        json!(0),
        json!(false),
    ] {
        cases.push(json!({"stdout":wire(&[event_values(path,json!("inline"))],"\n")}));
    }
    for text in [JsValue::Null, json!(42), json!(false), json!([]), json!({})] {
        cases.push(json!({"stdout":wire(&[event_values(json!("/repo/search/a.txt"),text)],"\n")}));
    }
    cases
}
struct Operations {
    case: JsValue,
    calls: Mutex<Vec<JsValue>>,
}
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["isDirectory", path]));
            Ok(self.case.get("isDirectory").and_then(JsValue::as_bool).unwrap_or(true))
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["readFile", path]));
            if self.case["readError"] == JsValue::Bool(true) {
                return Err(io::Error::other("scripted read failure"));
            }
            Ok(self
                .case
                .get("content")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_else(|| "one\ntwo\nthree\nfour\nfive".into()))
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
const fs=require('node:fs');const p=JSON.parse(process.env.PI_FAKE_RG_PROTOCOL);
fs.writeSync(1,p.stdoutBytes?Buffer.from(p.stdoutBytes):(p.stdout||''));
fs.writeSync(2,p.stderrBytes?Buffer.from(p.stderrBytes):(p.stderr||''));process.exit(p.code||0);
"#;
    executable_script::write_executable_script(&bin.join("rg"), &format!("#!{}\n{body}", node.trim()));
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}
fn observe_native_execution(result: Result<Result<AgentToolResult, ToolError>, tokio::task::JoinError>) -> JsValue {
    match result {
        Ok(Ok(value)) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Ok(Err(error)) => json!({"ok":false,"error":error.message}),
        Err(error) => {
            let payload = error.into_panic();
            let native = payload
                .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                .expect("native uncaught identity");
            json!({"uncaught":native.value(),"constructor":native.constructor_class().name(),"string":native.string_value()})
        }
    }
}
#[tokio::test]
async fn grep_raw_protocol_counting_decoding_and_cached_read_failures_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rg(root.path());
    let cases = cases();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-protocol-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &path);
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        pi_js_compat::process_env::set("PI_FAKE_RG_PROTOCOL", &js_json::stringify_lossless(case));
        let operations = Arc::new(Operations {
            case: case.clone(),
            calls: Mutex::new(Vec::new()),
        });
        let tool = GrepTool::with_options(
            "/repo",
            GrepToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let mut args = json!({"pattern":"x","path":"search"});
        for key in ["context", "limit"] {
            if let Some(value) = case.get(key) {
                args.as_object_mut().unwrap().insert(key.into(), value.clone());
            }
        }
        let execution = tool.execute("grep-protocol".into(), args, None, Arc::new(|_| Ok(())), None);
        let result = observe_native_execution(tokio::spawn(execution).await);
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
