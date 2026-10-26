//! Historical Mermaid rendering through the interactive terminal and a faux provider.

use crate::shared_pty as pty;
#[path = "support/scripted_registry.rs"]
mod scripted_registry;

use crate::shared_tui_virtual_terminal::VirtualTerminal;
use pi_ai::providers::faux::{FauxAssistantMessageOptions, FauxResponseStep, faux_assistant_message, faux_provider};
use pi_ai::types::StopReason;
use pi_coding_agent::core::{
    agent_session_runtime::{
        CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeResult, create_agent_session_runtime,
    },
    agent_session_services::AgentSessionServices,
    auth_storage::AuthStorage,
    model_registry::ModelRegistry,
    permissions::presets::ApprovalPresetName,
    resource_loader::{ResourceLoader, StaticResourceLoader},
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions, SettingsScope},
};
use pi_coding_agent::modes::interactive::interactive_mode::{InteractiveMode, InteractiveModeOptions};
use pi_js_compat::js_json::JsString;
use pi_tui::terminal::Terminal;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[test]
fn interactive_mermaid_renders_answer_but_not_thinking() {
    let root = tempfile::tempdir().unwrap();
    let env = [
        ("HOME".to_owned(), root.path().display().to_string()),
        (
            "XDG_CONFIG_HOME".to_owned(),
            root.path().join("config").display().to_string(),
        ),
        (
            "PI_CODING_AGENT_DIR".to_owned(),
            root.path().join("agent").display().to_string(),
        ),
        (
            "PI_CODING_AGENT_STATE_DIR".to_owned(),
            root.path().join("state").display().to_string(),
        ),
        ("PI_OFFLINE".to_owned(), "1".to_owned()),
        ("NO_COLOR".to_owned(), "1".to_owned()),
    ];
    let executable = std::env::current_exe().unwrap();
    let mut child = pty::spawn(
        &executable,
        &[
            "--ignored",
            "--exact",
            "interactive_mermaid_red::mermaid_interactive_child",
            "--nocapture",
        ],
        &env,
        root.path(),
    );
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
    child.write_input("Render fixture\r");
    child.wait_for("Turn ended: done", Duration::from_secs(20));
    std::thread::sleep(Duration::from_millis(250));
    let mut terminal = VirtualTerminal::new(120, 40);
    terminal.write(&child.output().into());
    let screen = terminal.viewport().join("\n");
    child.write_input("\u{3}\u{3}");
    assert_eq!(child.wait(Duration::from_secs(20)), 0);
    child.close_master();
    assert!(
        !screen.contains("No more faux responses queued"),
        "provider exhausted: {screen}"
    );
    let thinking = screen
        .split("THINKING-BEGIN")
        .nth(1)
        .unwrap()
        .split("THINKING-END")
        .next()
        .unwrap();
    assert!(
        thinking.contains("flowchart LR"),
        "thinking diagram must remain raw: {screen}"
    );
    assert!(
        thinking.contains("X[ThinkingStart] --> Y[ThinkingDone]"),
        "thinking source is missing: {screen}"
    );
    let answer = screen
        .split("ANSWER-BEGIN")
        .nth(1)
        .unwrap()
        .split("ANSWER-END")
        .next()
        .unwrap();
    assert!(
        answer.contains("┌───────┐"),
        "answer diagram must render Unicode boxes: {screen}"
    );
    assert!(
        answer.contains("───▶"),
        "answer diagram must render a Unicode arrow: {screen}"
    );
    assert!(
        !answer.contains("A[Start] --> B[Done]"),
        "answer source must not remain fenced: {screen}"
    );
}

#[test]
#[ignore = "interactive pty child"]
fn mermaid_interactive_child() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
        tokio::task::LocalSet::new().run_until(async {
            let cwd: JsString = std::env::current_dir().unwrap().to_str().unwrap().into();
            let agent_dir: JsString = std::env::var("PI_CODING_AGENT_DIR").unwrap().into();
            let faux = faux_provider(Default::default());
            let model = faux.get_model(None).unwrap();
            let mut response = faux_assistant_message("", FauxAssistantMessageOptions { stop_reason: Some(StopReason::ToolUse), ..Default::default() });
            response.content = pi_js_compat::js_json::from_value(&pi_js_compat::js_json!([
                {"type":"thinking","thinking":"THINKING-BEGIN\n```mermaid\nflowchart LR\n  X[ThinkingStart] --> Y[ThinkingDone]\n```\nTHINKING-END"},
                {"type":"text","text":"ANSWER-BEGIN\n\n```mermaid\nflowchart LR\n  A[Start] --> B[Done]\n```\n\nANSWER-END"},
                {"type":"toolCall","id":"finish","name":"end_turn","arguments":{"reason":"done"}}
            ])).unwrap();
            faux.set_responses(vec![FauxResponseStep::Message(Box::new(response))]);
            let auth = AuthStorage::in_memory([(model.provider.clone(), serde_json::json!({"type":"api_key","key":"fixture"}))]);
            let registry = ModelRegistry::in_memory(auth.clone()).unwrap();
            scripted_registry::register_scripted_provider(&registry, Arc::clone(&faux.provider));
            let mut settings = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
            settings.set_hide_thinking_block(false);
            settings.set_approval_preset(ApprovalPresetName::AutoApprove, SettingsScope::Global).unwrap();
            let settings = Arc::new(Mutex::new(settings));
            let loader: Arc<dyn ResourceLoader> = Arc::new(StaticResourceLoader::default());
            let factory: CreateAgentSessionRuntimeFactory = Arc::new(move |request| {
                let (loader, registry, auth, model, settings) = (loader.clone(), registry.clone(), auth.clone(), model.clone(), settings.clone());
                Box::pin(async move {
                    let result = create_agent_session(CreateAgentSessionOptions {
                        cwd: Some(request.cwd.clone()), agent_dir: Some(request.agent_dir.clone()),
                        session_manager: Some(request.session_manager), resource_loader: Some(loader.clone()),
                        settings_manager: Some(settings.clone()), model_registry: Some(registry.clone()),
                        auth_storage: Some(auth.clone()), model: Some(model), tools: Some(vec!["end_turn".to_owned()]), ..Default::default()
                    }).await.map_err(|error| error.message)?;
                    Ok(CreateAgentSessionRuntimeResult {
                        session: result.session,
                        services: AgentSessionServices { cwd: request.cwd, agent_dir: request.agent_dir, model_registry: registry, auth_storage: auth, settings_manager: settings, resource_loader: loader, diagnostics: vec![] },
                        diagnostics: vec![], model_fallback_message: None,
                    })
                })
            });
            let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
            let runtime = create_agent_session_runtime(factory, &cwd, &agent_dir, manager, None, None).await.unwrap();
            let mode = InteractiveMode::new(runtime, InteractiveModeOptions::default(), None).await.unwrap();
            mode.run().await.unwrap();
        })
    );
}
