use crate::shared_oracle as oracle;

use pi_ai::types::Model;
use pi_coding_agent::cli::list_models::{ListModelsIo, ListModelsRuntime, list_models};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde_json::{Value, json};
use std::sync::Mutex;

struct StubRuntime {
    error: Option<JsString>,
    models: Vec<Model>,
    available_error: Option<NativeNodeError>,
}

impl ListModelsRuntime for StubRuntime {
    fn get_error(&self) -> Option<JsString> {
        self.error.clone()
    }

    fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError> {
        match &self.available_error {
            Some(error) => Err(error.clone()),
            None => Ok(self.models.clone()),
        }
    }
}

struct Case {
    id: u64,
    name: &'static str,
    models: Vec<Value>,
    search: Option<&'static str>,
    load_error: Option<&'static str>,
}

fn model(id: &str, provider: &str, context_window: f64, max_tokens: f64, reasoning: bool, image: bool) -> Value {
    let mut input = vec!["text"];
    if image {
        input.push("image");
    }
    json!({
        "id": id,
        "name": id,
        "api": "openai-responses",
        "provider": provider,
        "baseUrl": "https://example.invalid/v1",
        "reasoning": reasoning,
        "input": input,
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": context_window,
        "maxTokens": max_tokens,
    })
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            id: 28_300,
            name: "empty catalog",
            models: Vec::new(),
            search: None,
            load_error: None,
        },
        Case {
            id: 28_301,
            name: "empty catalog with load error",
            models: Vec::new(),
            search: None,
            load_error: Some("models.json: unexpected token"),
        },
        Case {
            id: 28_302,
            name: "token count formatting",
            models: vec![
                model("a", "alpha", 1_000_000.0, 200_000.0, true, true),
                model("b", "alpha", 1_234_567.0, 1_250_000.0, false, false),
                model("c", "alpha", 1_150_000.0, 999_950.0, false, true),
                model("d", "alpha", 999.0, 1_000.0, true, false),
                model("e", "alpha", 0.0, 1_999_999.0, false, false),
            ],
            search: None,
            load_error: None,
        },
        Case {
            id: 28_303,
            name: "provider and id sorting",
            models: vec![
                model("z-model", "zeta", 1000.0, 100.0, false, false),
                model("a-model", "alpha", 1000.0, 100.0, false, false),
                model("b-model", "alpha", 1000.0, 100.0, false, false),
                model("a-model", "beta", 1000.0, 100.0, false, false),
            ],
            search: None,
            load_error: None,
        },
        Case {
            id: 28_304,
            name: "fuzzy search match",
            models: vec![
                model("claude-sonnet-5", "anthropic", 200_000.0, 8192.0, true, true),
                model("gpt-4.1", "openai", 1_047_576.0, 32_768.0, false, true),
                model("claude-haiku-4-5", "anthropic", 200_000.0, 8192.0, false, true),
            ],
            search: Some("sonnet"),
            load_error: None,
        },
        Case {
            id: 28_305,
            name: "fuzzy search across provider and id",
            models: vec![
                model("claude-sonnet-5", "anthropic", 200_000.0, 8192.0, true, true),
                model("gpt-4.1", "openai", 1_047_576.0, 32_768.0, false, true),
            ],
            search: Some("openai gpt"),
            load_error: None,
        },
        Case {
            id: 28_306,
            name: "search with no matches",
            models: vec![model("gpt-4.1", "openai", 1_047_576.0, 32_768.0, false, true)],
            search: Some("missing-model"),
            load_error: None,
        },
        Case {
            id: 28_307,
            name: "empty search pattern keeps every model",
            models: vec![
                model("b", "beta", 1000.0, 100.0, false, false),
                model("a", "alpha", 1000.0, 100.0, true, true),
            ],
            search: Some("   "),
            load_error: None,
        },
        Case {
            id: 28_308,
            name: "unicode identifiers and widths",
            models: vec![
                model("モデル-é", "プロバイダー", 1000.0, 100.0, false, false),
                model("ascii", "ascii", 2000.0, 200.0, true, true),
            ],
            search: None,
            load_error: None,
        },
        Case {
            id: 28_309,
            name: "one fuzzy token spans provider and model id",
            models: vec![model("gpt-4.1", "openai", 1000.0, 100.0, false, false)],
            search: Some("opengpt"),
            load_error: None,
        },
        Case {
            id: 28_310,
            name: "empty load error has no warning",
            models: vec![model("gpt-4.1", "openai", 1000.0, 100.0, false, false)],
            search: None,
            load_error: Some(""),
        },
    ]
}

fn source_package_dir() -> String {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .expect("source lock must be valid JSON");
    format!(
        "{}/packages/coding-agent",
        lock["worktree"].as_str().expect("worktree path")
    )
}

