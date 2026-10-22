use std::{cell::Cell, rc::Rc};

use pi_agent_core::ThinkingLevel;
use pi_coding_agent::modes::interactive::components::{
    show_images_selector::ShowImagesSelectorComponent, thinking_selector::ThinkingSelectorComponent,
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;

fn init_theme() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
}

#[test]
fn thinking_selector_retains_selection_after_unmatched_input() {
    init_theme();
    let selected = Rc::new(Cell::new(None));
    let callback = selected.clone();
    let mut selector = ThinkingSelectorComponent::new(
        ThinkingLevel::High,
        &[ThinkingLevel::Low, ThinkingLevel::High],
        Rc::new(move |level| callback.set(Some(level))),
        Rc::new(|| {}),
    );
    let list = selector.get_select_list();
    list.borrow_mut().handle_input(&JsString::from_utf16(vec![0xd800]));
    assert!(JsString::join(selector.render(80), "\n").contains("Deep reasoning"));
    list.borrow_mut().handle_input(&"\r".into());
    assert_eq!(selected.get(), Some(ThinkingLevel::High));
}

#[test]
fn image_selector_retains_selection_after_unmatched_input() {
    init_theme();
    let selected = Rc::new(Cell::new(None));
    let callback = selected.clone();
    let mut selector =
        ShowImagesSelectorComponent::new(false, Rc::new(move |show| callback.set(Some(show))), Rc::new(|| {}));
    let list = selector.get_select_list();
    list.borrow_mut().handle_input(&JsString::from_utf16(vec![0xdc00]));
    assert!(JsString::join(selector.render(80), "\n").contains("Show text placeholder instead"));
    list.borrow_mut().handle_input(&"\r".into());
    assert_eq!(selected.get(), Some(false));
}

#[test]
fn user_message_keeps_raw_units_across_padding_and_invalidation() {
    use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
    init_theme();
    let text = JsString::from("raw ") + JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    let mut message = UserMessageComponent::new(text.clone(), None, 1, vec![]);
    for padding in [1, 2] {
        message.set_output_pad(padding);
        message.invalidate();
        let output = JsString::join(message.render(80), "\n");
        assert!(output.contains(&text));
        assert!(output.contains("\u{1b}]133;A\u{7}"));
        assert!(output.contains("\u{1b}]133;B\u{7}\u{1b}]133;C\u{7}"));
    }
}

#[test]
fn setup_ignores_raw_input_then_completes_both_steps() {
    use pi_coding_agent::modes::interactive::{components::first_time_setup::*, theme::TerminalTheme};
    init_theme();
    let submitted = Rc::new(Cell::new(None));
    let callback = submitted.clone();
    let mut setup = FirstTimeSetupComponent::new(FirstTimeSetupOptions {
        detected_theme: TerminalTheme::Dark,
        on_theme_preview: Rc::new(|_| {}),
        on_submit: Rc::new(move |value| callback.set(Some(value))),
        on_cancel: Rc::new(|| {}),
    });
    setup.handle_input(&JsString::from_utf16(vec![0xd800]));
    assert!(JsString::join(setup.render(80), "\n").contains("Pick a theme."));
    setup.handle_input(&"\n".into());
    setup.handle_input(&"\n".into());
    assert_eq!(
        submitted.get(),
        Some(FirstTimeSetupResult {
            theme: TerminalTheme::Dark,
            share_analytics: true
        })
    );
}

#[test]
fn trust_selector_keeps_raw_path_in_summary() {
    use pi_coding_agent::{
        core::trust_manager::{ProjectTrustDecision, ProjectTrustStoreEntry},
        modes::interactive::components::trust_selector::*,
    };
    init_theme();
    let raw_path = JsString::from("/tmp/q18-") + JsString::from_utf16(vec![0xd800]);
    let mut selector = TrustSelectorComponent::new(TrustSelectorOptions {
        cwd: raw_path.clone(),
        saved_decision: Some(ProjectTrustStoreEntry {
            path: raw_path.clone(),
            decision: ProjectTrustDecision::Trusted,
        }),
        project_trusted: true,
        on_select: Rc::new(|_| {}),
        on_cancel: Rc::new(|| {}),
    })
    .unwrap();
    selector.handle_input(&JsString::from_utf16(vec![0xdc00]));
    assert!(JsString::join(selector.render(120), "\n").contains(&raw_path));
}

