//! Behavioral tests for `modes/interactive/components/settings-selector.ts`.

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::Transport;
use pi_coding_agent::core::settings_manager::{DefaultProjectTrust, DoubleEscapeAction, Settings, TreeFilterMode};
use pi_coding_agent::modes::interactive::components::settings_selector::{
    SettingsCallbacks, SettingsConfig, SettingsSelectorComponent, SteeringMode,
};
use pi_coding_agent::modes::interactive::theme::{
    ColorMode, ColorValue, REQUIRED_COLORS, TerminalTheme, Theme, set_theme_instance,
};
use pi_tui::utils::strip_terminal_sequences;
use pi_tui::{Component, KeybindingsManager, set_keybindings};

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(JsString, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                ))),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

#[derive(Default)]
struct Recorder {
    themes: RefCell<Vec<JsString>>,
    previews: RefCell<Vec<JsString>>,
    warnings: RefCell<Vec<Settings>>,
    thinking: RefCell<Vec<ThinkingLevel>>,
}

fn noop<T: 'static>(_: T) -> Rc<dyn Fn()> {
    Rc::new(|| {})
}

fn callbacks(recorder: Rc<Recorder>) -> Rc<SettingsCallbacks> {
    let theme_recorder = recorder.clone();
    let preview_recorder = recorder.clone();
    let warnings_recorder = recorder.clone();
    let thinking_recorder = recorder;
    Rc::new(SettingsCallbacks {
        on_auto_compact_change: Rc::new(|_| {}),
        on_show_images_change: Rc::new(|_| {}),
        on_image_width_cells_change: Rc::new(|_| {}),
        on_auto_resize_images_change: Rc::new(|_| {}),
        on_block_images_change: Rc::new(|_| {}),
        on_enable_skill_commands_change: Rc::new(|_| {}),
        on_steering_mode_change: Rc::new(|_| {}),
        on_follow_up_mode_change: Rc::new(|_| {}),
        on_transport_change: Rc::new(|_| {}),
        on_http_idle_timeout_ms_change: Rc::new(|_| {}),
        on_thinking_level_change: Rc::new(move |level| thinking_recorder.thinking.borrow_mut().push(level)),
        on_theme_change: Rc::new(move |theme| theme_recorder.themes.borrow_mut().push(theme.clone())),
        on_theme_preview: Some(Rc::new(move |theme| {
            preview_recorder.previews.borrow_mut().push(theme.clone());
        })),
        on_hide_thinking_block_change: Rc::new(|_| {}),
        on_hide_tool_output_change: Rc::new(|_| {}),
        on_hide_coordination_messages_change: Rc::new(|_| {}),
        on_collapse_changelog_change: Rc::new(|_| {}),
        on_enable_install_telemetry_change: Rc::new(|_| {}),
        on_double_escape_action_change: Rc::new(|_| {}),
        on_tree_filter_mode_change: Rc::new(|_| {}),
        on_show_hardware_cursor_change: Rc::new(|_| {}),
        on_editor_padding_x_change: Rc::new(|_| {}),
        on_output_pad_change: Rc::new(|_| {}),
        on_autocomplete_max_visible_change: Rc::new(|_| {}),
        on_quiet_startup_change: Rc::new(|_| {}),
        on_default_project_trust_change: Rc::new(|_| {}),
        on_clear_on_shrink_change: Rc::new(|_| {}),
        on_show_terminal_progress_change: Rc::new(|_| {}),
        on_warnings_change: Rc::new(move |warnings| warnings_recorder.warnings.borrow_mut().push(warnings)),
        on_cancel: noop(()),
    })
}

fn config() -> SettingsConfig {
    SettingsConfig {
        auto_compact: true,
        show_images: true,
        image_width_cells: 80.0,
        auto_resize_images: true,
        block_images: false,
        enable_skill_commands: true,
        steering_mode: SteeringMode::OneAtATime,
        follow_up_mode: SteeringMode::All,
        transport: Transport::Sse,
        http_idle_timeout_ms: 300_000.0,
        thinking_level: ThinkingLevel::Low,
        available_thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Low, ThinkingLevel::High],
        current_theme: JsString::from("dark"),
        terminal_theme: TerminalTheme::Dark,
        available_themes: vec![JsString::from("dark"), JsString::from("light")],
        hide_thinking_block: false,
        hide_tool_output: false,
        hide_coordination_messages: false,
        collapse_changelog: false,
        enable_install_telemetry: true,
        double_escape_action: DoubleEscapeAction::Fork,
        tree_filter_mode: TreeFilterMode::Default,
        show_hardware_cursor: false,
        editor_padding_x: 0.0,
        output_pad: 0,
        autocomplete_max_visible: 5.0,
        quiet_startup: false,
        default_project_trust: DefaultProjectTrust::Ask,
        clear_on_shrink: false,
        show_terminal_progress: true,
        warnings: Settings::new(),
    }
}

