use super::*;
use pi_ai::request::CallbackError;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::{JsObject, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

fn rejection(kind: &str, value: &JsValue) -> CallbackError {
    if kind == "raw" {
        return CallbackError::from_rejection(AbortReason::Value(value.clone()));
    }
    NativeNodeError::from_constructor(
        NativeErrorClass::RangeError,
        value.get("message").unwrap().as_string().unwrap().clone(),
    )
    .with_property("code", value.get("code").unwrap().clone())
    .with_property("custom", value.get("custom").unwrap().clone())
    .into()
}

fn error_wire(error: &CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(native) | AbortReason::DomException(native) => JsValue::Object(JsObject::from([
            ("kind".into(), JsValue::String("error".into())),
            (
                "class".into(),
                JsValue::String(native.constructor_class().name().into()),
            ),
            ("value".into(), native.value()),
        ])),
        AbortReason::Value(value) => JsValue::Object(JsObject::from([
            ("kind".into(), JsValue::String("value".into())),
            ("value".into(), value),
        ])),
    }
}

struct RecordDetach(Arc<Mutex<Vec<String>>>);
impl pi_coding_agent::core::tool_detach_registry::ToolDetachHandle for RecordDetach {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.0.lock().unwrap().push("detach".into());
        Ok(false)
    }
}

#[tokio::test]
async fn before_restart_rejection_skips_teardown_and_restarter_like_frozen() {
    let mut source = oracle::TsOracle::spawn("agent-session-runtime-callback-oracle.mts", "packages/coding-agent/src");
    let value = pi_js_compat::js_json::parse_lossless(
        r#"{"message":"restart\ud800","code":"E_RESTART","custom":{"tail":"\udfff"}}"#,
    )
    .unwrap();
    for kind in ["native", "raw"] {
        let harness = build_harness(false, false).await;
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let registry = harness
            .runtime
            .session()
            .extension_runner()
            .create_context()
            .tool_detach_registry()
            .unwrap()
            .unwrap();
        let subscription = registry.register(Arc::new(RecordDetach(calls.clone())));
        let events = calls.clone();
        harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
            events.lock().unwrap().push("teardown".into());
            Ok(())
        })));
        let events = calls.clone();
        let failure = rejection(kind, &value);
        harness.runtime.set_before_process_restart(Some(Arc::new(move || {
            events.lock().unwrap().push("beforeRestart".into());
            let failure = failure.clone();
            Box::pin(async move { Err(failure) })
        })));
        let events = calls.clone();
        harness.runtime.set_process_restarter(Arc::new(move |_| {
            events.lock().unwrap().push("restarter".into());
            Box::pin(async { Err(CallbackError::new("unexpected restarter")) })
        }));
        let source_root = harness._temp.path().join("source-before-restart");
        std::fs::create_dir(&source_root).unwrap();
        let expected = source.request(&JsValue::Object(JsObject::from([
            ("root".into(), JsValue::String(source_root.to_str().unwrap().into())),
            ("operation".into(), JsValue::String("beforeRestart".into())),
            ("kind".into(), JsValue::String(kind.into())),
            ("value".into(), value.clone()),
        ])));
        let current = harness.runtime.session();
        let context = current.extension_runner().create_context();
        let error = harness.runtime.restart(None, true).await.unwrap_err();
        assert!(context.cwd().is_ok(), "hook rejection must leave outgoing context live");
        let visible = current.session_manager().get_entries().iter().any(|entry| {
            entry
                .as_custom_message()
                .is_some_and(|entry| entry.custom_type == "setup_visible")
        });
        let actual = JsValue::Object(JsObject::from([
            (
                "calls".into(),
                pi_js_compat::js_json::to_value(&*calls.lock().unwrap()).unwrap(),
            ),
            ("error".into(), error_wire(&error)),
            ("setupVisible".into(), JsValue::Bool(visible)),
            (
                "agentMessageCount".into(),
                JsValue::Number(current.messages().len() as f64),
            ),
        ]));
        assert_eq!(actual, expected, "{kind}");
        drop(subscription);
    }
}

