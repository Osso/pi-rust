use super::*;
use crate::core::{
    auth_storage::AuthStorage,
    extensions::{ExtensionRunner, types::*},
    model_registry::ModelRegistry,
    session_manager::SessionManager,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{Arc, Mutex};

pub(super) fn runner(
    dir: &std::path::Path,
    loaded: LoadExtensionsResult,
    model: Option<pi_ai::types::Model>,
) -> ExtensionRunner {
    let cwd: JsString = dir.to_str().unwrap().into();
    let sessions = Arc::new(Mutex::new(SessionManager::in_memory(Some(&cwd), None).unwrap()));
    let models = Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(JsString, JsValue)>::new())).unwrap());
    let actions = loaded.runtime.actions();
    let mut runner = ExtensionRunner::new(loaded.extensions, loaded.runtime, cwd, sessions, models, None);
    runner
        .bind_core(
            actions,
            ExtensionContextActions {
                get_model: Arc::new(move || model.clone()),
                get_thinking_level: None,
                set_thinking_level: None,
                get_scoped_models: None,
                get_footer_data: None,
                is_idle: Arc::new(|| true),
                has_active_retry: Arc::new(|| false),
                is_project_trusted: Arc::new(|| true),
                get_signal: Arc::new(|| None),
                abort: Arc::new(|| {}),
                has_pending_messages: Arc::new(|| false),
                shutdown: Arc::new(|| {}),
                restart: Arc::new(|_| Box::pin(async { Ok(()) })),
                get_control_db_path: None,
                get_context_usage: Arc::new(|| None),
                get_multi_agent_agent_id: None,
                get_multi_agent_parent_session_id: None,
                register_supervisor_review_cancellation: None,
                get_no_supervisor: None,
                get_multi_agent_requires_agent_id: None,
                get_multi_agent_store: None,
                get_tool_detach_registry: None,
                compact: Arc::new(|_| {}),
                get_system_prompt: Arc::new(JsString::default),
                get_system_prompt_options: None,
            },
            None,
        )
        .unwrap();
    runner
}

#[tokio::test]
async fn claude_bridge_lifecycle_tree_navigation_rebuilds_its_session_before_reuse() {
    let (dir, config, loaded) = integration_tests::setup().await;
    let model = integration_tests::model(&config);
    let runner = runner(dir.path(), loaded, Some(model.clone()));
    let key = runner.create_context().session_manager().unwrap().get_session_id();
    let mut opts = pi_ai::request::SimpleStreamOptions::default();
    opts.stream.cwd = Some(dir.path().to_str().unwrap().into());
    opts.stream.session_id = Some(key);
    let mut context: pi_ai::types::Context = js_json::from_value(
        &js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"hello","timestamp":1}]}"#).unwrap(),
    )
    .unwrap();
    let stream = config.stream_simple.as_ref().unwrap();
    let first = stream(model.clone(), context.clone(), Some(opts.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, pi_ai::types::StopReason::Stop);
    context.messages.push(pi_ai::types::Message::Assistant(Box::new(first)));
    context.messages.push(
        js_json::from_value(
            &js_json::parse_lossless(r#"{"role":"user","content":"assert-lifecycle-rebuild","timestamp":2}"#).unwrap(),
        )
        .unwrap(),
    );
    let mut event = ExtensionEvent::SessionTree(SessionTreeEvent {
        new_leaf_id: None,
        old_leaf_id: None,
        summary_entry: None,
        from_extension: None,
    });
    runner.emit(&mut event).await;
    let second = stream(model, context, Some(opts)).unwrap().result().await.unwrap();
    assert_eq!(
        second.stop_reason,
        pi_ai::types::StopReason::Stop,
        "{:?}",
        second.error_message
    );
}

#[tokio::test]
async fn claude_bridge_lifecycle_new_resume_and_fork_clear_old_mirrors() {
    for reason in [
        SessionStartReason::New,
        SessionStartReason::Resume,
        SessionStartReason::Fork,
    ] {
        let (dir, config, loaded) = integration_tests::setup().await;
        let model = integration_tests::model(&config);
        let runner = runner(dir.path(), loaded, Some(model.clone()));
        let mut opts = pi_ai::request::SimpleStreamOptions::default();
        opts.stream.cwd = Some(dir.path().to_str().unwrap().into());
        opts.stream.session_id = Some(runner.create_context().session_manager().unwrap().get_session_id());
        let mut context: pi_ai::types::Context = js_json::from_value(
            &js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"hello","timestamp":1}]}"#).unwrap(),
        )
        .unwrap();
        let stream = config.stream_simple.as_ref().unwrap();
        let first = stream(model.clone(), context.clone(), Some(opts.clone()))
            .unwrap()
            .result()
            .await
            .unwrap();
        context.messages.push(pi_ai::types::Message::Assistant(Box::new(first)));
        context.messages.push(
            js_json::from_value(
                &js_json::parse_lossless(r#"{"role":"user","content":"assert-session-cleared","timestamp":2}"#)
                    .unwrap(),
            )
            .unwrap(),
        );
        runner
            .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                reason,
                previous_session_file: None,
            }))
            .await;
        let second = stream(model, context, Some(opts)).unwrap().result().await.unwrap();
        assert_eq!(
            second.stop_reason,
            pi_ai::types::StopReason::Stop,
            "{reason:?}: {:?}",
            second.error_message
        );
    }
}

