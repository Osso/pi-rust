use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use pi_coding_agent::core::agent_session_runtime::AgentSessionRuntime;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsObject, JsValue, parse_lossless, stringify_lossless};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

pub fn mark_pending() {
    if std::env::var_os("PI_HARNESS_PRINT_FIXTURE").is_none() {
        return;
    }
    let root = PathBuf::from(std::env::var("PI_HARNESS_PRINT_ROOT").expect("print fixture root"));
    let ready = pi_js_compat::js_json!({"pid":std::process::id()});
    std::fs::write(root.join("print-ready.pending"), stringify_lossless(&ready)).unwrap();
    std::fs::rename(root.join("print-ready.pending"), root.join("print-ready.json")).unwrap();
}

pub struct PrintFixture {
    root: PathBuf,
    capture: bool,
}

impl PrintFixture {
    pub fn configure(runtime: &AgentSessionRuntime) -> Option<Self> {
        let encoded = std::env::var("PI_HARNESS_PRINT_FIXTURE").ok()?;
        let config = parse_lossless(&encoded).expect("print fixture config");
        let root = PathBuf::from(std::env::var("PI_HARNESS_PRINT_ROOT").expect("print fixture root"));
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        if let Some(rejection) = config.get("promptRejection") {
            let rejection = make_rejection(rejection);
            runtime.session().set_working_directory_recovery_handlers(
                pi_coding_agent::core::agent_session::WorkingDirectoryRecoveryHandlers {
                    before_run: Arc::new(move || {
                        let rejection = rejection.clone();
                        Box::pin(async move { Err(rejection) })
                    }),
                    after_turn: Arc::new(|| Box::pin(async { Ok(()) })),
                },
            );
        }
        let rejection = config.get("disposeRejection").map(make_rejection);
        if rejection.is_some() || config.get("traceDispose").and_then(JsValue::as_bool) == Some(true) {
            let trace = root.join("print-trace.jsonl");
            runtime.set_before_session_invalidate(Some(Arc::new(move || {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&trace)
                    .unwrap();
                writeln!(file, "{{\"event\":\"beforeInvalidate\"}}").unwrap();
                match &rejection {
                    Some(rejection) => Err(rejection.clone()),
                    None => Ok(()),
                }
            })));
        }
        Some(Self {
            root,
            capture: config.get("capture").and_then(JsValue::as_bool) == Some(true),
        })
    }

    pub fn capture(&self) -> bool {
        self.capture
    }

    pub fn returned(&self, code: i32) {
        let report = pi_js_compat::js_json!({"returned":code});
        std::fs::write(self.root.join("print-report.json"), stringify_lossless(&report)).unwrap();
    }

    pub fn rejected(&self, error: CallbackError) {
        let report = JsValue::Object(JsObject::from([(
            "rejected".into(),
            serialize_rejection(error.thrown_value()),
        )]));
        std::fs::write(self.root.join("print-report.json"), stringify_lossless(&report)).unwrap();
    }
}

fn make_rejection(value: &JsValue) -> CallbackError {
    if value
        .get("kind")
        .and_then(JsValue::as_string)
        .is_some_and(|kind| kind == "raw")
    {
        return CallbackError::from_rejection(AbortReason::Value(value.get("value").unwrap().clone()));
    }
    let class = match value.get("name").and_then(JsValue::as_string).unwrap() {
        name if name == "Error" => NativeErrorClass::Error,
        name if name == "TypeError" => NativeErrorClass::TypeError,
        other => panic!("unsupported fixture class {other:?}"),
    };
    let mut error =
        NativeNodeError::from_constructor(class, value.get("message").unwrap().as_string().unwrap().clone());
    if let Some(code) = value.get("code") {
        error = error.with_property("code", code.clone());
    }
    error.into()
}

fn serialize_rejection(reason: AbortReason) -> JsValue {
    match reason {
        AbortReason::Error(error) | AbortReason::DomException(error) => JsValue::Object(JsObject::from([
            ("kind".into(), JsValue::String("error".into())),
            ("value".into(), error.value()),
            (
                "typeError".into(),
                JsValue::Bool(error.is_instance_of(NativeErrorClass::TypeError)),
            ),
        ])),
        AbortReason::Value(value) => JsValue::Object(JsObject::from([
            ("kind".into(), JsValue::String("raw".into())),
            ("value".into(), value),
        ])),
    }
}
