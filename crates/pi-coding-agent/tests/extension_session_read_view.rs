#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, lock_session_manager};
use pi_js_compat::js_json;

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