fn selector(config: SettingsConfig, recorder: Rc<Recorder>) -> SettingsSelectorComponent {
    set_keybindings(KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    install_theme();
    SettingsSelectorComponent::new(config, callbacks(recorder))
}

fn type_label(component: &mut SettingsSelectorComponent, label: &str) {
    // Frozen SettingsList reserves a single Space for activation.
    component.handle_input(&JsString::from(&label.replace(' ', "")));
}

fn plain(component: &mut SettingsSelectorComponent) -> JsString {
    JsString::join(
        component
            .render(120)
            .iter()
            .map(strip_terminal_sequences)
            .collect::<Vec<_>>(),
        "\n",
    )
}

#[test]
fn removed_mermaid_and_cache_notice_options_are_not_searchable() {
    for label in ["Mermaid diagrams", "Cache miss notices"] {
        let mut component = selector(config(), Rc::new(Recorder::default()));
        type_label(&mut component, label);
        let output = plain(&mut component);
        assert!(
            output.contains("No matching settings"),
            "removed option still visible: {output:?}"
        );
    }
}

#[test]
fn renders_the_settings_rows_in_source_order() {
    pi_tui::set_capabilities(pi_tui::TerminalCapabilities {
        images: Some(pi_tui::ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    });
    let recorder = Rc::new(Recorder::default());
    let mut component = selector(config(), recorder);
    let output = plain(&mut component);
    let first_window = [
        "Auto-compact",
        "Show images",
        "Image width",
        "Auto-resize images",
        "Block images",
        "Skill commands",
        "Show hardware cursor",
        "Editor padding",
        "Output padding",
        "Autocomplete max items",
    ];
    let mut position = 0;
    for label in first_window {
        let found = output
            .slice(position, output.len())
            .find(label)
            .unwrap_or_else(|| panic!("{label} missing in {output:?}"));
        position += found + label.len();
    }

    for label in [
        "Steering mode",
        "Follow-up mode",
        "Transport",
        "HTTP idle timeout",
        "Hide thinking",
        "Collapse changelog",
        "Quiet startup",
        "Install telemetry",
        "Default project trust",
        "Double-escape action",
        "Tree filter mode",
        "Warnings",
        "Thinking level",
        "Theme",
        "Clear on shrink",
        "Terminal progress",
    ] {
        let recorder = Rc::new(Recorder::default());
        let mut component = selector(config(), recorder);
        type_label(&mut component, label);
        let output = plain(&mut component);
        assert!(output.contains(label), "{label:?} missing in {output:?}");
    }
}

#[test]
fn theme_submenu_previews_and_switches_to_automatic_mode() {
    let recorder = Rc::new(Recorder::default());
    let mut component = selector(config(), recorder.clone());

    type_label(&mut component, "Theme");
    component.handle_input(&JsString::from("\r"));
    let output = plain(&mut component);
    assert!(output.contains("Select a theme, or choose Automatic"), "{output:?}");

    component.handle_input(&JsString::from("\u{1b}[A"));
    component.handle_input(&JsString::from("\r"));
    let output = plain(&mut component);
    assert!(output.contains("Automatic Theme"), "{output:?}");
    assert!(
        recorder.previews.borrow().iter().any(|preview| preview == "dark/dark"),
        "{:?}",
        recorder.previews.borrow()
    );

    component.handle_input(&JsString::from("\u{1b}[B"));
    component.handle_input(&JsString::from("\u{1b}[B"));
    component.handle_input(&JsString::from("\r"));
    assert_eq!(*recorder.themes.borrow(), vec!["dark/dark".to_string()]);
}

#[test]
fn theme_submenu_selects_a_single_theme() {
    let recorder = Rc::new(Recorder::default());
    let mut component = selector(config(), recorder.clone());

    type_label(&mut component, "Theme");
    component.handle_input(&JsString::from("\r"));
    component.handle_input(&JsString::from("\u{1b}[B"));
    component.handle_input(&JsString::from("\r"));
    assert_eq!(*recorder.themes.borrow(), vec!["light".to_string()]);
}

#[test]
fn warnings_submenu_updates_the_warning_settings() {
    let recorder = Rc::new(Recorder::default());
    let mut component = selector(config(), recorder.clone());

    type_label(&mut component, "Warnings");
    component.handle_input(&JsString::from("\r"));
    component.handle_input(&JsString::from("\r"));
    assert_eq!(recorder.warnings.borrow().len(), 1);
    assert_eq!(
        recorder.warnings.borrow()[0]
            .get("anthropicExtraUsage")
            .and_then(pi_js_compat::js_json::JsValue::as_bool),
        Some(false)
    );
}

#[test]
fn thinking_submenu_selects_a_level() {
    let recorder = Rc::new(Recorder::default());
    let mut component = selector(config(), recorder.clone());

    type_label(&mut component, "Thinking level");
    component.handle_input(&JsString::from("\r"));
    component.handle_input(&JsString::from("\u{1b}[B"));
    component.handle_input(&JsString::from("\r"));
    assert_eq!(*recorder.thinking.borrow(), vec![ThinkingLevel::High]);
}
