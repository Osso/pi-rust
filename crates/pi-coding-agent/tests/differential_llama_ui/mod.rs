#[path = "../support/oracle.rs"]
mod oracle;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::llama::client::{LlamaModelInfo, LlamaProgress};
use pi_coding_agent::bundled_extensions::llama::ui::{
    ConnectionErrorChoice, HuggingFaceSearchFn, LlamaManagerAction, LlamaUi, ProgressState, RunWithProgressOptions,
    RunWithProgressOutcome, RunWithProgressRun, run_with_progress,
};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/coding-agent/src/extensions/llama/ui.ts";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Complete,
    Error,
    Cancel,
    NotStop,
}

impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Error => "error",
            Self::Cancel => "cancel",
            Self::NotStop => "not_stop",
        }
    }
}

struct FakeUi {
    scenario: Scenario,
    updates: Mutex<Vec<Value>>,
    cancel_calls: AtomicUsize,
}

impl FakeUi {
    fn new(scenario: Scenario) -> Arc<Self> {
        Arc::new(Self {
            scenario,
            updates: Mutex::new(Vec::new()),
            cancel_calls: AtomicUsize::new(0),
        })
    }
}

fn state_json(state: &ProgressState) -> Value {
    json!({
        "title": state.title,
        "model": state.model,
        "message": state.message,
        "ratio": state.ratio,
        "detail": state.detail,
    })
}

impl LlamaUi for FakeUi {
    fn show_models(&self, _server_url: String, _models: Vec<LlamaModelInfo>) -> BoxFuture<'_, LlamaManagerAction> {
        Box::pin(async { LlamaManagerAction::Close })
    }

    fn select(&self, _title: String, _options: Vec<String>) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { None })
    }

    fn confirm(&self, _title: String, _message: String) -> BoxFuture<'_, bool> {
        let stop = self.scenario == Scenario::Cancel;
        Box::pin(async move { stop })
    }

    fn connection_error(
        &self,
        _server_url: String,
        _message: pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, ConnectionErrorChoice> {
        Box::pin(async { ConnectionErrorChoice::Close })
    }

    fn search_models(&self, _search: HuggingFaceSearchFn) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { None })
    }

    fn show_status(&self, _title: String, _message: String) {}

    fn progress(&self, _state: ProgressState) -> BoxFuture<'_, ()> {
        let scenario = self.scenario;
        Box::pin(async move {
            match scenario {
                Scenario::Complete | Scenario::Error => std::future::pending::<()>().await,
                Scenario::Cancel => {}
                Scenario::NotStop => tokio::time::sleep(std::time::Duration::from_millis(1)).await,
            }
        })
    }

    fn update_progress(&self, state: &ProgressState) {
        self.updates.lock().unwrap().push(state_json(state));
    }
}

fn options(scenario: Scenario, ui: Arc<FakeUi>) -> RunWithProgressOptions<String> {
    let run: RunWithProgressRun<String> = Box::new(move |controller, update| {
        Box::pin(async move {
            update(LlamaProgress {
                message: "Half".to_owned(),
                ratio: Some(0.5),
                detail: None,
            });
            match scenario {
                Scenario::Error => Err("boom".to_owned()),
                Scenario::Cancel => {
                    controller.cancelled().await;
                    Err("Cancelled".to_owned())
                }
                Scenario::NotStop => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    Ok("done".to_owned())
                }
                Scenario::Complete => Ok("done".to_owned()),
            }
        })
    });
    RunWithProgressOptions {
        title: "Loading model".to_owned(),
        model: "test-model".to_owned(),
        initial_message: "Starting…".to_owned(),
        cancel_title: "Stop loading?".to_owned(),
        cancel_message: "test-model".to_owned(),
        run,
        cancel: Box::new(move || {
            let ui = Arc::clone(&ui);
            Box::pin(async move {
                ui.cancel_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }),
    }
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("llama-ui-oracle.ts", SOURCE_MODULE);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for scenario in [Scenario::Complete, Scenario::Error, Scenario::Cancel, Scenario::NotStop] {
        let response = oracle.request(&json!({
            "id": scenario.name(),
            "op": "run_with_progress",
            "scenario": scenario.name(),
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        let expected = &response["output"];

        let ui = FakeUi::new(scenario);
        let result = runtime.block_on(run_with_progress(
            Arc::clone(&ui) as Arc<dyn LlamaUi>,
            options(scenario, Arc::clone(&ui)),
        ));
        let actual = match result {
            Ok(RunWithProgressOutcome::Cancelled) => json!({
                "outcome": {"cancelled": true},
                "updates": *ui.updates.lock().unwrap(),
                "cancelCalls": ui.cancel_calls.load(Ordering::SeqCst),
            }),
            Ok(RunWithProgressOutcome::Completed(value)) => json!({
                "outcome": {"cancelled": false, "value": value},
                "updates": *ui.updates.lock().unwrap(),
                "cancelCalls": ui.cancel_calls.load(Ordering::SeqCst),
            }),
            Err(error) => json!({
                "error": error,
                "updates": *ui.updates.lock().unwrap(),
                "cancelCalls": ui.cancel_calls.load(Ordering::SeqCst),
            }),
        };
        assert_eq!(
            serde_json::to_string(expected).unwrap(),
            serde_json::to_string(&actual).unwrap(),
            "scenario {}",
            scenario.name()
        );
    }
}
