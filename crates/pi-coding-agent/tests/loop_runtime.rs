#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use bundled::{Harness, HarnessOptions};
use pi_coding_agent::core::extensions::types::{AgentEndEvent, AgentEndSessionContinuation, SessionShutdownEvent};
use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager, lock_session_manager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use std::sync::{Arc, Mutex, atomic::Ordering};

const SESSION_ONE: &str = "00000000-0000-4000-8000-000000000001";

async fn run_case(ops: &Value) -> Value {
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(|pi| {
            let result = pi_coding_agent::bundled_extensions::r#loop::loop_extension(pi);
            Box::pin(async move { result })
        }),
        &"/repo".into(),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<first-party:loop>".into(),
    )
    .await
    .expect("extension");
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(
            Some(&"/repo".into()),
            Some(NewSessionOptions {
                id: Some(SESSION_ONE.into()),
                ..Default::default()
            }),
        )
        .expect("session"),
    ));
    let mut options = HarnessOptions::new("/repo");
    options.session_manager = manager.clone();
    let harness = Harness::new(vec![extension.clone()], runtime.clone(), options).await;
    let sends = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut actions = runtime.actions();
    actions.send_message = {
        let sends = sends.clone();
        Arc::new(move |message, options| {
            sends.lock().unwrap().push(json!([message, options]));
            Ok(())
        })
    };
    runtime.set_actions(actions);
    let command = extension
        .read()
        .commands
        .get(&JsString::from("loop"))
        .expect("command")
        .handler
        .clone();
    let handlers = extension.read().handlers.clone();
    let mut steps = Vec::new();
    for op in ops.as_array().expect("ops") {
        let result = match op["kind"].as_string().expect("kind").to_utf8().unwrap().as_str() {
            "command" => {
                command(
                    op["args"].as_string().cloned().unwrap_or_default(),
                    harness.command_context(),
                )
                .await
            }
            "tick" => {
                tokio::task::yield_now().await;
                tokio::time::advance(std::time::Duration::from_secs(1)).await;
                for _ in 0..4 {
                    tokio::task::yield_now().await;
                }
                Ok(())
            }
            "state" => {
                if let Some(idle) = op["idle"].as_bool() {
                    harness.idle.store(idle, Ordering::SeqCst);
                }
                if let Some(pending) = op["pending"].as_bool() {
                    harness.pending.store(pending, Ordering::SeqCst);
                }
                if let Some(id) = op["sessionId"].as_string() {
                    lock_session_manager(&manager)
                        .new_session(Some(NewSessionOptions {
                            id: Some(id.clone()),
                            ..Default::default()
                        }))
                        .expect("session switch");
                }
                Ok(())
            }
            "end" => {
                let messages = if op["include"].as_bool() == Some(true) {
                    let sends = sends.lock().unwrap();
                    let sent = if op["old"].as_bool() == Some(true) {
                        sends.first()
                    } else {
                        sends.last()
                    };
                    sent.map(|sent| {
                        let mut message = sent[0].as_object().expect("message").clone();
                        message.insert("role".into(), json!("custom"));
                        vec![pi_agent_core::agent::AgentMessage::Custom(message)]
                    })
                    .unwrap_or_default()
                } else {
                    Vec::new()
                };
                if let Some(handler) = handlers.agent_end.first() {
                    handler
                        .call(
                            AgentEndEvent {
                                messages,
                                session_continuation: op["continuation"]
                                    .as_bool()
                                    .filter(|value| *value)
                                    .map(|_| AgentEndSessionContinuation::CwdRelocation),
                            },
                            harness.context(),
                        )
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            "shutdown" => handlers.session_shutdown[0]
                .call(
                    SessionShutdownEvent {
                        reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Quit,
                        target_session_file: None,
                    },
                    harness.context(),
                )
                .await
                .map(|_| ()),
            _ => panic!("unknown op"),
        };
        steps.push(json!({"sent": sends.lock().unwrap().len(), "error": result.err().map(|error| error.message)}));
    }
    let sends = sends.lock().unwrap().clone();
    handlers.session_shutdown[0]
        .call(
            SessionShutdownEvent {
                reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Quit,
                target_session_file: None,
            },
            harness.context(),
        )
        .await
        .expect("cleanup");
    json!({"steps": steps, "sends": sends})
}

#[tokio::test(start_paused = true)]
async fn loop_runtime_matches_frozen_deferral_generation_and_session_guards() {
    let cases = vec![
        json!([{"kind": "command", "args": "1s continue"}, {"kind": "tick"}, {"kind": "tick"}, {"kind": "end", "include": true}, {"kind": "tick"}, {"kind": "command", "args": "stop"}, {"kind": "tick"}]),
        json!([{"kind": "command", "args": "1s busy"}, {"kind": "state", "idle": false}, {"kind": "tick"}, {"kind": "tick"}, {"kind": "end"}, {"kind": "tick"}, {"kind": "end", "include": true}, {"kind": "tick"}]),
        json!([{"kind": "command", "args": "1s pending"}, {"kind": "state", "pending": true}, {"kind": "tick"}, {"kind": "end"}, {"kind": "state", "pending": false}, {"kind": "end"}, {"kind": "tick"}]),
        json!([{"kind": "command", "args": "1s initial"}, {"kind": "tick"}, {"kind": "command", "args": "1s replacement"}, {"kind": "tick"}, {"kind": "end", "include": true, "old": true}, {"kind": "tick"}, {"kind": "end", "include": true}, {"kind": "tick"}]),
        json!([{"kind": "command", "args": "1s relocation"}, {"kind": "state", "idle": false}, {"kind": "tick"}, {"kind": "end", "continuation": true}, {"kind": "end"}]),
        json!([{"kind": "command", "args": "1s session"}, {"kind": "state", "sessionId": "00000000-0000-4000-8000-000000000002"}, {"kind": "tick"}, {"kind": "end"}, {"kind": "state", "sessionId": SESSION_ONE}, {"kind": "tick"}]),
        json!([{"kind": "command", "args": "1s shutdown"}, {"kind": "shutdown"}, {"kind": "tick"}, {"kind": "command", "args": "1s rejected"}]),
        json!([{"kind": "command", "args": JsString::from("1s ") + JsString::from_utf16(vec![0xd800, 0x61, 0xdc00])}, {"kind": "tick"}]),
    ];
    let mut ts = oracle::TsOracle::spawn(
        "loop-runtime-oracle.ts",
        "packages/coding-agent/extensions/loop/src/index.ts",
    );
    let response = ts.request(&json!({"id": 1, "operation": "runtime", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle {response:?}");
    for (index, ops) in cases.iter().enumerate() {
        assert_eq!(run_case(ops).await, response["output"][index], "case {index}");
    }
}