#[test]
fn permission_selectors_ignore_raw_input_and_keep_scope_selection() {
    use pi_coding_agent::{
        core::permissions::presets::{APPROVAL_PRESETS, SANDBOX_PROFILES},
        modes::interactive::components::{approval_selector::*, sandbox_selector::*},
    };
    init_theme();
    let approved = Rc::new(Cell::new(None));
    let callback = approved.clone();
    let mut approval = ApprovalSelectorComponent::new(ApprovalSelectorOptions {
        current_preset: APPROVAL_PRESETS[0].name,
        on_select: Rc::new(move |value| callback.set(Some(value))),
        on_cancel: Rc::new(|| {}),
    });
    approval.handle_input(&JsString::from_utf16(vec![0xd800]));
    approval.handle_input(&"\t".into());
    approval.handle_input(&"\n".into());
    assert_eq!(approved.get().unwrap().scope, ApprovalScope::Project);
    assert!(JsString::join(approval.render(80), "\n").contains("Save scope: project"));
    let selected = Rc::new(Cell::new(None));
    let callback = selected.clone();
    let mut sandbox = SandboxSelectorComponent::new(SandboxSelectorOptions {
        current_profile: SANDBOX_PROFILES[0].name,
        session_profile: None,
        on_select: Rc::new(move |value| callback.set(Some(value))),
        on_cancel: Rc::new(|| {}),
    });
    sandbox.handle_input(&JsString::from_utf16(vec![0xdc00]));
    sandbox.handle_input(&"\t".into());
    sandbox.handle_input(&"\t".into());
    sandbox.handle_input(&"\n".into());
    assert_eq!(
        selected.get(),
        Some(SandboxSelectorSelection::Session { profile: None })
    );
    assert!(JsString::join(sandbox.render(80), "\n").contains("Inherit global/project"));
}

#[test]
fn compaction_expanded_summary_keeps_raw_units() {
    use pi_coding_agent::{
        core::messages::{CompactionSummaryMessage, CompactionSummaryRole},
        modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent,
    };
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    let mut message = CompactionSummaryMessageComponent::new(
        CompactionSummaryMessage {
            role: CompactionSummaryRole::CompactionSummary,
            summary: raw.clone(),
            tokens_before: 1234.0,
            tokens_after: Some(500.0),
            kept_from_previous_context_tokens: None,
            compacted_result_tokens: None,
            duration_ms: Some(1500.0),
            provider_native: None,
            timestamp: 0.0,
        },
        None,
    );
    message.set_expanded(true);
    assert!(JsString::join(message.render(80), "\n").contains(&raw));
    message.invalidate();
    assert!(JsString::join(message.render(80), "\n").contains(&raw));
    message.set_expanded(false);
    assert!(!JsString::join(message.render(80), "\n").contains(&raw));
}

#[test]
fn animation_leaf_caches_native_render_output() {
    use pi_coding_agent::modes::interactive::components::{armin::ArminComponent, daxnuts::DaxnutsComponent};
    init_theme();
    let mut armin = ArminComponent::new(Rc::new(|| {}));
    let first = armin.render(80);
    assert!(JsString::join(first.clone(), "\n").contains("ARMIN SAYS HI"));
    assert_eq!(armin.render(80), first);
    armin.invalidate();
    assert_eq!(armin.render(80), first);
    armin.dispose();
    let mut daxnuts = DaxnutsComponent::new(Rc::new(|| {}));
    let first = daxnuts.render(80);
    assert_eq!(daxnuts.render(80), first);
    daxnuts.invalidate();
    assert_eq!(daxnuts.render(80), first);
    daxnuts.dispose();
}

#[test]
fn assistant_message_keeps_raw_thinking_label_text_and_error() {
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextContent, ThinkingContent};
    use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    let mut message = AssistantMessage::empty("faux", "faux", "fixture", 0);
    message.content = vec![
        AssistantContent::Thinking(ThinkingContent::new(raw.clone())),
        AssistantContent::Text(TextContent::new(raw.clone())),
    ];
    message.stop_reason = StopReason::Error;
    message.error_message = Some(JsString::from("failed ") + &raw);
    let mut view = AssistantMessageComponent::new(Some(message), false, None, None, 1, vec![]);
    let output = JsString::join(view.render(100), "\n");
    assert!(output.contains(&raw));
    assert!(output.contains(JsString::from("Error: failed ") + &raw));
    view.set_hidden_thinking_label(JsString::from("hidden ") + &raw);
    view.set_hide_thinking_block(true);
    view.invalidate();
    assert!(JsString::join(view.render(100), "\n").contains(JsString::from("hidden ") + &raw));
}

#[test]
fn earendil_announcement_renders_and_invalidates_native_lines() {
    use pi_coding_agent::modes::interactive::{
        components::earendil_announcement::EarendilAnnouncementComponent, theme::with_theme,
    };
    init_theme();
    let mut panel = EarendilAnnouncementComponent::new(with_theme(|theme| std::sync::Arc::new(theme.clone())));
    let output = JsString::join(panel.render(100), "\n");
    assert!(output.contains("pi has joined Earendil"));
    assert!(output.contains("https://mariozechner.at/posts/2026-04-08-ive-sold-out/"));
    panel.invalidate();
    assert_eq!(JsString::join(panel.render(100), "\n"), output);
}

