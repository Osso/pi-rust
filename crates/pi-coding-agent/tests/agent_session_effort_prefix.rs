use super::*;
use pi_coding_agent::core::footer_data_provider::ExtensionStatus;
use std::sync::Mutex;

struct EffortUi {
    theme: Arc<pi_coding_agent::modes::interactive::theme::Theme>,
    statuses: Mutex<std::collections::BTreeMap<JsString, JsString>>,
}
impl ExtensionUiContext for EffortUi {
    fn theme(&self) -> Result<Arc<pi_coding_agent::modes::interactive::theme::Theme>, pi_ai::request::CallbackError> {
        Ok(Arc::clone(&self.theme))
    }
    fn set_status(&self, key: &JsString, text: Option<ExtensionStatus>) -> Result<(), pi_ai::request::CallbackError> {
        let mut statuses = self.statuses.lock().unwrap();
        match text {
            Some(ExtensionStatus::Text(text)) => {
                statuses.insert(key.clone(), text);
            }
            None => {
                statuses.remove(key);
            }
            Some(_) => panic!("unexpected callable effort status"),
        }
        Ok(())
    }
}
fn effort_observation(session: &AgentSession, ui: &EffortUi) -> serde_json::Value {
    let mode = session
        .session_manager()
        .get_entries()
        .into_iter()
        .rev()
        .find_map(|entry| match entry {
            pi_coding_agent::core::session_manager::SessionEntry::Known(
                pi_coding_agent::core::session_manager::KnownSessionEntry::Custom(entry),
            ) if entry.custom_type == "multi-agent-mode" => {
                entry.data.as_ref().and_then(|data| data.get("mode")).cloned()
            }
            _ => None,
        });
    let status = ui
        .statuses
        .lock()
        .unwrap()
        .get(&JsString::from("multi-agent-mode"))
        .cloned();
    json!({"mode":mode,"tools":session.get_active_tool_names(),"status":status.map(|status|pi_tui::utils::strip_terminal_sequences(&status))})
}

#[tokio::test]
async fn real_effort_mode_tools_and_status_change_before_thinking_setter_returns_like_frozen() {
    use pi_coding_agent::core::extensions::ExtensionFactory;
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let factory: ExtensionFactory = Arc::new(|api| {
        pi_coding_agent::bundled_extensions::effort::effort_extension(api).unwrap();
        api.register_tool(pi_coding_agent::core::extensions::types::ToolDefinition {
            name: "spawn_agent".into(),
            label: "Spawn".into(),
            description: "Unused fixture tool".into(),
            parameters: json!({"type":"object","properties":{}}),
            prompt_snippet: None,
            prompt_guidelines: None,
            approval_required: None,
            approval_kind: None,
            render_shell: None,
            prepare_arguments: None,
            prepare_execution: None,
            execution_mode: None,
            execute: Arc::new(|_, _, _, _, _| {
                Box::pin(async {
                    Ok(pi_agent_core::AgentToolResult {
                        content: Vec::new(),
                        details: None,
                        is_error: None,
                        terminate: None,
                    })
                })
            }),
            render_call: None,
            render_result: None,
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    let ui = Arc::new(EffortUi {
        theme: pi_coding_agent::modes::interactive::theme::get_theme_by_name(&"dark".into()).expect("dark theme"),
        statuses: Mutex::new(Default::default()),
    });
    fixture
        .session
        .bind_extensions(ExtensionBindings {
            ui_context: Some(ui.clone()),
            mode: Some(ExtensionMode::Tui),
            ..Default::default()
        })
        .await
        .unwrap();
    fixture
        .session
        .set_active_tools_by_name(&["spawn_agent".into()])
        .unwrap();
    fixture
        .session
        .prompt("/multi-agent disabled", Default::default())
        .await
        .unwrap();
    let before = effort_observation(&fixture.session, &ui);
    let mut model = fixture.session.model();
    model.thinking_level_map = Some(serde_json::from_value(json!({"ultra":"high"})).unwrap());
    fixture.session.agent().set_model(model);
    fixture.session.set_thinking_level(ThinkingLevel::Ultra).unwrap();
    let after = effort_observation(&fixture.session, &ui);
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected=oracle.request(&json!({"operation":"thinking-effort-prefix","dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok":true,"value":{"before":{"mode":"disabled","tools":[],"status":"multi-agent: disabled"},"after":{"mode":"proactive","tools":["spawn_agent"],"status":"multi-agent: active"}}})
    );
    assert_eq!(json!({"before":before,"after":after}), expected["value"]);
}
