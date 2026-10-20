#[path = "../examples/extensions/model_status.rs"]
mod example;
#[path = "support/extension_examples_a.rs"]
mod harness;

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
fn model(provider: &str, id: &str) -> Value {
    json!({"provider":provider,"id":id,"name":id,"api":"faux","baseUrl":"", "reasoning":false,"input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":1024,"maxTokens":64})
}
#[tokio::test]
async fn all_model_sources_update_status_and_log_but_restore_does_not_notify() {
    let _default_factory = harness::factory(example::model_status_extension);
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
