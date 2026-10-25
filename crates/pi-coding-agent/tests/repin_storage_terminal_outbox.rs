use crate::shared_oracle as oracle;

use pi_coding_agent::core::terminal_outbox_delivery::{
    is_terminal_outbox_cleanup_due, terminal_outbox_retention_threshold,
};
use pi_js_compat::node_error::NativeNodeError;
use serde_json::{Value, json};

fn projected(error: &NativeNodeError) -> Value {
    error.value().to_serde().unwrap()
}

#[test]
fn terminal_retention_helpers_match_frozen_fractional_and_nonfinite_numbers() {
    let mut ts = oracle::TsOracle::spawn("storage-terminal-outbox-oracle.mts", "packages/coding-agent/src/core");
    let now_cases = [
        (0.0, None),
        (3_599_999.5, None),
        (3_600_000.0, None),
        (3_600_000.5, None),
        (8_640_000_604_800_000.0, None),
        (8_640_000_604_800_001.0, None),
        (-8_640_000_000_000_000.0, None),
        (f64::NAN, Some("nan")),
        (f64::INFINITY, Some("infinity")),
        (f64::NEG_INFINITY, Some("negativeInfinity")),
    ];
    for (now, kind) in now_cases {
        for (last, last_kind) in [
            (None, None),
            (Some(0.0), None),
            (Some(0.5), None),
            (Some(f64::NAN), Some("nan")),
        ] {
            let mut request = json!({"id":1,"op":"helpers"});
            if let Some(kind) = kind {
                request["nowKind"] = json!(kind);
            } else {
                request["now"] = json!(now);
            }
            if let Some(kind) = last_kind {
                request["lastKind"] = json!(kind);
            } else if let Some(last) = last {
                request["last"] = json!(last);
            }
            let expected = ts.request(&request);
            assert_eq!(expected["ok"], true);
            let threshold = match terminal_outbox_retention_threshold(now) {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":projected(&error)}),
            };
            assert_eq!(
                json!({"due":is_terminal_outbox_cleanup_due(last, now),"threshold":threshold}),
                expected["output"]
            );
        }
    }
}

#[test]
fn invalid_and_throwing_clocks_preserve_frozen_delivery_error_and_gate_order() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::terminal_outbox_delivery::{
        DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections,
    };
    use pi_js_compat::abort::AbortReason;
    use pi_js_compat::callback_error::CallbackError;
    use pi_js_compat::js_json::JsValue;
    use pi_js_compat::node_error::NativeErrorClass;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite");
    let mut ts = oracle::TsOracle::spawn("storage-terminal-outbox-oracle.mts", "packages/coding-agent/src/core");
    let created = ts.request(&json!({"id":1,"op":"create","dir":tmp.path(),"db":db}));
    assert_eq!(created["ok"], true);
    let path = created["output"]["path"].as_str().unwrap();
    for absent in [true, false] {
        for clock in ["not a date", "throw-error", "throw-null"] {
            let expected = ts.request(&json!({"id":2,"op":"deliver","dir":tmp.path(),"db":db,"path":path,"clock":clock,"noPersistence":absent}));
            let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
            if !absent {
                let mut session = SessionManager::open(&path.into(), None, None).unwrap();
                session
                    .set_metadata_control_db_path(Some(&db.to_str().unwrap().into()), Default::default())
                    .unwrap();
                store.set_persistence_session_manager(Some(&session));
            }
            let calls = Arc::new(AtomicUsize::new(0));
            let clock_calls = Arc::clone(&calls);
            let options = DeliverTerminalOutboxOptions {
                artifact_root: Some(tmp.path().to_str().unwrap().into()),
                claim_id: "fixture".into(),
                control_db_path: db.to_str().unwrap().into(),
                store,
                now: Arc::new(move || {
                    clock_calls.fetch_add(1, Ordering::SeqCst);
                    match clock {
                        "throw-error" => Err(CallbackError::from(
                            NativeNodeError::from_constructor(NativeErrorClass::TypeError, "clock failed")
                                .with_property("code", JsValue::String("FIXTURE".into())),
                        )),
                        "throw-null" => Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Null))),
                        _ => Ok(clock.into()),
                    }
                }),
            };
            let actual = match deliver_terminal_outbox_projections(&options) {
                Ok(delivered) => json!({"ok":true,"delivered":delivered,"calls":calls.load(Ordering::SeqCst)}),
                Err(error) => {
                    let error = match error.thrown_value() {
                        AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
                        AbortReason::Value(value) => value,
                    };
                    json!({"ok":false,"error":error.to_serde().unwrap(),"calls":calls.load(Ordering::SeqCst)})
                }
            };
            assert_eq!(
                JsValue::from(actual),
                JsValue::from(&expected["output"]),
                "absent={absent} clock={clock}"
            );
        }
    }
}
