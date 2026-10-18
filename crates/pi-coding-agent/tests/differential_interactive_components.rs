#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_coding_agent::core::extensions::types::{MarkdownMessageType, MarkdownTransformer};
use pi_coding_agent::core::permissions::presets::{ApprovalPresetName, SandboxProfileName, SandboxProfileScope};
use pi_coding_agent::core::trust_manager::{ProjectTrustDecision, ProjectTrustStoreEntry};
use pi_coding_agent::modes::interactive::components::approval_selector::{
    ApprovalSelectorComponent, ApprovalSelectorOptions, ApprovalSelectorScope,
};
use pi_coding_agent::modes::interactive::components::elapsed_time::format_elapsed_duration;
use pi_coding_agent::modes::interactive::components::extension_selector::{
    ExtensionSelectorComponent, ExtensionSelectorOptions,
};
use pi_coding_agent::modes::interactive::components::keybinding_hints::{
    KeyTextFormatOptions, format_key_text, key_display_text, key_hint, key_text, raw_key_hint,
};
use pi_coding_agent::modes::interactive::components::markdown_transform::create_markdown_transform;
use pi_coding_agent::modes::interactive::components::sandbox_selector::{
    SandboxSelectorComponent, SandboxSelectorOptions, SandboxSelectorSelection,
};
use pi_coding_agent::modes::interactive::components::show_images_selector::ShowImagesSelectorComponent;
use pi_coding_agent::modes::interactive::components::theme_selector::ThemeSelectorComponent;
use pi_coding_agent::modes::interactive::components::thinking_selector::ThinkingSelectorComponent;
use pi_coding_agent::modes::interactive::components::trust_selector::{TrustSelectorComponent, TrustSelectorOptions};
use pi_coding_agent::modes::interactive::components::user_message_selector::{
    UserMessageItem, UserMessageSelectorComponent,
};
use pi_coding_agent::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use pi_coding_agent::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use pi_coding_agent::modes::interactive::theme_api::{ThemeApi, ThemeBg, ThemeColor};
use pi_tui::Component;
use serde_json::{Value, json};

struct StubTheme;

impl ThemeApi for StubTheme {
    fn fg(&self, color: ThemeColor, text: &str) -> String {
        format!("<fg:{}>{text}</>", color_name(color))
    }

    fn bg(&self, color: ThemeBg, text: &str) -> String {
        format!("<bg:{}>{text}</>", bg_name(color))
    }

    fn bold(&self, text: &str) -> String {
        format!("<b>{text}</b>")
    }

    fn italic(&self, text: &str) -> String {
        format!("<i>{text}</i>")
    }
}

fn color_name(color: ThemeColor) -> &'static str {
    match color {
        ThemeColor::Accent => "accent",
        ThemeColor::Border => "border",
        ThemeColor::BorderAccent => "borderAccent",
        ThemeColor::CustomMessageLabel => "customMessageLabel",
        ThemeColor::CustomMessageText => "customMessageText",
        ThemeColor::Dim => "dim",
        ThemeColor::Error => "error",
        ThemeColor::MdLink => "mdLink",
        ThemeColor::Muted => "muted",
        ThemeColor::Success => "success",
        ThemeColor::Text => "text",
        ThemeColor::Warning => "warning",
    }
}

fn bg_name(color: ThemeBg) -> &'static str {
    match color {
        ThemeBg::CustomMessageBg => "customMessageBg",
        ThemeBg::SelectedBg => "selectedBg",
    }
}

fn theme() -> Rc<dyn ThemeApi> {
    Rc::new(StubTheme)
}

type Events = Rc<RefCell<Vec<Value>>>;

fn record(events: &Events, value: Value) {
    events.borrow_mut().push(value);
}

fn events_json(events: &Events) -> Value {
    json!(events.borrow().clone())
}

#[test]
fn interactive_components_match_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let themes_response = oracle.request(&json!({
        "id": 0,
        "module": "modes/interactive/theme/theme.ts",
        "operation": "availableThemes",
    }));
    let themes: Vec<String> = serde_json::from_value(themes_response["output"].clone()).expect("theme names");
    for (request, actual) in requests(&themes) {
        let expected = oracle.request(&request);
        assert_eq!(actual, expected["output"], "mismatch for {request}");
    }
}

