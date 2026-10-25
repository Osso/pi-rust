#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, lock_session_manager};
use pi_js_compat::js_json;

#[test]
fn frozen_border_editor_render_theme_failure_is_process_uncaught() {
    let source = oracle::read_source_lock();
    oracle::verify_source_head(&source);
    let child = oracle::spawn_oracle_process(
        &source,
        "border-editor-render-uncaught.mts",
        "packages/coding-agent/examples/extensions/border-status-editor.ts",
        &[],
        std::process::Stdio::piped(),
    );
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(17),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"phase":"render-theme-getter","name":"Error","message":"border-render-theme-failed"})
    );
}

#[test]
fn uninitialized_sandbox_theme_rejects_like_frozen_without_panicking() {
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-boundary-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    );
    let expected = oracle.request(&js_json!({"id":4,"operation":"sandbox","initializeTheme":false,"actions":[{"profile":"full-access","scope":"global"}]}));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "extension_session_read_view::sandbox_theme_uninitialized_probe",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let result = text
        .lines()
        .find_map(|line| line.strip_prefix("THEME_RESULT "))
        .unwrap();
    assert_eq!(pi_js_compat::js_json::parse_lossless(result).unwrap(), expected);
}

#[tokio::test]
#[ignore = "child helper for uninitialized_sandbox_theme_rejects_like_frozen_without_panicking"]
async fn sandbox_theme_uninitialized_probe() {
    use pi_coding_agent::core::{
        permissions::presets::{SandboxProfileName, SandboxProfileScope},
        settings_manager::SettingsManager,
    };
    use std::sync::{Arc, Mutex};
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        Default::default(),
    )));
    let runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        "/tmp",
        support::session_manager("/tmp"),
        support::model_registry().await,
        Some(Arc::clone(&settings)),
    );
    let error = runner
        .create_command_context()
        .set_sandbox_profile(Some(SandboxProfileName::FullAccess), SandboxProfileScope::Global)
        .unwrap_err();
    let value = js_json!({"id":4,"output":{"results":[error.message],"profile":settings.lock().unwrap().get_explicit_sandbox_profile()}});
    println!("THEME_RESULT {}", pi_js_compat::js_json::stringify_lossless(&value));
}

#[tokio::test]
async fn command_sandbox_setter_uses_constructor_settings_without_command_binding_like_frozen() {
    use pi_coding_agent::core::{
        permissions::presets::{SandboxProfileName, SandboxProfileScope},
        settings_manager::SettingsManager,
    };
    use std::sync::{Arc, Mutex};
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "extensions-boundary-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    );
    let expected = oracle.request(&js_json!({"id":3,"operation":"sandbox","actions":[{"profile":"full-access","scope":"global"},{"scope":"global"},{"profile":"workspace-write","scope":"session"},{"profile":"read-only","scope":"global","reset":true}]}));
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        Default::default(),
    )));
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        "/tmp",
        support::session_manager("/tmp"),
        support::model_registry().await,
        Some(Arc::clone(&settings)),
    );
    let mut results = Vec::new();
    for (profile, scope, reset) in [
        (Some(SandboxProfileName::FullAccess), SandboxProfileScope::Global, false),
        (None, SandboxProfileScope::Global, false),
        (
            Some(SandboxProfileName::WorkspaceWrite),
            SandboxProfileScope::Session,
            false,
        ),
        (Some(SandboxProfileName::ReadOnly), SandboxProfileScope::Global, true),
    ] {
        if reset {
            runner.bind_command_context(None);
        }
        results.push(
            runner
                .create_command_context()
                .set_sandbox_profile(profile, scope)
                .err()
                .map(|error| error.message),
        );
    }
    assert_eq!(
        js_json!({"id":3,"output":{"results":results,"profile":settings.lock().unwrap().get_explicit_sandbox_profile()}}),
        expected
    );
}

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

#[tokio::test]
async fn core_and_sdk_barrels_forward_frozen_public_contracts() {
    use pi_coding_agent::core::sdk::{
        ExtensionApi, ExtensionCommandContext, ExtensionContext, ExtensionFactory, PromptTemplate, Skill,
        SlashCommandInfo, SlashCommandSource, ToolDefinition, create_agent_session_runtime, create_bash_tool,
        create_coding_tools, create_edit_tool, create_find_tool, create_grep_tool, create_ls_tool, create_outline_tool,
        create_read_only_tools, create_read_tool, create_references_tool, create_symbol_tool, create_write_tool,
        with_file_mutation_queue,
    };
    use pi_coding_agent::core::{
        AgentToolResult, AgentToolUpdateCallback, BashExecutorOptions, BashResult, ExecOptions, ExecResult,
        ExtensionHandler, ModelRequestEndEvent, ModelRequestStartEvent, execute_bash_with_operations,
    };
    let _: Option<(
        AgentToolUpdateCallback,
        BashExecutorOptions,
        BashResult,
        ExecOptions,
        ExecResult,
        ModelRequestEndEvent,
        ModelRequestStartEvent,
        ExtensionApi,
        ExtensionCommandContext,
        ExtensionContext,
        ExtensionFactory,
        PromptTemplate,
        Skill,
        SlashCommandInfo,
        SlashCommandSource,
        ToolDefinition,
    )> = None;
    let _ = (
        execute_bash_with_operations,
        create_agent_session_runtime,
        create_coding_tools,
        create_read_only_tools,
    );
    let cwd = pi_js_compat::js_json::JsString::from("/tmp");
    let tools = [
        create_bash_tool(&cwd, Default::default()),
        create_edit_tool(&cwd, Default::default()),
        create_find_tool(&cwd, Default::default()),
        create_grep_tool(&cwd, Default::default()),
        create_ls_tool(&cwd, Default::default()),
        create_outline_tool(&cwd, Default::default()),
        create_read_tool(&cwd, Default::default()),
        create_references_tool(&cwd, Default::default()),
        create_symbol_tool(&cwd, Default::default()),
        create_write_tool(&cwd, Default::default()),
    ];
    assert_eq!(
        tools.iter().map(|tool| tool.name()).collect::<Vec<_>>(),
        [
            "bash",
            "edit",
            "find",
            "grep",
            "ls",
            "outline",
            "read",
            "references",
            "symbol",
            "write"
        ]
    );
    assert_eq!(with_file_mutation_queue(&cwd, || async { 123 }).await.unwrap(), 123);
    let _: Option<
        std::sync::Arc<dyn ExtensionHandler<pi_coding_agent::core::extensions::types::SessionStartEvent, ()>>,
    > = None;
    let result = AgentToolResult {
        content: vec![],
        details: Some(pi_js_compat::js_json::JsValue::Null),
        is_error: None,
        terminate: None,
    };
    assert_eq!(
        pi_js_compat::js_json::to_value(&result).unwrap(),
        js_json!({"content":[],"details":null})
    );
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
