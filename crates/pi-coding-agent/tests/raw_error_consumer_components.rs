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