#[test]
fn user_message_selector_retains_raw_normalized_preview_and_id() {
    use pi_coding_agent::modes::interactive::components::user_message_selector::{UserMessageItem, UserMessageList};
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let id = JsString::from_utf16(vec![0xdc00]);
    let mut list = UserMessageList::new(
        vec![UserMessageItem {
            id: id.clone(),
            text: raw.clone() + "\nsecond",
        }],
        Some(&id),
    );
    let selected = Rc::new(std::cell::RefCell::new(None));
    let callback = selected.clone();
    list.on_select = Some(Rc::new(move |id| *callback.borrow_mut() = Some(id)));
    assert!(JsString::join(list.render(80), "\n").contains(raw + " second"));
    list.handle_input(&"\r".into());
    assert_eq!(*selected.borrow(), Some(id));
}

#[test]
fn agent_switcher_main_thread_selection_is_native() {
    use pi_coding_agent::modes::interactive::components::agent_switcher::AgentSwitcherComponent;
    init_theme();
    let selected = Rc::new(std::cell::RefCell::new(None));
    let callback = selected.clone();
    let mut switcher = AgentSwitcherComponent::new(
        vec![],
        None,
        Rc::new(move |id| *callback.borrow_mut() = Some(id)),
        Rc::new(|| {}),
        Rc::new(|_| {}),
    );
    switcher.handle_input(&JsString::from_utf16(vec![0xd800]));
    assert!(JsString::join(switcher.render(80), "\n").contains("Main thread"));
    switcher.handle_input(&"\r".into());
    assert_eq!(*selected.borrow(), Some(JsString::from("main")));
}

#[tokio::test(flavor = "current_thread")]
async fn bash_execution_keeps_raw_command_and_streamed_output() {
    use pi_coding_agent::modes::interactive::components::bash_execution::{
        BashExecutionComponent, BashExecutionOptions,
    };
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let mut view = BashExecutionComponent::new(
        raw.clone(),
        Rc::new(|| {}),
        false,
        BashExecutionOptions {
            show_elapsed: Some(false),
        },
    );
    view.append_output(JsString::from("out ") + &raw);
    view.append_output("\nnext");
    assert_eq!(view.get_output(), JsString::from("out ") + &raw + "\nnext");
    view.set_complete(Some(0), false, None, None);
    for expanded in [false, true] {
        view.set_expanded(expanded);
        assert!(JsString::join(view.render(80), "\n").contains(JsString::from("out ") + &raw));
        assert_eq!(view.get_command(), &raw);
    }
    view.dispose();
}

#[test]
fn scoped_models_filters_raw_missing_ids_and_preserves_search_input() {
    use pi_coding_agent::modes::interactive::components::scoped_models_selector::*;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let selected = Rc::new(std::cell::RefCell::new(None));
    let callback = selected.clone();
    let mut selector = ScopedModelsSelectorComponent::new(
        ModelsConfig {
            all_models: vec![],
            enabled_model_ids: Some(vec![raw.clone()]),
        },
        ModelsCallbacks {
            on_change: Rc::new(move |value| *callback.borrow_mut() = Some(value)),
            on_persist: Rc::new(|_| {}),
            on_cancel: Rc::new(|| {}),
        },
    );
    assert!(JsString::join(selector.render(80), "\n").contains("No matching models"));
    assert!(!JsString::join(selector.render(80), "\n").contains(&raw));
    selector.handle_input(&raw);
    let rendered = JsString::join(selector.render(80), "\n");
    assert!(rendered.contains(&raw), "search input lost units: {rendered:?}");
    assert!(rendered.contains("No matching models"));
    selector.handle_input(&"\r".into());
    assert_eq!(*selected.borrow(), None);
}

#[test]
fn tool_execution_keeps_raw_result_and_argument_json() {
    use pi_coding_agent::modes::interactive::components::tool_execution::*;
    use pi_js_compat::js_json::JsValue;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let mut view = ToolExecutionComponent::new(
        "fixture",
        "call",
        JsValue::String(raw.clone()),
        ToolExecutionOptions::default(),
        None,
        std::sync::Arc::new(|| {}),
        "/tmp",
    );
    view.update_result(
        ToolExecutionResult {
            content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                raw.clone(),
            ))],
            details: None,
            is_error: false,
        },
        false,
        None,
    );
    for expanded in [false, true] {
        view.set_expanded(expanded);
        view.invalidate();
        let output = JsString::join(view.render(120), "\n");
        assert!(output.contains(&raw));
        assert!(output.contains("\\ud800"));
    }
    view.dispose();
}

