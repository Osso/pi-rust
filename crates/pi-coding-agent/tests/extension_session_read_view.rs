#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, lock_session_manager};
use pi_js_compat::js_json;

#[tokio::test]
async fn context_model_captures_the_bound_getter_but_reads_its_current_value_like_frozen() {
    use pi_ai::types::Model;
    use pi_coding_agent::core::extensions::types::ExtensionContextActions;
    use std::sync::{Arc, Mutex};
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-boundary-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    );
    let expected = oracle.request(&js_json!({"id":1}));
    let mut runner = support::runner_for(Vec::new(), "/tmp", support::runtime(), support::model_registry().await);
    let before = runner.create_context();
    let command_before = runner.create_command_context();
    let make_model = |id: &str| -> Model {
        serde_json::from_value(serde_json::json!({"id":id,"name":id,"api":"faux","provider":"faux","baseUrl":"http://localhost:0","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap()
    };
    let current = Arc::new(Mutex::new(Some(make_model("first"))));
    let model_source = Arc::clone(&current);
    runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_model: Arc::new(move || model_source.lock().unwrap().clone()),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();
    let after = runner.create_context();
    let replacement = make_model("replacement");
    runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_model: Arc::new(move || Some(replacement.clone())),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();
    *current.lock().unwrap() = Some(make_model("mutated"));
    let output = [
        before.model().unwrap(),
        command_before.model().unwrap(),
        after.model().unwrap(),
        runner.create_context().model().unwrap(),
    ]
    .map(|model| model.map(|model| model.id));
    assert_eq!(js_json!({"id":1,"output":output}), expected);
}

#[tokio::test]
async fn runner_diagnostics_stringify_native_rejections_like_frozen() {
    use pi_ai::request::CallbackError;
    use pi_coding_agent::core::extensions::types::HandlerError;
    use pi_js_compat::abort::AbortReason;
    use std::sync::{Arc, Mutex};
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-boundary-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    );
    let values = vec![
        js_json!("plain string"),
        js_json!({"why":"failed"}),
        js_json!(123),
        js_json!(null),
        js_json!(pi_js_compat::js_json::JsString::from_utf16(vec![0xd800])),
    ];
    let expected = oracle.request(&js_json!({"id":2,"values":values}));
    let runtime = support::runtime();
    let mut extensions = Vec::new();
    for value in values {
        extensions.push(
            support::extension(&runtime, "/tmp", "<inline>", move |api| {
                let value = value.clone();
                api.on_session_start(
                    move |_,
                          _|
                          -> pi_coding_agent::core::extensions::types::BoxFuture<
                        'static,
                        Result<Option<()>, HandlerError>,
                    > {
                        let value = value.clone();
                        Box::pin(async move {
                            Err(HandlerError::from(CallbackError::from_rejection(AbortReason::Value(
                                value,
                            ))))
                        })
                    },
                )
                .unwrap();
            })
            .await,
        );
    }
    let runner = support::runner_for(extensions, "/tmp", runtime, support::model_registry().await);
    let output = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&output);
    let _listener = runner.on_error(Arc::new(move |error| {
        observed.lock().unwrap().push(error.error.clone());
    }));
    runner
        .emit(
            &mut pi_coding_agent::core::extensions::types::ExtensionEvent::SessionStart(
                pi_coding_agent::core::extensions::types::SessionStartEvent {
                    reason: pi_coding_agent::core::extensions::types::SessionStartReason::Startup,
                    previous_session_file: None,
                },
            ),
        )
        .await;
    assert_eq!(js_json!({"id":2,"output":output.lock().unwrap().clone()}), expected);
}

#[test]
fn extension_barrel_forwards_tool_and_source_contracts() {
    use pi_coding_agent::core::extensions::{
        AgentToolResult, BuildSystemPromptOptions, ExecOptions, ExecResult, KeybindingsManager, SlashCommandInfo,
        SlashCommandSource, SourceInfo, ToolExecutionMode,
    };
    let result = AgentToolResult {
        content: Vec::new(),
        details: Some(pi_js_compat::js_json::JsValue::Null),
        is_error: None,
        terminate: None,
    };
    assert_eq!(
        pi_js_compat::js_json::to_value(&result).unwrap(),
        js_json!({"content":[],"details":null})
    );
    let _: Option<(
        ToolExecutionMode,
        SourceInfo,
        SlashCommandInfo,
        SlashCommandSource,
        ExecOptions,
        ExecResult,
        BuildSystemPromptOptions,
        KeybindingsManager,
    )> = None;
}

