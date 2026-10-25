use crate::shared_oracle as oracle;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::Transport;
use pi_coding_agent::{
    core::{
        keybindings::KeybindingsManager,
        settings_manager::{DefaultProjectTrust, DoubleEscapeAction, Settings, TreeFilterMode},
    },
    modes::interactive::{
        components::settings_selector::{SettingsCallbacks, SettingsConfig, SettingsSelectorComponent, SteeringMode},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, TerminalTheme, Theme, set_theme_instance},
    },
    utils::uncaught::{catch_panics, format_uncaught, install_uncaught_handler, throw_uncaught},
};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};
use pi_tui::Component;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn raw(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
}
fn colors(offset: usize) -> Value {
    let mut colors = serde_json::Map::new();
    for (index, name) in REQUIRED_COLORS.iter().enumerate() {
        colors.insert(
            (*name).into(),
            json!(format!(
                "#{:02x}{:02x}{:02x}",
                (16 + 7 * index + offset) % 256,
                (32 + 11 * index + offset) % 256,
                (48 + 13 * index + offset) % 256
            )),
        );
    }
    Value::Object(colors)
}
fn set_colors(value: &Value) {
    let colors: Vec<_> = value
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, color)| (name.as_str().into(), ColorValue::Text(color.as_str().unwrap().into())))
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap(),
    ));
}
#[derive(Clone)]
struct Events {
    values: Rc<RefCell<Vec<Value>>>,
    fail: String,
    preview_colors: Option<Value>,
}
impl Events {
    fn emit(&self, name: &str, value: Option<JsValue>) {
        self.values
            .borrow_mut()
            .push(json!([name, value.as_ref().map(stringify_lossless)]));
        if name == "onThemePreview"
            && let Some(colors) = &self.preview_colors
        {
            set_colors(colors);
        }
        if name == self.fail {
            throw_uncaught("Error", format!("callback {name}"));
        }
    }
}
fn owned<T: 'static>(events: &Events, name: &'static str, convert: impl Fn(T) -> JsValue + 'static) -> Rc<dyn Fn(T)> {
    let events = events.clone();
    Rc::new(move |value| events.emit(name, Some(convert(value))))
}
fn text(events: &Events, name: &'static str) -> Rc<dyn Fn(&JsString)> {
    let events = events.clone();
    Rc::new(move |value| events.emit(name, Some(JsValue::String(value.clone()))))
}
fn warning_value(value: Settings) -> JsValue {
    JsValue::Object(value)
}
fn steering(value: SteeringMode) -> JsValue {
    JsValue::String(
        match value {
            SteeringMode::All => "all",
            SteeringMode::OneAtATime => "one-at-a-time",
        }
        .into(),
    )
}
fn enum_value<T: serde::Serialize>(value: T) -> JsValue {
    serde_json::to_value(value).unwrap().into()
}
fn double_escape(value: DoubleEscapeAction) -> JsValue {
    JsValue::String(
        match value {
            DoubleEscapeAction::Fork => "fork",
            DoubleEscapeAction::Tree => "tree",
            DoubleEscapeAction::None => "none",
        }
        .into(),
    )
}
fn tree_filter(value: TreeFilterMode) -> JsValue {
    JsValue::String(
        match value {
            TreeFilterMode::Default => "default",
            TreeFilterMode::NoTools => "no-tools",
            TreeFilterMode::UserOnly => "user-only",
            TreeFilterMode::LabeledOnly => "labeled-only",
            TreeFilterMode::All => "all",
        }
        .into(),
    )
}
fn callbacks(events: &Events, preview: bool) -> Rc<SettingsCallbacks> {
    let cancel = events.clone();
    Rc::new(SettingsCallbacks {
        on_auto_compact_change: owned(events, "onAutoCompactChange", JsValue::Bool),
        on_show_images_change: owned(events, "onShowImagesChange", JsValue::Bool),
        on_image_width_cells_change: owned(events, "onImageWidthCellsChange", JsValue::Number),
        on_auto_resize_images_change: owned(events, "onAutoResizeImagesChange", JsValue::Bool),
        on_block_images_change: owned(events, "onBlockImagesChange", JsValue::Bool),
        on_enable_skill_commands_change: owned(events, "onEnableSkillCommandsChange", JsValue::Bool),
        on_steering_mode_change: owned(events, "onSteeringModeChange", steering),
        on_follow_up_mode_change: owned(events, "onFollowUpModeChange", steering),
        on_transport_change: owned(events, "onTransportChange", enum_value::<Transport>),
        on_http_idle_timeout_ms_change: owned(events, "onHttpIdleTimeoutMsChange", JsValue::Number),
        on_thinking_level_change: owned(events, "onThinkingLevelChange", enum_value::<ThinkingLevel>),
        on_theme_change: text(events, "onThemeChange"),
        on_theme_preview: preview.then(|| text(events, "onThemePreview")),
        on_hide_thinking_block_change: owned(events, "onHideThinkingBlockChange", JsValue::Bool),
        on_hide_tool_output_change: owned(events, "onHideToolOutputChange", JsValue::Bool),
        on_hide_coordination_messages_change: owned(events, "onHideCoordinationMessagesChange", JsValue::Bool),
        on_collapse_changelog_change: owned(events, "onCollapseChangelogChange", JsValue::Bool),
        on_enable_install_telemetry_change: owned(events, "onEnableInstallTelemetryChange", JsValue::Bool),
        on_double_escape_action_change: owned(events, "onDoubleEscapeActionChange", double_escape),
        on_tree_filter_mode_change: owned(events, "onTreeFilterModeChange", tree_filter),
        on_show_hardware_cursor_change: owned(events, "onShowHardwareCursorChange", JsValue::Bool),
        on_editor_padding_x_change: owned(events, "onEditorPaddingXChange", JsValue::Number),
        on_output_pad_change: owned(events, "onOutputPadChange", |value| JsValue::Number(value as f64)),
        on_autocomplete_max_visible_change: owned(events, "onAutocompleteMaxVisibleChange", |value| {
            JsValue::Number(value)
        }),
        on_quiet_startup_change: owned(events, "onQuietStartupChange", JsValue::Bool),
        on_default_project_trust_change: owned(
            events,
            "onDefaultProjectTrustChange",
            enum_value::<DefaultProjectTrust>,
        ),
        on_clear_on_shrink_change: owned(events, "onClearOnShrinkChange", JsValue::Bool),
        on_show_terminal_progress_change: owned(events, "onShowTerminalProgressChange", JsValue::Bool),
        on_warnings_change: owned(events, "onWarningsChange", warning_value),
        on_cancel: Rc::new(move || cancel.emit("onCancel", None)),
    })
}
fn native_config(value: &Value) -> SettingsConfig {
    let warnings = parse_lossless(value["warningsJson"].as_str().unwrap()).unwrap();
    SettingsConfig {
        auto_compact: value["autoCompact"].as_bool().unwrap(),
        show_images: value["showImages"].as_bool().unwrap(),
        image_width_cells: number(&value["imageWidthCells"]),
        auto_resize_images: value["autoResizeImages"].as_bool().unwrap(),
        block_images: value["blockImages"].as_bool().unwrap(),
        enable_skill_commands: value["enableSkillCommands"].as_bool().unwrap(),
        steering_mode: SteeringMode::OneAtATime,
        follow_up_mode: SteeringMode::All,
        transport: Transport::Sse,
        http_idle_timeout_ms: number(&value["httpIdleTimeoutMs"]),
        thinking_level: ThinkingLevel::Low,
        available_thinking_levels: value["availableThinkingLevels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|level| serde_json::from_value(level.clone()).unwrap())
            .collect(),
        current_theme: raw(&value["currentTheme"]),
        terminal_theme: if value["terminalTheme"] == "light" {
            TerminalTheme::Light
        } else {
            TerminalTheme::Dark
        },
        available_themes: value["availableThemes"].as_array().unwrap().iter().map(raw).collect(),
        hide_thinking_block: value["hideThinkingBlock"].as_bool().unwrap(),
        hide_tool_output: value["hideToolOutput"].as_bool().unwrap(),
        hide_coordination_messages: value["hideCoordinationMessages"].as_bool().unwrap(),
        collapse_changelog: value["collapseChangelog"].as_bool().unwrap(),
        enable_install_telemetry: value["enableInstallTelemetry"].as_bool().unwrap(),
        double_escape_action: DoubleEscapeAction::Fork,
        tree_filter_mode: TreeFilterMode::Default,
        show_hardware_cursor: value["showHardwareCursor"].as_bool().unwrap(),
        editor_padding_x: number(&value["editorPaddingX"]),
        output_pad: value["outputPad"].as_u64().unwrap() as u8,
        autocomplete_max_visible: number(&value["autocompleteMaxVisible"]),
        quiet_startup: value["quietStartup"].as_bool().unwrap(),
        default_project_trust: DefaultProjectTrust::Ask,
        clear_on_shrink: value["clearOnShrink"].as_bool().unwrap(),
        show_terminal_progress: value["showTerminalProgress"].as_bool().unwrap(),
        warnings: warnings.as_object().unwrap().clone(),
    }
}
fn native(request: &Value) -> Value {
    KeybindingsManager::new(Vec::new(), None).install();
    set_colors(&request["colors"]);
    pi_tui::set_capabilities(pi_tui::TerminalCapabilities {
        images: request["images"]
            .as_bool()
            .unwrap()
            .then_some(pi_tui::ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: false,
    });
    let events = Events {
        values: Rc::new(RefCell::new(Vec::new())),
        fail: request["fail"].as_str().unwrap().into(),
        preview_colors: request["changeThemeOnPreview"]
            .as_bool()
            .unwrap()
            .then(|| request["previewColors"].clone()),
    };
    let mut component = SettingsSelectorComponent::new(
        native_config(&request["config"]),
        callbacks(&events, request["preview"].as_bool().unwrap()),
    );
    let width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |component: &mut SettingsSelectorComponent, error: Option<JsString>| {
        let lines: Vec<_> = component
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect();
        let list: Vec<_> = component
            .get_settings_list_mut()
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect();
        let mut value = json!({"lines":lines,"list":list,"events":events.values.borrow().clone()});
        if request["children"].as_bool().unwrap_or(false) {
            value["children"] = json!(component.as_container().map_or_else(Vec::new, |container| {
                container
                    .children()
                    .iter()
                    .flat_map(|child| child.borrow_mut().render(width))
                    .map(|line| line.units().to_vec())
                    .collect::<Vec<_>>()
            }));
        }
        if let Some(error) = error {
            value["error"] = json!(error.to_utf8().unwrap());
        }
        value
    };
    let mut output = vec![snapshot(&mut component, None)];
    for step in request["steps"].as_array().unwrap() {
        let result = catch_panics(|| match step["op"].as_str().unwrap() {
            "key" => component.get_settings_list_mut().handle_input(&raw(&step["units"])),
            "invalidate" => component.invalidate(),
            op => panic!("unknown step {op}"),
        });
        output.push(snapshot(
            &mut component,
            result.err().map(|error| format_uncaught(error.as_ref())),
        ));
    }
    json!(output)
}
fn key(value: &str) -> Value {
    json!({"op":"key","units":units(value)})
}
fn request(steps: Vec<Value>) -> Value {
    json!({
        "config":{"autoCompact":true,"showImages":true,"imageWidthCells":80,"autoResizeImages":true,"blockImages":false,"enableSkillCommands":true,"steeringMode":"one-at-a-time","followUpMode":"all","transport":"sse","httpIdleTimeoutMs":300000,
            "thinkingLevel":"low","availableThinkingLevels":["off","minimal","low","medium","high","xhigh","max","ultra"],"currentTheme":units("dark"),"terminalTheme":"dark","availableThemes":[units("dark"),units("light"),units("fancy")],
            "hideThinkingBlock":false,"hideToolOutput":false,"hideCoordinationMessages":false,"collapseChangelog":false,"enableInstallTelemetry":true,"doubleEscapeAction":"fork","treeFilterMode":"default","showHardwareCursor":false,"editorPaddingX":0,"outputPad":0,"autocompleteMaxVisible":5,"quietStartup":false,"defaultProjectTrust":"ask","clearOnShrink":false,"showTerminalProgress":true,"warningsJson":"{}"},
        "images":true,"colors":colors(0),"previewColors":colors(1),"preview":true,"changeThemeOnPreview":false,"fail":"","width":120,"children":false,"steps":steps
    })
}
fn enter(label: &str) -> Vec<Value> {
    vec![key(&label.replace(' ', "")), key("\r")]
}
#[test]
fn settings_state_callbacks_and_submenu_sequences_match_frozen() {
    install_uncaught_handler();
    let mut cases = Vec::<(&str, Value)>::new();
    for images in [false, true] {
        let mut case = request(vec![key("\u{1b}[B"), key("\u{1b}[A"), key("\u{1b}")]);
        case["images"] = json!(images);
        cases.push(("constructor-navigation", case));
    }
    for label in [
        "Auto-compact",
        "Show images",
        "Image width",
        "Auto-resize images",
        "Block images",
        "Skill commands",
        "Steering mode",
        "Follow-up mode",
        "Transport",
        "HTTP idle timeout",
        "Hide thinking",
        "Hide tool output",
        "Hide coordination messages",
        "Collapse changelog",
        "Quiet startup",
        "Install telemetry",
        "Default project trust",
        "Double-escape action",
        "Tree filter mode",
        "Show hardware cursor",
        "Editor padding",
        "Output padding",
        "Autocomplete max items",
        "Clear on shrink",
        "Terminal progress",
    ] {
        let mut steps = enter(label);
        steps.extend([key("\r"), key("\u{1b}")]);
        cases.push(("setting-toggle", request(steps)));
    }
    let mut warnings = request({
        let mut steps = enter("Warnings");
        steps.extend([key("\r"), key("\u{1b}"), key("\r"), key("\r"), key("\u{1b}")]);
        steps
    });
    warnings["config"]["warningsJson"] =
        json!(r#"{"9":"nine","anthropicExtraUsage":false,"unknown":{"text":"\ud800","n":[1,2]}}"#);
    cases.push(("warning-unknown-raw-state", warnings));
    for value in [json!(1.5), json!(1e30), json!("NaN"), json!("-0")] {
        for (field, label) in [
            ("imageWidthCells", "Image width"),
            ("editorPaddingX", "Editor padding"),
            ("autocompleteMaxVisible", "Autocomplete max items"),
            ("httpIdleTimeoutMs", "HTTP idle timeout"),
        ] {
            let mut case = request(enter(label));
            case["config"][field] = value.clone();
            cases.push(("numeric-config", case));
        }
    }
    let mut thinking = request({
        let mut steps = enter("Thinking level");
        steps.extend([key("\u{1b}[B"), key("\r")]);
        steps
    });
    thinking["config"]["thinkingLevel"] = json!("low");
    cases.push(("thinking-submenu", thinking));
    let mut preview = request({
        let mut steps = enter("Theme");
        steps.extend([key("\u{1b}[B"), json!({"op":"invalidate"}), key("\u{1b}")]);
        steps
    });
    preview["changeThemeOnPreview"] = json!(true);
    cases.push(("theme-preview-colors-rollback", preview));
    let mut auto = request({
        let mut steps = enter("Theme");
        steps.extend([
            key("\u{1b}[A"),
            key("\r"),
            key("\u{1b}[B"),
            key("\r"),
            key("\u{1b}[B"),
            key("\r"),
            key("\u{1b}[B"),
            key("\r"),
        ]);
        steps
    });
    auto["changeThemeOnPreview"] = json!(true);
    cases.push(("theme-automatic-apply", auto));
    for terminal in ["light", "dark"] {
        let mut automatic = request({
            let mut steps = enter("Theme");
            steps.extend([
                key("\u{1b}[B"),
                key("\u{1b}[B"),
                key("\u{1b}[B"),
                key("\r"),
                key("\u{1b}[A"),
                key("\r"),
            ]);
            steps
        });
        automatic["config"]["currentTheme"] = json!(units("light/dark"));
        automatic["config"]["terminalTheme"] = json!(terminal);
        cases.push(("automatic-to-single", automatic));
    }
    let mut raw_themes = request({
        let mut steps = enter("Theme");
        steps.extend([key("\u{1b}[B"), key("\r")]);
        steps
    });
    raw_themes["config"]["currentTheme"] = json!([55296]);
    raw_themes["config"]["availableThemes"] = json!([[100, 97, 114, 107], [55296], [56320]]);
    raw_themes["preview"] = json!(false);
    cases.push(("raw-theme-and-no-preview", raw_themes));
    let mut inherited = request(vec![
        json!({"op":"invalidate"}),
        json!({"op":"key","units":units("Hide tool output")}),
        key("\r"),
    ]);
    inherited["children"] = json!(true);
    cases.push(("inherited-children-live-getter", inherited));
    let mut failure = request(enter("Auto-compact"));
    failure["fail"] = json!("onAutoCompactChange");
    cases.push(("callback-error", failure));
    let mut oracle = oracle::TsOracle::spawn("settings-selector-oracle.mts", "packages/coding-agent/src");
    let mut mismatches = Vec::new();

    for (name, request) in cases {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "source {name}: {expected}");
        let actual = native(&request);

        if actual != expected["output"] {
            let index = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .position(|(actual, expected)| actual != expected)
                .unwrap();
            let fields: Vec<_> = ["lines", "list", "events", "children", "error"]
                .into_iter()
                .filter(|field| actual[index][field] != expected["output"][index][field])
                .collect();
            eprintln!(
                "{name}: first mismatch step {index}, fields {fields:?}; source events {} native events {}",
                expected["output"][index]["events"], actual[index]["events"]
            );
            if name == "constructor-navigation" {
                for (line, (actual, expected)) in actual[index]["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(expected["output"][index]["lines"].as_array().unwrap())
                    .enumerate()
                {
                    if actual != expected {
                        eprintln!(
                            "first line {line}: source {} / native {}",
                            stringify_lossless(&JsValue::String(raw(expected))),
                            stringify_lossless(&JsValue::String(raw(actual)))
                        );
                        break;
                    }
                }
            }
            if let Some(event_index) = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .position(|(actual, expected)| actual["events"] != expected["events"])
            {
                eprintln!(
                    "event mismatch at {event_index}: source {} / native {}",
                    expected["output"][event_index]["events"], actual[event_index]["events"]
                );
            }
            mismatches.push(name);
        }
    }

    assert!(mismatches.is_empty(), "settings mismatches: {mismatches:?}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