#[test]
fn list_models_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("list-models-oracle.ts", "packages/coding-agent/src/cli/list-models.ts");
    let source_package_dir = source_package_dir();
    let rust_package_dir = pi_coding_agent::config::get_package_dir().unwrap().to_utf8().unwrap();

    for case in cases() {
        let expected = oracle.request(&json!({
            "id": case.id,
            "operation": "list_models",
            "models": case.models,
            "searchPattern": case.search,
            "loadError": case.load_error,
        }));

        let runtime = StubRuntime {
            error: case.load_error.map(JsString::from),
            available_error: None,
            models: case
                .models
                .iter()
                .map(|model| serde_json::from_value(model.clone()).expect("model fixture must deserialize"))
                .collect(),
        };
        let io = CaptureIo::default();
        let search = case.search.map(JsString::from);
        run_list(&runtime, search.as_ref(), &io).unwrap();
        let actual = json!({"id":case.id,"ok":true,"output":serde_json::to_value(io.snapshot(None)).unwrap()});
        let expected = substitute_package_dir(expected, &source_package_dir, &rust_package_dir);
        assert_eq!(expected, actual, "list models mismatch: {}", case.name);
    }
}

#[derive(Default)]
struct CaptureIo {
    events: Mutex<Vec<(&'static str, JsString)>>,
}
impl ListModelsIo for CaptureIo {
    fn stdout(&self, message: &JsString) {
        self.events.lock().unwrap().push(("stdout", message.clone()));
    }
    fn stderr(&self, message: &JsString) {
        self.events.lock().unwrap().push(("stderr", message.clone()));
    }
}
impl CaptureIo {
    fn snapshot(&self, failure: Option<&NativeNodeError>) -> JsValue {
        let events = self.events.lock().unwrap();
        let stdout = JsString::join(
            events
                .iter()
                .filter(|(stream, _)| *stream == "stdout")
                .map(|(_, message)| message),
            "",
        );
        let stderr = JsString::join(
            events
                .iter()
                .filter(|(stream, _)| *stream == "stderr")
                .map(|(_, message)| message),
            "",
        );
        let warning = if stderr.is_empty() {
            None
        } else {
            Some(stderr.strip_suffix("\n").unwrap_or(stderr.clone()))
        };
        let failure=failure.map(|error| {
            let mut properties=error.value().as_object().unwrap().clone();
            properties.shift_remove("name");properties.shift_remove("message");
            pi_js_compat::js_json!({"name":error.name,"message":error.message,"constructor":error.constructor_class().name(),"properties":JsValue::Object(properties)})
        });
        let events = events
            .iter()
            .map(|(stream, message)| pi_js_compat::js_json!({"stream":stream,"text":message}))
            .collect::<Vec<_>>();
        pi_js_compat::js_json!({"warning":warning,"stdout":stdout,"events":events,"failure":failure})
    }
}

fn run_list(runtime: &StubRuntime, search: Option<&JsString>, io: &CaptureIo) -> Result<(), NativeNodeError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(list_models(runtime, search, io))
}

#[test]
fn list_models_preserves_native_search_and_warning_before_availability_failure() {
    let mut oracle = oracle::TsOracle::spawn("list-models-oracle.ts", "packages/coding-agent/src/cli/list-models.ts");
    let search = JsString::from_utf16(vec![0xd800]);
    let models = vec![model("gpt-4.1", "openai", 1000.0, 100.0, false, false)];
    let expected = oracle.request(&pi_js_compat::js_json!({"id":28311,"models":models,"searchPattern":search}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let runtime = StubRuntime {
        error: None,
        models: models
            .iter()
            .map(|model| serde_json::from_value(model.clone()).unwrap())
            .collect(),
        available_error: None,
    };
    let io = CaptureIo::default();
    run_list(&runtime, Some(&search), &io).unwrap();
    assert_eq!(io.snapshot(None), expected["output"]);

    let warning = JsString::from_utf16(vec![0xdfff]);
    let message = JsString::from_utf16(vec![0xd800, b'!' as u16]);
    let expected = oracle.request(&pi_js_compat::js_json!({"id":28312,"models":[],"loadError":warning,
        "availableError":{"name":"TypeError","message":message,"code":"E_FIXTURE"}}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let original = NativeNodeError::from_constructor(NativeErrorClass::TypeError, message)
        .with_property("code", JsValue::String("E_FIXTURE".into()));
    let runtime = StubRuntime {
        error: Some(warning),
        models: Vec::new(),
        available_error: Some(original.clone()),
    };
    let io = CaptureIo::default();
    let failure = run_list(&runtime, None, &io).unwrap_err();
    assert_eq!(failure, original, "propagate the complete concrete error");
    assert_eq!(io.snapshot(Some(&failure)), expected["output"]);
}

/// The guidance text embeds the install directory, which differs between the
/// TypeScript source worktree and the Rust test binary.
fn substitute_package_dir(mut value: Value, from: &str, to: &str) -> Value {
    fn walk(value: &mut Value, from: &str, to: &str) {
        match value {
            Value::String(text) => *text = text.replace(from, to),
            Value::Array(items) => {
                for item in items {
                    walk(item, from, to);
                }
            }
            Value::Object(entries) => {
                for entry in entries.values_mut() {
                    walk(entry, from, to);
                }
            }
            _ => {}
        }
    }
    walk(&mut value, from, to);
    value
}