#[test]
fn setup_writes_detach_all_five_caller_payloads_like_frozen() {
    use pi_coding_agent::core::messages::CustomMessageContent;
    use pi_coding_agent::core::session_manager::{CompactionAppendOptions, SessionManager};
    use pi_js_compat::js_json::{self, JsValue};
    let mut oracle = oracle::TsOracle::spawn(
        "session-entry-writes-oracle.ts",
        "packages/coding-agent/src/core/extensions/session-entry-writes.ts",
    );
    let expected = oracle.request(&js_json!({"id":1}));
    let mut session = SessionManager::in_memory(Some(&"/private/context".into()), None).unwrap();
    let mut data = js_json!({"nested":{"value":1}});
    let mut message = js_json!({"role":"user","content":[{"type":"text","text":"original"}],"timestamp":1});
    let first = session.append_message(js_json::from_value(&message).unwrap()).unwrap();
    let custom = session
        .append_custom_entry(&"setup".into(), Some(data.clone()))
        .unwrap();
    let notice = session
        .append_custom_message_entry(
            &"notice".into(),
            CustomMessageContent::Text("text".into()),
            true,
            Some(data.clone()),
        )
        .unwrap();
    let compact = session
        .append_compaction(
            &"summary".into(),
            &first,
            123.0,
            CompactionAppendOptions {
                details: Some(data.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let branch = session
        .branch_with_summary(Some(&first), &"branch".into(), Some(data.clone()), None)
        .unwrap();
    data["nested"]["value"] = JsValue::Number(99.0);
    message.get_mut("content").unwrap().as_array_mut().unwrap()[0]["text"] = JsValue::String("mutated".into());
    let payloads = [
        (&first, "message"),
        (&custom, "data"),
        (&notice, "details"),
        (&compact, "details"),
        (&branch, "details"),
    ]
    .into_iter()
    .map(|(id, key)| js_json::to_value(session.get_entry(id).unwrap()).unwrap()[key].clone())
    .collect::<Vec<_>>();
    assert_eq!(js_json!({"id":1,"output":payloads,"cwd":session.get_cwd()}), expected);
}

#[tokio::test]
async fn stale_command_wait_for_idle_reports_the_frozen_error_instead_of_succeeding() {
    let session = support::session_manager("/private/context");
    let runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        "/private/context",
        session,
        support::model_registry().await,
        None,
    );
    let command = runner.create_command_context();
    command.wait_for_idle().expect("active wait").await;
    runner.invalidate(Some("extension runtime replaced"));
    let error = command.wait_for_idle().err().expect("stale wait must reject");
    assert_eq!(error.message, "extension runtime replaced");
}

#[tokio::test]
async fn readonly_context_snapshot_is_isolated_and_retained_view_tracks_live_writes() {
    let session = support::session_manager("/private/context");
    let runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        "/private/context",
        session.clone(),
        support::model_registry().await,
        None,
    );
    let context = runner.create_context();
    let view = context.session_manager().unwrap();
    let id = lock_session_manager(&session)
        .append_custom_entry(&"probe".into(), Some(js_json!({"nested":{"value":1}})))
        .unwrap();
    let mut snapshot = view.get_entry(&id).unwrap();
    let SessionEntry::Known(KnownSessionEntry::Custom(entry)) = &mut snapshot else {
        panic!("custom entry");
    };
    entry.data = Some(js_json!({"nested":{"value":99}}));
    assert_ne!(view.get_entry(&id), Some(snapshot));
    assert_eq!(view.get_leaf_id(), Some(id));
    view.set_session_goal_json(&"{\"goal\":1}".into()).unwrap();
    assert_eq!(
        lock_session_manager(&session).get_session_goal_json().unwrap(),
        Some("{\"goal\":1}".into())
    );
    view.clear_session_goal_json().unwrap();
    assert_eq!(lock_session_manager(&session).get_session_goal_json().unwrap(), None);
    runner.invalidate(Some("extension runtime replaced"));
    assert_eq!(
        context.session_manager().err().unwrap().message,
        "extension runtime replaced"
    );
    assert_eq!(view.get_cwd(), "/private/context");
}