fn requests(themes: &[String]) -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    cases.extend(elapsed_cases());
    cases.extend(visual_truncate_cases());
    cases.extend(markdown_transform_cases());
    cases.extend(key_hint_cases());
    cases.extend(model_search_cases());
    cases.extend(approval_cases());
    cases.extend(sandbox_cases());
    cases.extend(trust_cases());
    cases.extend(show_images_cases());
    cases.extend(thinking_cases());
    cases.extend(theme_cases(themes));
    cases.extend(user_message_cases());
    cases.extend(extension_cases());
    cases
}

fn elapsed_cases() -> Vec<(Value, Value)> {
    let values = [
        0.0,
        1.0,
        999.0,
        1000.0,
        1500.0,
        59_999.0,
        60_000.0,
        61_000.0,
        3_599_999.0,
        3_600_000.0,
        3_661_000.0,
        -5.0,
        123_456.789,
    ];
    let formatted: Vec<String> = values.iter().map(|value| format_elapsed_duration(*value)).collect();
    vec![(
        json!({
            "id": 1,
            "module": "modes/interactive/components/elapsed-time.ts",
            "operation": "elapsed",
            "values": values,
        }),
        json!(formatted),
    )]
}

fn visual_truncate_cases() -> Vec<(Value, Value)> {
    let cases = [
        ("", 2usize, 20usize, 0usize),
        ("one line", 2, 20, 0),
        ("line one\nline two\nline three", 2, 40, 0),
        ("line one\nline two\nline three", 0, 40, 0),
        ("line one\nline two\nline three", 5, 40, 0),
        ("a very long line that wraps across several visual rows", 2, 12, 1),
        ("a very long line that wraps across several visual rows", 1, 12, 0),
    ];
    let request_cases: Vec<Value> = cases
        .iter()
        .map(|(text, max, width, padding)| {
            json!({"text": text, "maxVisualLines": max, "width": width, "paddingX": padding})
        })
        .collect();
    let output: Vec<Value> = cases
        .iter()
        .map(|(text, max, width, padding)| {
            let result = truncate_to_visual_lines(text, *max, *width, *padding);
            json!({"visualLines": result.visual_lines, "skippedCount": result.skipped_count})
        })
        .collect();
    vec![(
        json!({
            "id": 2,
            "module": "modes/interactive/components/visual-truncate.ts",
            "operation": "visualTruncate",
            "cases": request_cases,
        }),
        json!(output),
    )]
}

fn markdown_transform_cases() -> Vec<(Value, Value)> {
    let transformers = json!([
        {"kind": "upper"},
        {"kind": "append", "text": "!"},
        {"kind": "append", "text": "?"},
    ]);
    let inputs = ["hello", "", "a\nb"];
    let rust_transformers: Vec<MarkdownTransformer> = vec![
        Arc::new(|markdown: &str, _context| markdown.to_uppercase()),
        Arc::new(|markdown: &str, _context| format!("{markdown}!")),
        Arc::new(|markdown: &str, _context| format!("{markdown}?")),
    ];
    let transform = create_markdown_transform(MarkdownMessageType::Assistant, true, rust_transformers);
    let output: Vec<String> = inputs.iter().map(|input| transform(input, 80)).collect();
    vec![(
        json!({
            "id": 3,
            "module": "modes/interactive/components/markdown-transform.ts",
            "operation": "markdownTransform",
            "messageType": "assistant",
            "isStreaming": true,
            "availableWidth": 80,
            "transformers": transformers,
            "inputs": inputs,
        }),
        json!(output),
    )]
}

