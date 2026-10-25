use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::read::{ReadOperations, ReadRange, ReadTool, ReadToolOptions};
use pi_js_compat::{
    abort::AbortController,
    js_json::{JsString, JsValue},
};
use serde_json::{Value, json};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;

struct GatedOperations {
    stage: String,
    fail: bool,
    range: bool,
    image: bool,
    trace: Mutex<Vec<String>>,
    entered: Notify,
    release: Notify,
    completed: Notify,
}
impl GatedOperations {
    async fn step(&self, stage: &str) -> io::Result<()> {
        self.trace.lock().unwrap().push(format!("{stage}:start"));
        if stage == self.stage {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.trace.lock().unwrap().push(format!("{stage}:end"));
        let final_stage = if self.fail || self.stage == "access" {
            self.stage.as_str()
        } else if self.range {
            "range"
        } else {
            "read"
        };
        if stage == final_stage {
            self.completed.notify_one();
        }
        if self.fail && stage == self.stage {
            return Err(io::Error::other(format!("late {stage} failure")));
        }
        Ok(())
    }
}
impl ReadOperations for GatedOperations {
    fn access<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(self.step("access"))
    }
    fn detect_image_mime_type<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async {
            self.step("mime").await?;
            Ok(self.image.then(|| "image/png".into()))
        })
    }
    fn stat<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Option<f64>>> {
        Box::pin(async {
            self.step("stat").await?;
            Ok(Some(if self.range { 20000.0 } else { 4.0 }))
        })
    }
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async {
            self.step("read").await?;
            Ok(b"head".to_vec())
        })
    }
    fn read_range(&self) -> Option<ReadRange<'_>> {
        Some(Box::new(move |_, _, _| {
            Box::pin(async {
                self.step("range").await?;
                Ok(b"head".to_vec())
            })
        }))
    }
}
fn outcome(result: Result<AgentToolResult, ToolError>) -> Value {
    match result {
        Ok(result) => json!({"value":pi_js_compat::js_json::to_value(&result).unwrap().to_serde().unwrap()}),
        Err(error) => json!({"error":error.message}),
    }
}

#[tokio::test]
async fn abort_settles_promptly_without_cancelling_the_frozen_background_sequence() {
    let cwd = tempfile::tempdir().unwrap();
    let mut cases = Vec::new();
    for stage in ["access", "mime", "stat", "read", "range"] {
        for fail in [false, true] {
            cases.push(json!({"stage":stage,"fail":fail,"range":stage=="range"}));
        }
    }
    cases.push(json!({"stage":"read","image":true}));
    cases.push(json!({"stage":"mime","image":true}));
    let mut oracle = oracle::TsOracle::spawn("read-abort-oracle.ts", "packages/coding-agent/src/core/tools/read.ts");
    let expected: Value = oracle.request(&json!({"id":83,"cwd":cwd.path(),"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let mut mismatches = Vec::new();
    for (case, expected) in cases.iter().zip(expected["output"].as_array().unwrap()) {
        let operations = Arc::new(GatedOperations {
            stage: case["stage"].as_str().unwrap().into(),
            fail: case["fail"].as_bool().unwrap_or(false),
            range: case["range"].as_bool().unwrap_or(false),
            image: case["image"].as_bool().unwrap_or(false),
            trace: Mutex::new(Vec::new()),
            entered: Notify::new(),
            release: Notify::new(),
            completed: Notify::new(),
        });
        let tool = ReadTool::with_options(
            cwd.path().to_str().unwrap(),
            ReadToolOptions {
                operations: Some(operations.clone()),
                auto_resize_images: Some(false),
            },
        );
        let controller = AbortController::new();
        let mut task = tokio::spawn(tool.execute(
            "abort-read".into(),
            JsValue::from(json!({"path":"fixture"})),
            Some(controller.signal()),
            Arc::new(|_| Ok(())),
            None,
        ));
        tokio::time::timeout(Duration::from_secs(2), operations.entered.notified())
            .await
            .unwrap();
        controller.abort(Some(JsValue::String("custom reason ignored by read".into()).into()));
        let early_result = tokio::time::timeout(Duration::from_millis(40), &mut task).await;
        let early = match &early_result {
            Ok(result) => outcome(result.as_ref().unwrap().clone()),
            Err(_) => json!("pending"),
        };
        let before_release = operations.trace.lock().unwrap().clone();
        operations.release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), operations.completed.notified())
            .await
            .unwrap();
        let settled = match early_result {
            Ok(result) => outcome(result.unwrap()),
            Err(_) => outcome(
                tokio::time::timeout(Duration::from_secs(2), task)
                    .await
                    .unwrap()
                    .unwrap(),
            ),
        };
        let trace = operations.trace.lock().unwrap().clone();
        let actual = json!({"early":early,"beforeRelease":before_release,"settled":settled,"trace":trace});
        if actual != *expected {
            mismatches.push(format!("{case}: {actual}\nexpected {expected}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