#[tokio::test]
async fn claude_bridge_lifecycle_agent_end_delivers_terminating_tool_result_and_waits_for_settlement() {
    let (dir, config, loaded) = integration_tests::setup().await;
    let model = integration_tests::model(&config);
    let runner = runner(dir.path(), loaded, Some(model.clone()));
    let mut options = pi_ai::request::SimpleStreamOptions::default();
    options.stream.cwd = Some(dir.path().to_str().unwrap().into());
    options.stream.session_id = Some(runner.create_context().session_manager().unwrap().get_session_id());
    let mut context: pi_ai::types::Context = js_json::from_value(&js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"terminating-tool","timestamp":1}],"tools":[{"name":"end_turn","description":"Finish","parameters":{"type":"object","properties":{"reason":{"type":"string"}}}}]}"#).unwrap()).unwrap();
    let first = config.stream_simple.as_ref().unwrap()(model.clone(), context.clone(), Some(options.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, pi_ai::types::StopReason::ToolUse);
    context.messages.push(pi_ai::types::Message::Assistant(Box::new(first)));
    context.messages.push(js_json::from_value(&js_json::parse_lossless(r#"{"role":"toolResult","toolCallId":"ending","toolName":"end_turn","content":[{"type":"text","text":"complete \udfff"}],"isError":false,"timestamp":2}"#).unwrap()).unwrap());
    let messages: Vec<pi_agent_core::agent::AgentMessage> =
        js_json::from_value(&js_json::to_value(&context.messages).unwrap()).unwrap();
    runner
        .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent {
            messages,
            session_continuation: None,
        }))
        .await;
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".fixture-end-turn-settled")).unwrap(),
        "settled"
    );
}

#[tokio::test]
async fn claude_bridge_shutdown_finishes_active_stream_and_reaps_its_process() {
    let (dir, config, loaded) = integration_tests::setup().await;
    let model = integration_tests::model(&config);
    let runner = runner(dir.path(), loaded, Some(model.clone()));
    let controller = pi_js_compat::abort::AbortController::new();
    let mut opts = pi_ai::request::SimpleStreamOptions::default();
    opts.stream.cwd = Some(dir.path().to_str().unwrap().into());
    opts.stream.session_id = Some(runner.create_context().session_manager().unwrap().get_session_id());
    opts.stream.request.signal = Some(controller.signal());
    let context = js_json::from_value(
        &js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"wait","timestamp":1}]}"#).unwrap(),
    )
    .unwrap();
    let stream = config.stream_simple.as_ref().unwrap()(model, context, Some(opts)).unwrap();
    let pid: u32 = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(dir.path().join(".fixture-pid"))
                && let Ok(pid) = text.parse()
            {
                break pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
    let result = tokio::time::timeout(std::time::Duration::from_millis(100), stream.result()).await;
    if result.is_err() {
        controller.abort(None);
        tokio::time::timeout(std::time::Duration::from_secs(1), stream.result())
            .await
            .unwrap()
            .unwrap();
    }
    let result = result.expect("shutdown must settle the active stream").unwrap();
    assert_eq!(result.stop_reason, pi_ai::types::StopReason::Error);
    assert_eq!(result.error_message.unwrap(), "pi session shut down");
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "shutdown must await child cleanup"
    );
}

#[tokio::test]
async fn claude_bridge_lifecycle_branch_summary_takes_over_navigation_and_cancels_on_error() {
    let (dir, config, loaded) = integration_tests::setup().await;
    let runner = runner(dir.path(), loaded, Some(integration_tests::model(&config)));
    let mut tree: SessionBeforeTreeEvent = js_json::from_value(&js_json::parse_lossless(r#"{"preparation":{"targetId":"target","entriesToSummarize":[{"type":"message","id":"u","parentId":null,"timestamp":"2026-10-25T00:00:00.000Z","message":{"role":"user","content":"branch history","timestamp":1}}],"userWantsSummary":true}}"#).unwrap()).unwrap();
    let result = runner.emit(&mut ExtensionEvent::SessionBeforeTree(tree.clone())).await;
    let crate::core::extensions::runner::EmitResult::SessionBeforeTree(result) = result else {
        panic!("bridge branch summary");
    };
    assert!(result.summary.unwrap().summary.contains("Claude: "));
    tree.preparation.custom_instructions = Some("summary-error".into());
    let result = runner.emit(&mut ExtensionEvent::SessionBeforeTree(tree)).await;
    let crate::core::extensions::runner::EmitResult::SessionBeforeTree(result) = result else {
        panic!("bridge branch failure");
    };
    assert_eq!(result.cancel, Some(true));
    assert!(result.summary.is_none());
}