fn key_hint_cases() -> Vec<(Value, Value)> {
    let keys = json!([
        {"key": "ctrl+a/shift+b"},
        {"key": "alt+left", "options": {"capitalize": true}},
        {"key": "escape"},
        {"key": "ctrl+enter"},
        {"key": ""},
    ]);
    let bindings = json!([
        "tui.select.confirm",
        "tui.select.cancel",
        "app.tools.expand",
        "missing.binding"
    ]);
    let hints = json!([
        {"binding": "tui.select.confirm", "description": "save"},
        {"binding": "tui.select.cancel", "description": "cancel"},
    ]);
    let raw_hints = json!([
        {"key": "↑↓", "description": "navigate"},
        {"key": "Tab", "description": "scope"},
    ]);
    let rust_keys = [
        ("ctrl+a/shift+b", KeyTextFormatOptions::default()),
        ("alt+left", KeyTextFormatOptions { capitalize: true }),
        ("escape", KeyTextFormatOptions::default()),
        ("ctrl+enter", KeyTextFormatOptions::default()),
        ("", KeyTextFormatOptions::default()),
    ];
    let rust_format: Vec<String> = rust_keys
        .iter()
        .map(|(key, options)| format_key_text(key, *options))
        .collect();
    let rust_key_text: Vec<String> = [
        "tui.select.confirm",
        "tui.select.cancel",
        "app.tools.expand",
        "missing.binding",
    ]
    .iter()
    .map(|binding| key_text(binding))
    .collect();
    let rust_key_display: Vec<String> = [
        "tui.select.confirm",
        "tui.select.cancel",
        "app.tools.expand",
        "missing.binding",
    ]
    .iter()
    .map(|binding| key_display_text(binding))
    .collect();
    let rust_hints: Vec<String> = [("tui.select.confirm", "save"), ("tui.select.cancel", "cancel")]
        .iter()
        .map(|(binding, description)| key_hint(&theme(), binding, description))
        .collect();
    let rust_raw_hints: Vec<String> = [("↑↓", "navigate"), ("Tab", "scope")]
        .iter()
        .map(|(key, description)| raw_key_hint(&theme(), key, description))
        .collect();
    vec![(
        json!({
            "id": 4,
            "module": "modes/interactive/components/keybinding-hints.ts",
            "operation": "keyHints",
            "keys": keys,
            "bindings": bindings,
            "hints": hints,
            "rawHints": raw_hints,
        }),
        json!({
            "formatKeyText": rust_format,
            "keyText": rust_key_text,
            "keyDisplayText": rust_key_display,
            "keyHint": rust_hints,
            "rawKeyHint": rust_raw_hints,
        }),
    )]
}

fn model_search_cases() -> Vec<(Value, Value)> {
    let items = json!([
        {"id": "gpt-5", "provider": "openai"},
        {"id": "gpt-5", "provider": "openrouter", "name": "GPT-5"},
        {"id": "claude", "provider": "anthropic", "name": ""},
    ]);
    let rust_items = [
        ModelSearchItem {
            id: "gpt-5".into(),
            provider: "openai".into(),
            name: None,
        },
        ModelSearchItem {
            id: "gpt-5".into(),
            provider: "openrouter".into(),
            name: Some("GPT-5".into()),
        },
        ModelSearchItem {
            id: "claude".into(),
            provider: "anthropic".into(),
            name: Some(String::new()),
        },
    ];
    let output: Vec<Value> = rust_items
        .iter()
        .map(|item| json!(get_model_search_candidates(item)))
        .collect();
    vec![(
        json!({
            "id": 5,
            "module": "modes/interactive/model-search.ts",
            "operation": "modelSearch",
            "items": items,
        }),
        json!(output),
    )]
}