#[tokio::test(flavor = "current_thread")]
async fn login_dialog_keeps_raw_prompt_answer_and_display() {
    use pi_coding_agent::modes::interactive::components::login_dialog::LoginDialogComponent;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    let mut dialog = LoginDialogComponent::new(Rc::new(|| {}), "fixture", Rc::new(|_, _| {}), None, Some(&raw));
    let answer = dialog.show_prompt(&raw, Some(&raw));
    assert!(JsString::join(dialog.render(120), "\n").contains(&raw));
    dialog.handle_input(&raw);
    dialog.handle_input(&"\r".into());
    assert_eq!(answer.await.unwrap().unwrap(), raw);
}

#[test]
fn oauth_selector_native_query_matches_raw_method_name() {
    use pi_coding_agent::modes::interactive::components::oauth_selector::*;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let mut provider = AuthSelectorProvider::new("fixture", "Fixture", pi_ai::auth::types::AuthType::ApiKey);
    provider.method_name = Some(raw.clone());
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider],
        Rc::new(|_, _| {}),
        Rc::new(|| {}),
        None,
    );
    selector.handle_input(&raw);
    assert!(!JsString::join(selector.render(120), "\n").contains("No matching providers"));
}

#[test]
fn config_values_preserve_raw_literals_and_native_environment_values() {
    use pi_coding_agent::core::{get_config_value_env_var_names, resolve_config_value};
    let raw = JsString::from_utf16(vec![0xd800]);
    let env = std::collections::HashMap::from([("SECRET".to_string(), raw.clone())]);
    assert_eq!(
        resolve_config_value(&(JsString::from("before ") + &raw + " $SECRET"), Some(&env)),
        Some(JsString::from("before ") + &raw + " " + &raw)
    );
    let invalid = JsString::from("${") + &raw + "}";
    assert_eq!(resolve_config_value(&invalid, Some(&env)), Some(invalid.clone()));
    assert!(get_config_value_env_var_names(&invalid).is_empty());
}

#[tokio::test]
async fn auth_storage_reads_and_writes_raw_credentials_without_scalar_projection() {
    use pi_ai::auth::types::{ApiKeyCredential, AuthOperationOptions, Credential, CredentialStore};
    use pi_coding_agent::core::auth_storage::AuthStorage;
    let store = AuthStorage::in_memory(Vec::<(JsString, pi_js_compat::js_json::JsValue)>::new());
    let raw = JsString::from_utf16(vec![0xd800]);
    let value = raw.clone();
    store
        .modify(
            "fixture",
            std::sync::Arc::new(move |_| {
                let raw = value.clone();
                Box::pin(async move {
                    Ok(Some(Credential::ApiKey(ApiKeyCredential {
                        key: Some(raw),
                        env: None,
                    })))
                })
            }),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let read = store
        .read("fixture", AuthOperationOptions::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.as_api_key().unwrap().key.as_ref(), Some(&raw));
    assert_eq!(
        store.get_api_key("fixture", Default::default()).await.unwrap(),
        Some(raw)
    );
}

#[test]
fn model_config_keeps_raw_provider_credentials_and_url() {
    use pi_coding_agent::core::{model_config::ModelConfig, provider_composer::ModelsJsonProvider};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models.json");
    std::fs::write(&path, r#"{"providers":{"fixture":{"apiKey":"secret\ud800","baseUrl":"https://example.test/\udc00","headers":{"X-Key":"\ud800"}}}}"#).unwrap();
    let config = ModelConfig::load(Some(&path.to_str().unwrap().into()));
    assert!(config.get_error().is_none());
    let provider: ModelsJsonProvider =
        pi_js_compat::js_json::from_value(config.get_provider("fixture").unwrap()).unwrap();
    assert_eq!(
        provider.api_key.unwrap(),
        JsString::from("secret") + JsString::from_utf16(vec![0xd800])
    );
    assert!(provider.base_url.unwrap().contains(JsString::from_utf16(vec![0xdc00])));
    assert_eq!(provider.headers.unwrap()["X-Key"], JsString::from_utf16(vec![0xd800]));
}

#[test]
fn auth_command_extracts_native_api_key_and_bearer_units() {
    use pi_ai::auth::types::{AuthResult, ModelAuth};
    use pi_coding_agent::cli::get_auth_credential;
    let raw = JsString::from_utf16(vec![0xd800]);
    let mut auth = AuthResult {
        auth: ModelAuth {
            api_key: Some(raw.clone()),
            ..Default::default()
        },
        env: None,
        source: None,
    };
    assert_eq!(get_auth_credential(Some(&auth)), Some(raw.clone()));
    auth.auth.api_key = None;
    auth.auth.headers = Some(pi_ai::types::ProviderHeaders::from([(
        "Authorization".to_string(),
        Some(JsString::from("Bearer ") + &raw),
    )]));
    assert_eq!(get_auth_credential(Some(&auth)), Some(raw));
}