// Frozen runtime:526-533 awaits setup before syncing agent messages/rebinding;
// a rejection keeps already-written setup entries, but skips later steps.
#[tokio::test]
async fn extension_setup_and_replacement_rejections_preserve_frozen_order_state_and_identity() {
    use pi_coding_agent::core::extensions::types::{HandlerError, NewSessionOptions as ExtensionNewSessionOptions};
    let mut source = oracle::TsOracle::spawn("agent-session-runtime-callback-oracle.mts", "packages/coding-agent/src");
    let value =
        pi_js_compat::js_json::parse_lossless(r#"{"message":"hook\ud800","code":"E_HOOK","custom":{"tail":"\udfff"}}"#)
            .unwrap();
    for operation in ["setup", "withSession"] {
        for kind in ["native", "raw"] {
            let calls = Arc::new(Mutex::new(Vec::<String>::new()));
            let events = calls.clone();
            let harness = build_harness_with_runtime_observer(
                false,
                false,
                false,
                Some(Arc::new(move |request| {
                    if request.session_start_event.as_ref().is_some_and(|event| {
                        event.reason != pi_coding_agent::core::extensions::types::SessionStartReason::Startup
                    }) {
                        events.lock().unwrap().push("create".into());
                    }
                })),
            )
            .await;
            let source_root = harness._temp.path().join("source-callback");
            std::fs::create_dir(&source_root).unwrap();
            let request = JsValue::Object(JsObject::from([
                ("root".into(), JsValue::String(source_root.to_str().unwrap().into())),
                ("operation".into(), JsValue::String(operation.into())),
                ("kind".into(), JsValue::String(kind.into())),
                ("value".into(), value.clone()),
            ]));
            let expected = source.request(&request);
            let events = calls.clone();
            harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
                events.lock().unwrap().push("teardown".into());
                Ok(())
            })));
            let events = calls.clone();
            harness.runtime.set_rebind_session(Some(Arc::new(move |_| {
                events.lock().unwrap().push("rebind".into());
                Box::pin(async { Ok(()) })
            })));
            let failure = HandlerError::from(rejection(kind, &value));
            let setup: pi_coding_agent::core::extensions::types::SessionSetupCallback = Arc::new({
                let events = calls.clone();
                let failure = failure.clone();
                move |manager| {
                    events.lock().unwrap().push("setup".into());
                    manager
                        .lock()
                        .unwrap()
                        .append_custom_message_entry(
                            &"setup_visible".into(),
                            pi_agent_core::harness::messages::CustomMessageContent::Text("setup payload".into()),
                            true,
                            None,
                        )
                        .unwrap();
                    let failure = failure.clone();
                    Box::pin(async move { if operation == "setup" { Err(failure) } else { Ok(()) } })
                }
            });
            let with_session: pi_coding_agent::core::extensions::types::WithSessionCallback = Arc::new({
                let events = calls.clone();
                move |_| {
                    events.lock().unwrap().push("withSession".into());
                    let failure = failure.clone();
                    Box::pin(async move {
                        if operation == "withSession" {
                            Err(failure)
                        } else {
                            Ok(())
                        }
                    })
                }
            });
            let actions = pi_coding_agent::modes::extension_actions::command_context_actions(
                Arc::new(Mutex::new(harness.runtime.session())),
                harness.runtime.clone(),
            );
            let error = (actions.new_session)(Some(ExtensionNewSessionOptions {
                parent_session: None,
                setup: Some(setup),
                with_session: Some(with_session),
            }))
            .await
            .unwrap_err();
            let current = harness.runtime.session();
            let visible = current.session_manager().get_entries().iter().any(|entry| {
                entry
                    .as_custom_message()
                    .is_some_and(|entry| entry.custom_type == "setup_visible")
            });
            let actual = JsValue::Object(JsObject::from([
                (
                    "calls".into(),
                    pi_js_compat::js_json::to_value(&*calls.lock().unwrap()).unwrap(),
                ),
                ("error".into(), error_wire(&CallbackError::from(error))),
                ("setupVisible".into(), JsValue::Bool(visible)),
                (
                    "agentMessageCount".into(),
                    JsValue::Number(current.messages().len() as f64),
                ),
            ]));
            assert_eq!(actual, expected, "{operation}/{kind}");
        }
    }
}