fn approval_cases() -> Vec<(Value, Value)> {
    let events = Events::default();
    let mut component = ApprovalSelectorComponent::new(
        ApprovalSelectorOptions {
            current_preset: ApprovalPresetName::AskMe,
            on_select: {
                let events = events.clone();
                Rc::new(move |selection| {
                    record(
                        &events,
                        json!({"preset": selection.preset.as_str(), "scope": approval_scope_name(selection.scope)}),
                    );
                })
            },
            on_cancel: Rc::new(|| {}),
        },
        theme(),
    );
    for input in ["j", "\t", "\n"] {
        component.handle_input(input);
    }
    let lines = component.render(120);
    vec![(
        json!({
            "id": 6,
            "module": "modes/interactive/components/approval-selector.ts",
            "operation": "approval",
            "currentPreset": "ask-me",
            "inputs": ["j", "\t", "\n"],
            "width": 120,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn approval_scope_name(scope: ApprovalSelectorScope) -> &'static str {
    match scope {
        ApprovalSelectorScope::Global => "global",
        ApprovalSelectorScope::Project => "project",
    }
}

fn sandbox_cases() -> Vec<(Value, Value)> {
    let events = Events::default();
    let mut component = SandboxSelectorComponent::new(
        SandboxSelectorOptions {
            current_profile: SandboxProfileName::WorkspaceWrite,
            session_profile: None,
            on_select: {
                let events = events.clone();
                Rc::new(move |selection| record(&events, sandbox_event(selection)))
            },
            on_cancel: Rc::new(|| {}),
        },
        theme(),
    );
    for input in ["j", "\t", "\t", "\n"] {
        component.handle_input(input);
    }
    let lines = component.render(120);
    vec![(
        json!({
            "id": 7,
            "module": "modes/interactive/components/sandbox-selector.ts",
            "operation": "sandbox",
            "currentProfile": "workspace-write",
            "inputs": ["j", "\t", "\t", "\n"],
            "width": 120,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn sandbox_event(selection: SandboxSelectorSelection) -> Value {
    match selection {
        SandboxSelectorSelection::Persistent { profile, scope } => json!({
            "profile": profile.as_str(),
            "scope": sandbox_scope_name(scope),
        }),
        SandboxSelectorSelection::Session { profile } => {
            let mut event = serde_json::Map::new();
            if let Some(profile) = profile {
                event.insert("profile".to_string(), json!(profile.as_str()));
            }
            event.insert("scope".to_string(), json!("session"));
            Value::Object(event)
        }
    }
}

fn sandbox_scope_name(scope: SandboxProfileScope) -> &'static str {
    match scope {
        SandboxProfileScope::Global => "global",
        SandboxProfileScope::Project => "project",
        SandboxProfileScope::Session => "session",
    }
}

fn trust_cases() -> Vec<(Value, Value)> {
    let cases = [
        json!({
            "savedDecision": null,
            "projectTrusted": false,
            "inputs": ["j", "\n"],
        }),
        json!({
            "savedDecision": {"path": "/work/project", "decision": true},
            "projectTrusted": true,
            "inputs": ["\n"],
        }),
        json!({
            "savedDecision": {"path": "/work", "decision": false},
            "projectTrusted": false,
            "inputs": ["j", "j", "\n"],
        }),
    ];
    let mut requests = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let saved = match &case["savedDecision"] {
            Value::Null => None,
            value => Some(ProjectTrustStoreEntry {
                path: value["path"].as_str().expect("path").to_string(),
                decision: if value["decision"].as_bool().expect("decision") {
                    ProjectTrustDecision::Trusted
                } else {
                    ProjectTrustDecision::Untrusted
                },
            }),
        };
        let inputs: Vec<&str> = case["inputs"]
            .as_array()
            .expect("inputs")
            .iter()
            .map(|value| value.as_str().expect("input"))
            .collect();
        let events = Events::default();
        let mut component = TrustSelectorComponent::new(
            TrustSelectorOptions {
                cwd: "/work/project".to_string(),
                saved_decision: saved,
                project_trusted: case["projectTrusted"].as_bool().expect("projectTrusted"),
                on_select: {
                    let events = events.clone();
                    Rc::new(move |selection| {
                        let updates: Vec<Value> = selection
                            .updates
                            .iter()
                            .map(|update| {
                                json!({
                                    "path": &update.path,
                                    "decision": match update.decision {
                                        ProjectTrustDecision::Trusted => Value::Bool(true),
                                        ProjectTrustDecision::Untrusted => Value::Bool(false),
                                        ProjectTrustDecision::Unknown => Value::Null,
                                    },
                                })
                            })
                            .collect();
                        record(&events, json!({"trusted": selection.trusted, "updates": updates}));
                    })
                },
                on_cancel: Rc::new(|| {}),
            },
            theme(),
        );
        for input in &inputs {
            component.handle_input(input);
        }
        let lines = component.render(120);
        requests.push((
            json!({
                "id": 8 + index,
                "module": "modes/interactive/components/trust-selector.ts",
                "operation": "trust",
                "cwd": "/work/project",
                "savedDecision": case["savedDecision"],
                "projectTrusted": case["projectTrusted"],
                "inputs": inputs,
                "width": 120,
            }),
            json!({"lines": lines, "events": events_json(&events)}),
        ));
    }
    requests
}

fn show_images_cases() -> Vec<(Value, Value)> {
    let events = Events::default();
    let mut component = ShowImagesSelectorComponent::new(
        true,
        {
            let events = events.clone();
            Rc::new(move |show| record(&events, json!({"show": show})))
        },
        Rc::new(|| {}),
        theme(),
    );
    for input in ["\x1b[B", "\r"] {
        component.select_list_mut().handle_input(input);
    }
    let lines = component.render(80);
    vec![(
        json!({
            "id": 20,
            "module": "modes/interactive/components/show-images-selector.ts",
            "operation": "showImages",
            "currentValue": true,
            "inputs": ["\u{1b}[B", "\r"],
            "width": 80,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn thinking_cases() -> Vec<(Value, Value)> {
    let events = Events::default();
    let mut component = ThinkingSelectorComponent::new(
        ThinkingLevel::Low,
        &[ThinkingLevel::Off, ThinkingLevel::Low, ThinkingLevel::High],
        {
            let events = events.clone();
            Rc::new(move |level| record(&events, json!({"level": level.as_str()})))
        },
        Rc::new(|| {}),
        theme(),
    );
    for input in ["\x1b[B", "\r"] {
        component.select_list_mut().handle_input(input);
    }
    let lines = component.render(100);
    vec![(
        json!({
            "id": 21,
            "module": "modes/interactive/components/thinking-selector.ts",
            "operation": "thinking",
            "currentLevel": "low",
            "availableLevels": ["off", "low", "high"],
            "inputs": ["\u{1b}[B", "\r"],
            "width": 100,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn theme_cases(available_themes: &[String]) -> Vec<(Value, Value)> {
    let current_theme = available_themes.last().expect("at least one theme");
    let events = Events::default();
    let mut component = ThemeSelectorComponent::new(
        available_themes,
        current_theme,
        {
            let events = events.clone();
            Rc::new(move |name| record(&events, json!({"select": name})))
        },
        Rc::new(|| {}),
        {
            let events = events.clone();
            Rc::new(move |name| record(&events, json!({"preview": name})))
        },
        theme(),
    );
    for input in ["\x1b[B", "\r"] {
        component.select_list_mut().handle_input(input);
    }
    let lines = component.render(90);
    vec![(
        json!({
            "id": 22,
            "module": "modes/interactive/components/theme-selector.ts",
            "operation": "theme",
            "currentTheme": current_theme,
            "inputs": ["\u{1b}[B", "\r"],
            "width": 90,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn user_message_cases() -> Vec<(Value, Value)> {
    let messages = vec![
        UserMessageItem {
            id: "a".into(),
            text: "first message".into(),
            timestamp: None,
        },
        UserMessageItem {
            id: "b".into(),
            text: "second\nmessage that is quite long and will need truncation".into(),
            timestamp: Some("2026-01-01T00:00:00.000Z".into()),
        },
        UserMessageItem {
            id: "c".into(),
            text: "  spaced  ".into(),
            timestamp: None,
        },
    ];
    let events = Events::default();
    let mut component = UserMessageSelectorComponent::new(
        messages,
        {
            let events = events.clone();
            Rc::new(move |id| record(&events, json!({"select": id})))
        },
        Rc::new(|| {}),
        Some("b"),
        theme(),
    );
    for input in ["\x1b[A", "\x1b[A", "\r"] {
        component.message_list_mut().handle_input(input);
    }
    let lines = component.render(40);
    vec![(
        json!({
            "id": 23,
            "module": "modes/interactive/components/user-message-selector.ts",
            "operation": "userMessage",
            "initialSelectedId": "b",
            "messages": [
                {"id": "a", "text": "first message"},
                {"id": "b", "text": "second\nmessage that is quite long and will need truncation", "timestamp": "2026-01-01T00:00:00.000Z"},
                {"id": "c", "text": "  spaced  "},
            ],
            "inputs": ["\u{1b}[A", "\u{1b}[A", "\r"],
            "width": 40,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}

fn extension_cases() -> Vec<(Value, Value)> {
    let events = Events::default();
    let mut component = ExtensionSelectorComponent::new(
        "Pick one",
        vec!["alpha".into(), "beta".into(), "gamma".into()],
        {
            let events = events.clone();
            Rc::new(move |option| record(&events, json!({"select": option})))
        },
        Rc::new(|| {}),
        ExtensionSelectorOptions::default(),
        theme(),
    );
    for input in ["j", "j", "\t"] {
        component.handle_input(input);
    }
    let lines = component.render(70);
    vec![(
        json!({
            "id": 24,
            "module": "modes/interactive/components/extension-selector.ts",
            "operation": "extension",
            "title": "Pick one",
            "options": ["alpha", "beta", "gamma"],
            "inputs": ["j", "j", "\t"],
            "width": 70,
        }),
        json!({"lines": lines, "events": events_json(&events)}),
    )]
}
