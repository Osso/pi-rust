#[path = "../examples/extensions/model_status.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;

use serde_json::{Value, json};
use std::io::Write;
use std::sync::Arc;

struct CaptureLog(harness::Trace);
impl Write for CaptureLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        // The example emits one complete line through write_all.
        let line = std::str::from_utf8(bytes).unwrap().strip_suffix('\n').unwrap();
        self.0.lock().unwrap().push(json!(["log", line]));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct FailLog;
impl Write for FailLog {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("console unavailable"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn console_failure_is_reported_after_ui_effects() {
    let factory = |_trace| {
        let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
            let result = example::model_status_with_output(api, FailLog);
            Box::pin(async move { result })
        });
        factory
    };
    let actual = harness::compare(
        "model-status",
        factory,
        json!({"logError":"console unavailable","steps":[
            {"op":"event","event":{"type":"model_select","model":model("faux","first"),"source":"set"}}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"],
        json!([
            ["notify", "Model: faux/first", "info"],
            ["status", "model", "🤖 first"],
            ["handlerError", "console unavailable"]
        ])
    );
}

fn model(provider: &str, id: &str) -> Value {
    json!({"provider":provider,"id":id,"name":id,"api":"faux","baseUrl":"", "reasoning":false,"input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":1024,"maxTokens":64})
}
#[tokio::test]
async fn default_entrypoint_registers_handler_and_writes_real_stdout() {
    const CHILD: &str = "PI_TEST_MODEL_STATUS_CHILD";
    const TRACE_MARKER: &str = "MODEL_STATUS_TRACE=";
    let first = model("faux", "first");
    let next = model("other/🚀", "next𐀀");
    let request = json!({"steps":[
        {"op":"event","event":{"type":"model_select","model":first,"source":"set"}},
        {"op":"event","event":{"type":"model_select","model":next,"previousModel":first,"source":"cycle"}},
        {"op":"event","event":{"type":"model_select","model":first,"previousModel":next,"source":"restore"}},
        {"op":"event","event":{"type":"model_select","model":next,"source":"fallback"}}
    ]});
    if std::env::var_os(CHILD).is_some() {
        let actual = harness::run(harness::factory(example::model_status_extension), &request).await;
        println!("{TRACE_MARKER}{actual}");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "default_entrypoint_registers_handler_and_writes_real_stdout",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("run default model-status entrypoint in child test process");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let transcript = stdout.lines().find_map(|line| line.strip_prefix(TRACE_MARKER)).unwrap();
    let actual: Value = serde_json::from_str(transcript).unwrap();
    let mut expected = harness::run_oracle("model-status", &request);
    let expected_logs: Vec<_> = expected["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|action| action[0] == "log")
        .map(|action| action[1].as_str().unwrap().to_owned())
        .collect();
    let stdout_logs: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("[model_select]"))
        .collect();
    assert_eq!(
        stdout_logs, expected_logs,
        "real stdout differs from pinned TS console output"
    );
    expected["trace"]
        .as_array_mut()
        .unwrap()
        .retain(|action| action[0] != "log");
    assert_eq!(actual, expected, "default entrypoint UI effects differ from pinned TS");
}

#[tokio::test]
async fn all_model_sources_update_status_and_log_but_restore_does_not_notify() {
    let factory = |trace| {
        let trace: harness::Trace = trace;
        let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
            let result = example::model_status_with_output(api, CaptureLog(trace.clone()));
            Box::pin(async move { result })
        });
        factory
    };
    let first = model("faux", "first");
    let next = model("other/🚀", "next𐀀");
    let actual = harness::compare(
        "model-status",
        factory,
        json!({"steps":[
            {"op":"event","event":{"type":"model_select","model":first,"source":"set"}},
            {"op":"event","event":{"type":"model_select","model":next,"previousModel":first,"source":"cycle"}},
            {"op":"event","event":{"type":"model_select","model":first,"previousModel":next,"source":"restore"}},
            {"op":"event","event":{"type":"model_select","model":next,"source":"fallback"}}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"],
        json!([
            ["notify", "Model: faux/first", "info"],
            ["status", "model", "🤖 first"],
            ["log", "[model_select] none → faux/first (set)"],
            ["notify", "Model: other/🚀/next𐀀", "info"],
            ["status", "model", "🤖 next𐀀"],
            ["log", "[model_select] faux/first → other/🚀/next𐀀 (cycle)"],
            ["status", "model", "🤖 first"],
            ["log", "[model_select] other/🚀/next𐀀 → faux/first (restore)"],
            ["notify", "Model: other/🚀/next𐀀", "info"],
            ["status", "model", "🤖 next𐀀"],
            ["log", "[model_select] none → other/🚀/next𐀀 (fallback)"]
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