#[tokio::test]
async fn setup_writes_across_await_keep_live_state_and_sync_only_on_success_like_frozen() {
    use pi_coding_agent::core::extensions::types::{HandlerError, NewSessionOptions as ExtensionNewSessionOptions};
    let mut source = oracle::TsOracle::spawn("agent-session-runtime-callback-oracle.mts", "packages/coding-agent/src");
    for operation in ["asyncSetup", "asyncSetupReject"] {
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let events = calls.clone();
        let harness = build_harness_with_runtime_observer(
            false,
            false,
            false,
            Some(Arc::new(move |request| {
                if request.session_start_event.as_ref().is_some_and(|event| {
                    event.reason != pi_coding_agent::core::extensions::types::SessionStartReason::Startup
                }) {
                    events.lock().unwrap().push("create".into());
                }
            })),
        )
        .await;
        let root = harness._temp.path().join("source-async-setup");
        std::fs::create_dir(&root).unwrap();
        let value = JsValue::String("setup rejected after await".into());
        let expected = source.request(&JsValue::Object(JsObject::from([
            ("root".into(), JsValue::String(root.to_str().unwrap().into())),
            ("operation".into(), JsValue::String(operation.into())),
            ("kind".into(), JsValue::String("raw".into())),
            ("value".into(), value.clone()),
        ])));
        let events = calls.clone();
        harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
            events.lock().unwrap().push("teardown".into());
            Ok(())
        })));
        let events = calls.clone();
        harness.runtime.set_rebind_session(Some(Arc::new(move |_| {
            events.lock().unwrap().push("rebind".into());
            Box::pin(async { Ok(()) })
        })));
        let events = calls.clone();
        let setup: pi_coding_agent::core::extensions::types::SessionSetupCallback = Arc::new(move |manager| {
            events.lock().unwrap().push("setup".into());
            manager
                .lock()
                .unwrap()
                .append_custom_message_entry(
                    &"setup_visible".into(),
                    pi_agent_core::harness::messages::CustomMessageContent::Text("setup payload".into()),
                    true,
                    None,
                )
                .unwrap();
            let events = events.clone();
            let failure = HandlerError::from(rejection("raw", &value));
            Box::pin(async move {
                tokio::task::yield_now().await;
                events.lock().unwrap().push("afterAwait".into());
                manager
                    .lock()
                    .unwrap()
                    .append_custom_message_entry(
                        &"setup_after_await".into(),
                        pi_agent_core::harness::messages::CustomMessageContent::Text("after await".into()),
                        true,
                        None,
                    )
                    .unwrap();
                if operation == "asyncSetupReject" {
                    Err(failure)
                } else {
                    Ok(())
                }
            })
        });
        let events = calls.clone();
        let actions = pi_coding_agent::modes::extension_actions::command_context_actions(
            Arc::new(Mutex::new(harness.runtime.session())),
            harness.runtime.clone(),
        );
        let result = (actions.new_session)(Some(ExtensionNewSessionOptions {
            parent_session: None,
            setup: Some(setup),
            with_session: Some(Arc::new(move |_| {
                events.lock().unwrap().push("withSession".into());
                Box::pin(async { Ok(()) })
            })),
        }))
        .await;
        let error = match result {
            Ok(result) => {
                assert!(!result.cancelled);
                JsValue::Object(JsObject::from([("kind".into(), JsValue::String("value".into()))]))
            }
            Err(error) => error_wire(&CallbackError::from(error)),
        };
        let current = harness.runtime.session();
        let entries = current
            .session_manager()
            .get_entries()
            .iter()
            .filter_map(|entry| entry.as_custom_message())
            .map(|entry| {
                JsValue::Object(JsObject::from([
                    ("customType".into(), JsValue::String(entry.custom_type.clone())),
                    (
                        "content".into(),
                        pi_js_compat::js_json::to_value(&entry.content).unwrap(),
                    ),
                ]))
            })
            .collect::<Vec<_>>();
        let actual = JsValue::Object(JsObject::from([
            (
                "calls".into(),
                pi_js_compat::js_json::to_value(&*calls.lock().unwrap()).unwrap(),
            ),
            ("error".into(), error),
            (
                "setupVisible".into(),
                JsValue::Bool(
                    entries
                        .iter()
                        .any(|entry| entry.get("customType") == Some(&JsValue::String("setup_visible".into()))),
                ),
            ),
            (
                "agentMessageCount".into(),
                JsValue::Number(current.messages().len() as f64),
            ),
            ("setupEntries".into(), JsValue::Array(entries)),
        ]));
        assert_eq!(actual, expected, "{operation}");
    }
}
