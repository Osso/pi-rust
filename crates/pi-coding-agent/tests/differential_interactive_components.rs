#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;
use crate::shared_oracle as oracle;

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_coding_agent::core::extensions::types::{
    CustomMessageContent, CustomMessageValue, MessageRenderOptions, RenderShell, ToolDefinition,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::messages::{CompactionSummaryMessage, CompactionSummaryRole};
use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
use pi_coding_agent::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_coding_agent::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions,
};
use pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponent;
use pi_coding_agent::modes::interactive::components::custom_message::CustomMessageComponent;
use pi_coding_agent::modes::interactive::components::diff::render_diff;
use pi_coding_agent::modes::interactive::components::elapsed_time::format_elapsed_duration;
use pi_coding_agent::modes::interactive::components::extension_editor::{
    ExtensionEditorComponent, ExtensionEditorHost,
};
use pi_coding_agent::modes::interactive::components::extension_input::{
    ExtensionInputComponent, ExtensionInputOptions,
};
use pi_coding_agent::modes::interactive::components::first_time_setup::{
    FirstTimeSetupComponent, FirstTimeSetupOptions,
};
use pi_coding_agent::modes::interactive::components::keybinding_hints::{
    KeyTextFormatOptions, format_key_text, key_display_text, key_hint, key_text, raw_key_hint,
};
use pi_coding_agent::modes::interactive::components::status_indicator::{
    BranchSummaryStatusIndicator, CompactionStatusIndicator, CompactionStatusReason, IdleStatus, RetryStatusIndicator,
    WorkingStatusIndicator,
};
use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
use pi_coding_agent::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use pi_coding_agent::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use pi_coding_agent::modes::interactive::theme::{
    ColorMode, ColorValue, REQUIRED_COLORS, TerminalTheme, Theme, set_theme_instance,
};
use pi_tui::{Component, Text};
use serde_json::{Value, json};

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

#[test]
fn custom_messages_match_the_pinned_typescript_source() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (request, actual) in custom_message_cases() {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "oracle error for {request}: {expected}");
        assert_eq!(actual, expected["output"], "mismatch for {request}");
    }
}

#[test]
fn user_message_in_transit_matches_the_pinned_typescript_source() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let mut message = UserMessageComponent::new("hello **pending**", None, 1);
    let request = json!({
        "id": "in-transit", "module": "modes/interactive/components/user-message.ts",
        "operation": "userMessage", "text": "hello **pending**", "outputPad": 1,
        "inTransit": true, "width": 40,
    });
    for in_transit in [true, true, false, true, false] {
        message.set_in_transit(in_transit);
        let mut request = request.clone();
        request["inTransit"] = json!(in_transit);
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(json!(message.render(40)), expected["output"]);
    }
    for (request, actual) in user_message_cases() {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(actual, expected["output"]);
    }
}

#[test]
fn config_selector_matches_the_pinned_typescript_source() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (request, actual) in config_selector_cases() {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "oracle error for {request}: {expected}");
        assert_eq!(actual, expected["output"], "mismatch for {request}");
    }
}

#[test]
fn status_indicators_match_the_pinned_typescript_source() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (request, actual) in status_indicator_cases() {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(actual, expected["output"], "{request}");
    }
}

#[test]
fn assistant_message_rendering_matches_the_pinned_typescript_source() {
    compare_component_cases(assistant_message_cases);
}

#[test]
fn extension_editor_constructor_rendering_matches_the_pinned_typescript_source() {
    compare_component_cases(extension_editor_cases);
}

#[test]
fn footer_component_matches_the_pinned_typescript_source() {
    compare_component_cases(|| {
        let mut cases = footer_cwd_cases();
        cases.extend(footer_cases());
        cases
    });
}

#[test]
fn tree_selector_matches_the_pinned_typescript_source() {
    compare_component_cases(tree_selector_cases);
}

#[test]
fn session_selector_matches_the_pinned_typescript_source() {
    compare_component_cases(session_selector_cases);
}

#[test]
fn bash_execution_matches_the_pinned_typescript_source() {
    compare_component_cases(bash_execution_cases);
}

#[test]
fn interactive_leaf_helpers_match_the_pinned_typescript_source() {
    compare_component_cases(requests);
}

fn compare_component_cases(case_factory: fn() -> Vec<(Value, Value)>) {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (request, actual) in case_factory() {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "oracle error for {request}: {expected}");
        assert_eq!(actual, expected["output"], "mismatch for {request}");
    }
}

fn requests() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    cases.extend(elapsed_cases());
    cases.extend(visual_truncate_cases());
    cases.extend(key_hint_cases());
    cases.extend(model_search_cases());
    cases.extend(diff_cases());
    cases.extend(branch_summary_cases());
    cases.extend(compaction_summary_cases());
    cases.extend(bordered_loader_cases());
    cases.extend(extension_input_cases());
    cases.extend(first_time_setup_cases());
    cases.extend(custom_entry_cases());
    cases.extend(tool_execution_cases());
    cases.extend(tool_renderer_cases());
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
            let result = truncate_to_visual_lines(&JsString::from(*text), *max, *width, *padding);
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
    let rust_format: Vec<JsString> = rust_keys
        .iter()
        .map(|(key, options)| format_key_text(*key, *options))
        .collect();
    let rust_key_text: Vec<JsString> = [
        "tui.select.confirm",
        "tui.select.cancel",
        "app.tools.expand",
        "missing.binding",
    ]
    .iter()
    .map(|binding| key_text(*binding))
    .collect();
    let rust_key_display: Vec<JsString> = [
        "tui.select.confirm",
        "tui.select.cancel",
        "app.tools.expand",
        "missing.binding",
    ]
    .iter()
    .map(|binding| key_display_text(*binding))
    .collect();
    let rust_hints: Vec<JsString> = [("tui.select.confirm", "save"), ("tui.select.cancel", "cancel")]
        .iter()
        .map(|(binding, description)| key_hint(*binding, *description))
        .collect();
    let rust_raw_hints: Vec<JsString> = [("↑↓", "navigate"), ("Tab", "scope")]
        .iter()
        .map(|(key, description)| raw_key_hint(*key, *description))
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
            name: Some(Default::default()),
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

fn diff_cases() -> Vec<(Value, Value)> {
    let cases = [
        " 1 foo\n-2 old value here\n+2 new value here\n 3 baz",
        " 1 keep\n-2 a\n-3 b\n+2 c\n+3 d\n 4 tail",
        " 1 a\n+2 inserted\n 2 b",
        "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-1 old\n+1 new",
        "-2 a\tb\n+2 a\tc",
        "-2 foo   bar baz\n+2 foo  baz",
        "-2 foo baz\n+2 foo bar baz",
        "-2 foo bar baz\n+2 foo baz",
        "-2 café  x\n+2 café y",
        "-2  x\n+2 x",
        "-2 (foo)\n+2 (bar)",
        "-2 foo\n+2 foo bar",
        "-2 a  b  c\n+2 a b c",
        "-2 日本語 テスト\n+2 日本語 テスト2",
        " 1 same\n-2\n+2\n 3 end",
        "",
    ];
    let output: Vec<JsString> = cases.iter().map(|value| render_diff(&JsString::from(*value))).collect();
    vec![(
        json!({
            "id": 6,
            "module": "modes/interactive/components/diff.ts",
            "operation": "renderDiff",
            "cases": cases,
        }),
        json!(output),
    )]
}

fn config_selector_cases() -> Vec<(Value, Value)> {
    let package_root = "/tmp/pi-config-oracle/package";
    let resource = |path: &str, scope, origin, source: &str, base: Option<&str>| ResolvedResource {
        path: path.into(),
        enabled: true,
        metadata: PathMetadata {
            source: source.into(),
            scope,
            origin,
            base_dir: base.map(JsString::from),
        },
    };
    let package = ResolvedPaths {
        extensions: vec![resource(
            &format!("{package_root}/extensions/bar.ts"),
            SourceScope::User,
            MetadataOrigin::Package,
            "npm:pi-tools",
            Some(package_root),
        )],
        ..Default::default()
    };
    let mixed = ResolvedPaths {
        extensions: vec![
            resource(
                "/tmp/pi-config-oracle/project/.pi/extensions/local.ts",
                SourceScope::Project,
                MetadataOrigin::TopLevel,
                "auto",
                None,
            ),
            resource(
                "/tmp/pi-config-oracle/agent/extensions/user.ts",
                SourceScope::User,
                MetadataOrigin::TopLevel,
                "auto",
                None,
            ),
            resource(
                "/tmp/pi-config-oracle/package/plugin/index.ts",
                SourceScope::Project,
                MetadataOrigin::Package,
                "npm:project",
                Some(package_root),
            ),
        ],
        skills: vec![resource(
            "/tmp/pi-config-oracle/agent/skills/tool/SKILL.md",
            SourceScope::User,
            MetadataOrigin::TopLevel,
            "auto",
            Some("/tmp/pi-config-oracle/agent"),
        )],
        prompts: vec![resource(
            "/tmp/pi-config-oracle/project/.pi/prompts/guide.md",
            SourceScope::Project,
            MetadataOrigin::TopLevel,
            "settings",
            None,
        )],
        themes: vec![resource(
            "/tmp/pi-config-oracle/agent/themes/night.json",
            SourceScope::User,
            MetadataOrigin::TopLevel,
            "settings",
            None,
        )],
    };
    let missing = ResolvedPaths {
        extensions: vec![resource(
            "/tmp/pi-config-oracle/missing.ts",
            SourceScope::Temporary,
            MetadataOrigin::Package,
            "npm:absent",
            None,
        )],
        ..Default::default()
    };
    vec![
        config_selector_case(
            7,
            package.clone(),
            json!({"packages":["npm:pi-tools"]}),
            json!({}),
            &[" ", " ", " "],
            100,
            24,
        ),
        config_selector_case(
            8,
            package,
            json!({"packages":[{"source":"npm:pi-tools","skills":[]}]}),
            json!({}),
            &[" ", "\r", "\t", "\u{1b}", "\u{3}"],
            50,
            10,
        ),
        config_selector_case(
            9,
            mixed.clone(),
            json!({}),
            json!({"packages":["npm:project"]}),
            &[
                " ",
                "\u{1b}[B",
                " ",
                "\u{1b}[6~",
                "\r",
                "\u{1b}[5~",
                "\u{1b}[A",
                "tool",
                "\u{15}",
                "no-match",
                "\r",
            ],
            70,
            13,
        ),
        config_selector_case(
            10,
            mixed,
            json!({"extensions":["!extensions/user.ts","+extensions/user.ts","other.ts"]}),
            json!({}),
            &["\u{1b}[6~", " ", "\u{1b}[B", " ", "\u{1b}[B", " ", "\u{1b}[B", " "],
            100,
            24,
        ),
        config_selector_case(11, missing, json!({}), json!({}), &[" ", " "], 35, 24),
        config_selector_case(
            12,
            ResolvedPaths::default(),
            json!({}),
            json!({}),
            &["\u{1b}[6~", "\u{1b}[5~", " ", "\u{1b}"],
            15,
            8,
        ),
    ]
}

fn config_selector_case(
    id: u64,
    resolved: ResolvedPaths,
    global: Value,
    project: Value,
    inputs: &[&str],
    width: usize,
    height: usize,
) -> (Value, Value) {
    use pi_coding_agent::core::settings_manager::{InMemorySettingsStorage, SettingsScope};
    let mut storage = InMemorySettingsStorage::new();
    storage.set(SettingsScope::Global, Some(global.to_string()));
    storage.set(SettingsScope::Project, Some(project.to_string()));
    let manager = Rc::new(RefCell::new(SettingsManager::from_storage(
        Box::new(storage),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let events = Rc::new(RefCell::new(Vec::new()));
    let callback = |event: &'static str| {
        let events = Rc::clone(&events);
        Rc::new(move || events.borrow_mut().push(event)) as Rc<dyn Fn()>
    };
    let cwd = "/tmp/pi-config-oracle/project";
    let agent_dir = "/tmp/pi-config-oracle/agent";
    let request = json!({
        "id":id, "module":"modes/interactive/components/config-selector.ts", "operation":"configSelector",
        "resolved":resolved, "globalSettings":global, "projectSettings":project, "cwd":cwd, "agentDir":agent_dir,
        "inputs":inputs, "width":width, "height":height,
    });
    let mut selector = ConfigSelectorComponent::new(ConfigSelectorOptions {
        resolved_paths: resolved,
        settings_manager: Rc::clone(&manager),
        cwd: cwd.into(),
        agent_dir: agent_dir.into(),
        on_close: callback("close"),
        on_exit: callback("exit"),
        request_render: callback("render"),
        terminal_height: Some(height),
    })
    .unwrap();
    let mut renders = vec![selector.render(width)];
    for input in inputs {
        selector.handle_input(&JsString::from(*input));
        renders.push(selector.render(width));
    }
    let actual = json!({
        "globalSettings":JsValue::Object(manager.borrow().get_global_settings()).to_serde().expect("scalar settings fixture"),
        "projectSettings":JsValue::Object(manager.borrow().get_project_settings()).to_serde().expect("scalar settings fixture"),
        "events":events.borrow().clone(), "renders":renders,
    });
    (request, actual)
}

fn branch_summary_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, expanded) in [false, true].into_iter().enumerate() {
        let request = json!({
            "id": 20 + index,
            "module": "modes/interactive/components/branch-summary-message.ts",
            "operation": "branchSummary",
            "summary": "The branch covered **two** files.\n\n- one\n- two",
            "fromId": "entry-1",
            "timestamp": 1_700_000_000_000.0,
            "expanded": expanded,
            "width": 60,
        });
        let mut component = BranchSummaryMessageComponent::new(
            BranchSummaryMessage {
                role: BranchSummaryRole::BranchSummary,
                summary: JsString::from("The branch covered **two** files.\n\n- one\n- two"),
                from_id: JsString::from("entry-1"),
                timestamp: 1_700_000_000_000.0,
            },
            None,
        );
        component.set_expanded(expanded);
        cases.push((request, json!(component.render(60))));
    }
    cases
}

fn compaction_message(tokens_before: f64) -> CompactionSummaryMessage {
    CompactionSummaryMessage {
        role: CompactionSummaryRole::CompactionSummary,
        summary: "The summary text.".into(),
        tokens_before,
        tokens_after: None,
        kept_from_previous_context_tokens: None,
        compacted_result_tokens: None,
        duration_ms: None,
        provider_native: None,
        timestamp: 1_700_000_000_000.0,
    }
}

fn compaction_summary_cases() -> Vec<(Value, Value)> {
    let mut variants = vec![compaction_message(999.0), compaction_message(1234.0)];
    let mut with_after = compaction_message(286_705.0);
    with_after.tokens_after = Some(31_234.0);
    with_after.kept_from_previous_context_tokens = Some(26_667.0);
    with_after.compacted_result_tokens = Some(4567.0);
    variants.push(with_after);
    let mut with_duration = compaction_message(1234.0);
    with_duration.duration_ms = Some(4567.0);
    variants.push(with_duration);
    let mut short_duration = compaction_message(1500.0);
    short_duration.duration_ms = Some(999.0);
    variants.push(short_duration);

    let mut cases = Vec::new();
    for (index, message) in variants.into_iter().enumerate() {
        for (offset, expanded) in [false, true].into_iter().enumerate() {
            let request = json!({
                "id": 30 + index * 2 + offset,
                "module": "modes/interactive/components/compaction-summary-message.ts",
                "operation": "compactionSummary",
                "message": serde_json::to_value(&message).expect("message serializes"),
                "expanded": expanded,
                "width": 100,
            });
            let mut component = CompactionSummaryMessageComponent::new(message.clone(), None);
            component.set_expanded(expanded);
            cases.push((request, json!(component.render(100))));
        }
    }
    cases
}

fn assistant_fixture(content: Vec<AssistantContent>, stop_reason: StopReason) -> AssistantMessage {
    AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        model: "gpt-4o-mini".to_owned(),
        usage: Usage::default(),
        stop_reason,
        timestamp: 1_700_000_000_000.0,
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        error_message: None,
    }
}

fn assistant_message_cases() -> Vec<(Value, Value)> {
    let text_only = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("hello **world**"))],
        StopReason::Stop,
    );
    let with_tool = assistant_fixture(
        vec![
            AssistantContent::Text(TextContent::new("calling tool")),
            AssistantContent::ToolCall(ToolCall::new(
                "tool-1",
                "read",
                (serde_json::json!({"path": "file.txt"})
                    .as_object()
                    .cloned()
                    .expect("object"))
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
            )),
        ],
        StopReason::ToolUse,
    );
    let thinking_and_text = assistant_fixture(
        vec![
            AssistantContent::Thinking(ThinkingContent::new("first thought")),
            AssistantContent::Thinking(ThinkingContent::new("second thought")),
            AssistantContent::Text(TextContent::new("answer")),
        ],
        StopReason::Stop,
    );
    let length = assistant_fixture(
        vec![AssistantContent::Thinking(ThinkingContent::new("private"))],
        StopReason::Length,
    );
    let mut aborted = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("partial"))],
        StopReason::Aborted,
    );
    aborted.error_message = Some(JsString::from("Request was aborted"));
    let mut error = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("partial"))],
        StopReason::Error,
    );
    error.error_message = Some(JsString::from("boom"));

    let mut cases = Vec::new();
    let variants = vec![
        (text_only, false, 1, 80),
        (with_tool, false, 1, 60),
        (thinking_and_text.clone(), false, 1, 80),
        (thinking_and_text, true, 1, 80),
        (length, true, 1, 80),
        (aborted, false, 1, 80),
        (error, false, 0, 80),
    ];
    for (index, (message, hide_thinking, output_pad, width)) in variants.into_iter().enumerate() {
        let request = json!({
            "id": 40 + index,
            "module": "modes/interactive/components/assistant-message.ts",
            "operation": "assistantMessage",
            "message": serde_json::to_value(&message).expect("message serializes"),
            "hideThinkingBlock": hide_thinking,
            "hiddenThinkingLabel": "Thinking...",
            "outputPad": output_pad,
            "width": width,
        });
        let mut component = AssistantMessageComponent::new(
            Some(message),
            hide_thinking,
            None,
            Some(JsString::from("Thinking...")),
            output_pad,
        );
        cases.push((request, json!(component.render(width))));
    }

    let transformed = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("mixed Case"))],
        StopReason::Stop,
    );
    let request = json!({
        "id": 50,
        "module": "modes/interactive/components/assistant-message.ts",
        "operation": "assistantMessage",
        "message": serde_json::to_value(&transformed).expect("message serializes"),
        "hideThinkingBlock": false,
        "hiddenThinkingLabel": "Thinking...",
        "outputPad": 1,
        "width": 80,
    });
    let mut component =
        AssistantMessageComponent::new(Some(transformed), false, None, Some(JsString::from("Thinking...")), 1);
    cases.push((request, json!(component.render(80))));
    cases
}

fn user_message_cases() -> Vec<(Value, Value)> {
    let variants = [
        ("hello", 1usize, 40usize, false),
        ("# Heading\n\nSome **bold** text.", 1, 40, false),
        ("1. first\n2. second", 0, 30, false),
        ("mixed Case", 1, 80, true),
        ("suffix", 1, 15, true),
    ];
    variants
        .into_iter()
        .enumerate()
        .map(|(index, (text, output_pad, width, in_transit))| {
            let request = json!({
                "id": 60 + index, "module": "modes/interactive/components/user-message.ts",
                "operation": "userMessage", "text": text, "outputPad": output_pad,
                "inTransit": in_transit, "width": width,
            });
            let mut component = UserMessageComponent::new(text, None, output_pad);
            component.set_in_transit(in_transit);
            (request, json!(component.render(width)))
        })
        .collect()
}

fn bordered_loader_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, cancellable) in [true, false].into_iter().enumerate() {
        let request = json!({
            "id": 70 + index,
            "module": "modes/interactive/components/bordered-loader.ts",
            "operation": "borderedLoader",
            "message": "Working...",
            "cancellable": cancellable,
            "width": 60,
        });
        let mut loader = BorderedLoader::new(
            std::rc::Rc::new(|| {}),
            pi_coding_agent::modes::interactive::theme::current_theme(),
            "Working...",
            BorderedLoaderOptions {
                cancellable: Some(cancellable),
            },
        );
        let output = json!(loader.render(60));
        loader.dispose();
        cases.push((request, output));
    }
    cases
}

fn status_indicator_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    let mut push = |_id: u64, request: Value, output: Value| cases.push((request, output));

    let mut working = WorkingStatusIndicator::new(std::rc::Rc::new(|| {}), "Working...", None);
    push(
        80,
        json!({
            "id": 80,
            "module": "modes/interactive/components/status-indicator.ts",
            "operation": "statusIndicator",
            "kind": "working",
            "message": "Working...",
            "width": 40,
        }),
        json!(working.render(40)),
    );
    working.dispose();

    let mut retry = RetryStatusIndicator::new(std::rc::Rc::new(|| {}), 1.0, 3.0, 2500.0);
    push(
        81,
        json!({
            "id": 81,
            "module": "modes/interactive/components/status-indicator.ts",
            "operation": "statusIndicator",
            "kind": "retry",
            "attempt": 1,
            "maxAttempts": 3,
            "delayMs": 2500,
            "width": 80,
        }),
        json!(retry.render(80)),
    );
    retry.dispose();

    for (index, (reason, label)) in [
        (CompactionStatusReason::Manual, "manual"),
        (CompactionStatusReason::Threshold, "threshold"),
        (CompactionStatusReason::Overflow, "overflow"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut indicator = CompactionStatusIndicator::new(std::rc::Rc::new(|| {}), reason);
        push(
            82 + index as u64,
            json!({
                "id": 82 + index as u64,
                "module": "modes/interactive/components/status-indicator.ts",
                "operation": "statusIndicator",
                "kind": "compaction",
                "reason": label,
                "width": 90,
            }),
            json!(indicator.render(90)),
        );
        indicator.dispose();
    }

    let mut branch = BranchSummaryStatusIndicator::new(std::rc::Rc::new(|| {}));
    push(
        85,
        json!({
            "id": 85,
            "module": "modes/interactive/components/status-indicator.ts",
            "operation": "statusIndicator",
            "kind": "branchSummary",
            "width": 90,
        }),
        json!(branch.render(90)),
    );
    branch.dispose();

    let mut idle = IdleStatus;
    push(
        86,
        json!({
            "id": 86,
            "module": "modes/interactive/components/status-indicator.ts",
            "operation": "statusIndicator",
            "kind": "idle",
            "width": 20,
        }),
        json!(idle.render(20)),
    );
    cases
}

fn extension_input_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, (title, inputs, width)) in [("Name", vec!["a", "b", "c"], 60usize), ("Name", vec![], 20)]
        .into_iter()
        .enumerate()
    {
        let request = json!({
            "id": 90 + index,
            "module": "modes/interactive/components/extension-input.ts",
            "operation": "extensionInput",
            "title": title,
            "placeholder": null,
            "inputs": inputs,
            "width": width,
        });
        let mut input = ExtensionInputComponent::new(
            &JsString::from(title),
            None,
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            ExtensionInputOptions::default(),
        );
        for data in &inputs {
            input.handle_input(&JsString::from(*data));
        }
        cases.push((request, json!(input.render(width))));
    }
    cases
}

fn extension_editor_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, (title, prefill, width)) in [("Compose", Some("prefill"), 80usize), ("Compose", None, 40)]
        .into_iter()
        .enumerate()
    {
        let request = json!({
            "id": 100 + index,
            "module": "modes/interactive/components/extension-editor.ts",
            "operation": "extensionEditor",
            "title": title,
            "prefill": prefill,
            "width": width,
        });
        let host: std::rc::Rc<dyn ExtensionEditorHost> = std::rc::Rc::new(TestEditorHost);
        let mut editor = ExtensionEditorComponent::new(
            host,
            KeybindingsManager::new(Default::default(), None),
            &JsString::from(title),
            prefill.map(JsString::from).as_ref(),
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            pi_coding_agent::modes::interactive::components::extension_editor::ExtensionEditorOptions {
                editor: pi_tui::components::editor::EditorOptions::default(),
                external_editor_command: Some(JsString::from("nano")),
            },
        );
        cases.push((request, json!(editor.render(width))));
    }
    cases
}

struct TestEditorHost;

impl pi_tui::components::editor::EditorHost for TestEditorHost {
    fn terminal_rows(&self) -> usize {
        24
    }

    fn request_render(&self) {}

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        std::rc::Rc::new(TestFixedCell)
    }
}

struct TestFixedCell;

impl pi_tui::components::editor::EditorFixedCell for TestFixedCell {
    fn place(&self, _row: i64, _col: i64) {}
    fn clear(&self) {}
    fn update(&self, _value: &JsString) -> bool {
        true
    }
}

impl ExtensionEditorHost for TestEditorHost {
    fn suspend(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
    fn resume(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
    fn request_forced_render(&self) {}
}

fn first_time_setup_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, (detected, inputs)) in [
        (TerminalTheme::Dark, vec!["j", "\r", "j", "\r"]),
        (TerminalTheme::Light, vec!["\r", "\r"]),
    ]
    .into_iter()
    .enumerate()
    {
        let request = json!({
            "id": 110 + index,
            "module": "modes/interactive/components/first-time-setup.ts",
            "operation": "firstTimeSetup",
            "detectedTheme": detected.as_str(),
            "inputs": inputs,
            "width": 100,
        });
        let previews = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let submitted = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let preview_sink = previews.clone();
        let submit_sink = submitted.clone();
        let mut setup = FirstTimeSetupComponent::new(FirstTimeSetupOptions {
            detected_theme: detected,
            on_theme_preview: std::rc::Rc::new(move |theme| preview_sink.borrow_mut().push(theme.as_str())),
            on_submit: std::rc::Rc::new(move |result| {
                submit_sink.borrow_mut().push(json!({
                    "theme": result.theme.as_str(),
                    "shareAnalytics": result.share_analytics,
                }));
            }),
            on_cancel: std::rc::Rc::new(|| {}),
        });
        let mut renders = vec![json!(setup.render(100))];
        for data in &inputs {
            setup.handle_input(&JsString::from(*data));
            renders.push(json!(setup.render(100)));
        }
        let output = json!({
            "renders": renders,
            "previews": *previews.borrow(),
            "submitted": *submitted.borrow(),
        });
        cases.push((request, output));
    }
    cases
}

fn custom_entry_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    for (index, expanded) in [false, true].into_iter().enumerate() {
        let request = json!({
            "id": 120 + index,
            "module": "modes/interactive/components/custom-entry.ts",
            "operation": "customEntry",
            "customType": "test",
            "expanded": expanded,
            "width": 40,
        });
        let renderer: pi_coding_agent::core::extensions::types::EntryRenderer = Arc::new(
            |entry: &pi_coding_agent::core::session_manager::CustomEntry,
             options: &pi_coding_agent::core::extensions::types::EntryRenderOptions,
             _theme: &pi_coding_agent::modes::interactive::theme::Theme| {
                let component: pi_tui::tui::ComponentRef =
                    std::rc::Rc::new(std::cell::RefCell::new(pi_tui::Text::new(
                        js_format!("custom {} expanded={}", entry.custom_type, options.expanded.to_string()),
                        0,
                        0,
                    )));
                Some(component)
            },
        );
        let entry = pi_coding_agent::core::session_manager::CustomEntry {
            custom_type: JsString::from("test"),
            data: None,
            id: JsString::from("entry"),
            parent_id: None,
            timestamp: JsString::from("2026-01-01T00:00:00.000Z"),
        };
        let mut component = CustomEntryComponent::new(
            entry,
            renderer,
            pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponentOptions {
                session_id: "fixture".into(),
                request_render: std::rc::Rc::new(|| Ok(false)),
            },
        );
        component.set_expanded(expanded);
        cases.push((
            request,
            json!({"lines": component.render(40), "hasContent": component.has_content()}),
        ));
    }

    let request = json!({
        "id": 122,
        "module": "modes/interactive/components/custom-entry.ts",
        "operation": "customEntryEmpty",
        "customType": "empty",
        "width": 40,
    });
    let renderer: pi_coding_agent::core::extensions::types::EntryRenderer = Arc::new(|_entry, _options, _theme| None);
    let entry = pi_coding_agent::core::session_manager::CustomEntry {
        custom_type: JsString::from("empty"),
        data: None,
        id: JsString::from("entry"),
        parent_id: None,
        timestamp: JsString::from("2026-01-01T00:00:00.000Z"),
    };
    let mut component = CustomEntryComponent::new(
        entry,
        renderer,
        pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponentOptions {
            session_id: "fixture".into(),
            request_render: std::rc::Rc::new(|| Ok(false)),
        },
    );
    cases.push((
        request,
        json!({"lines": component.render(40), "hasContent": component.has_content()}),
    ));
    cases
}

fn custom_message_value(content: CustomMessageContent) -> CustomMessageValue {
    CustomMessageValue {
        custom_type: JsString::from("test"),
        content,
        display: true,
        details: None,
        timestamp: 0.0,
    }
}

fn custom_message_cases() -> Vec<(Value, Value)> {
    let variants = [
        (json!("hello **world**"), false, false, false, 40usize),
        (
            json!([{"type": "text", "text": "first"}, {"type": "image", "data": "AA==", "mimeType": "image/png"}, {"type": "text", "text": "second"}]),
            false,
            false,
            false,
            50,
        ),
        (json!("custom"), true, false, true, 40),
        (json!("renderer **failed**"), true, true, true, 40),
    ];
    let mut cases = Vec::new();
    for (index, (content, custom_renderer, renderer_throws, expanded, width)) in variants.into_iter().enumerate() {
        let request = json!({
            "id": 130 + index,
            "module": "modes/interactive/components/custom-message.ts",
            "operation": "customMessage",
            "customType": "test",
            "content": content,
            "customRenderer": custom_renderer,
            "rendererThrows": renderer_throws,
            "expanded": expanded,
            "width": width,
        });
        let rust_content = match &content {
            Value::String(text) => CustomMessageContent::Text(JsString::from(text.clone())),
            Value::Array(blocks) => CustomMessageContent::Blocks(
                blocks
                    .iter()
                    .filter_map(|block| {
                        if block["type"] == "text" {
                            Some(pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                                block["text"].as_str().unwrap_or_default(),
                            )))
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
            _ => panic!("unexpected content"),
        };
        type MessageRenderer = pi_coding_agent::core::extensions::types::MessageRenderer;
        let renderer: Option<MessageRenderer> = custom_renderer.then(|| {
            Arc::new(
                move |_message: &CustomMessageValue,
                      options: &MessageRenderOptions,
                      _theme: &pi_coding_agent::modes::interactive::theme::Theme| {
                    assert!(!renderer_throws, "fixture renderer throw");
                    let component: pi_tui::tui::ComponentRef = std::rc::Rc::new(std::cell::RefCell::new(
                        pi_tui::Text::new(format!("custom expanded={}", options.expanded), 0, 0),
                    ));
                    Some(component)
                },
            ) as pi_coding_agent::core::extensions::types::MessageRenderer
        });
        let mut component = CustomMessageComponent::new(custom_message_value(rust_content), renderer, None);
        component.set_expanded(expanded);
        cases.push((request, json!(component.render(width))));
    }
    cases
}

fn bash_execution_cases() -> Vec<(Value, Value)> {
    type BashCase = (bool, Vec<&'static str>, bool, Option<f64>, bool, bool, usize);
    let variants: Vec<BashCase> = vec![
        (false, vec!["hello\n"], true, Some(0.0), false, false, 60),
        (true, vec!["hello\n"], true, Some(0.0), false, false, 60),
        (
            false,
            vec!["\u{1b}[31mred\u{1b}[0m\r\nline\r"],
            true,
            Some(3.0),
            false,
            false,
            60,
        ),
        (false, vec!["partial"], true, None, true, false, 60),
        (false, vec!["one\ntwo\n"], false, Some(0.0), false, false, 60),
        (false, vec!["fraction\n"], true, Some(1.5), false, false, 60),
        (false, vec!["exponent\n"], true, Some(1e21), false, false, 60),
    ];
    let mut cases = Vec::new();
    for (index, (exclude, chunks, complete, exit_code, cancelled, expanded, width)) in variants.into_iter().enumerate()
    {
        let request = json!({
            "id": 140 + index,
            "module": "modes/interactive/components/bash-execution.ts",
            "operation": "bashExecution",
            "command": "echo hi",
            "excludeFromContext": exclude,
            "chunks": chunks,
            "complete": complete,
            "exitCode": exit_code,
            "cancelled": cancelled,
            "truncation": null,
            "fullOutputPath": null,
            "expanded": expanded,
            "width": width,
        });
        let mut component = BashExecutionComponent::new(
            "echo hi",
            std::rc::Rc::new(|| {}),
            exclude,
            BashExecutionOptions {
                show_elapsed: Some(false),
            },
        );
        for chunk in &chunks {
            component.append_output(*chunk);
        }
        if complete {
            component.set_complete(exit_code, cancelled, None, None);
        }
        component.set_expanded(expanded);
        let lines = component.render(width);
        let output = component.get_output();
        let command = component.get_command().to_owned();
        component.dispose();
        cases.push((request, json!({"lines": lines, "output": output, "command": command})));
    }
    cases
}

fn build_tree_nodes(
    entries: Vec<pi_coding_agent::core::session_manager::SessionEntry>,
) -> Vec<pi_coding_agent::core::session_manager::SessionTreeNode> {
    use pi_coding_agent::core::session_manager::SessionTreeNode;
    let mut nodes: Vec<SessionTreeNode> = entries
        .into_iter()
        .map(|entry| SessionTreeNode {
            entry,
            children: Vec::new(),
            label: None,
            label_timestamp: None,
        })
        .collect();
    let ids: Vec<(JsString, usize)> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.entry.id().expect("fixture entry has id").clone(), index))
        .collect();
    let mut roots = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        match node.entry.parent_id() {
            None => roots.push(index),
            Some(parent_id) => {
                if let Some((_, parent_index)) = ids.iter().find(|(id, _)| id == parent_id) {
                    children[*parent_index].push(index);
                } else {
                    roots.push(index);
                }
            }
        }
    }
    fn build(index: usize, nodes: &mut Vec<SessionTreeNode>, children: &[Vec<usize>]) -> SessionTreeNode {
        let mut node = nodes[index].clone();
        node.children = children[index]
            .iter()
            .map(|child| build(*child, nodes, children))
            .collect();
        node
    }
    roots.iter().map(|index| build(*index, &mut nodes, &children)).collect()
}

fn tree_entries_user(
    id: &str,
    parent: Option<&str>,
    content: &str,
) -> pi_coding_agent::core::session_manager::SessionEntry {
    use pi_agent_core::agent::AgentMessage;
    use pi_ai::types::{UserContent, UserMessage, UserRole};
    use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionMessageEntry};
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: JsString::from(id.to_owned()),
        parent_id: parent.map(str::to_owned).map(JsString::from),
        timestamp: JsString::from("2026-01-01T00:00:00.000Z"),
        message: Box::new(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(JsString::from(content.to_owned())),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        })),
    }))
}

fn tree_entries_assistant(
    id: &str,
    parent: Option<&str>,
    text: &str,
) -> pi_coding_agent::core::session_manager::SessionEntry {
    use pi_agent_core::agent::AgentMessage;
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextContent};
    use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionMessageEntry};
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4", 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: JsString::from(id.to_owned()),
        parent_id: parent.map(str::to_owned).map(JsString::from),
        timestamp: JsString::from("2026-01-01T00:00:00.000Z"),
        message: Box::new(AgentMessage::Assistant(Box::new(message))),
    }))
}

fn tree_selector_cases() -> Vec<(Value, Value)> {
    use pi_coding_agent::modes::interactive::components::tree_selector::FilterMode;

    let metadata_entries = vec![
        tree_entries_user("user-1", None, "hello"),
        tree_entries_assistant("asst-1", Some("user-1"), "hi"),
        tree_entries_user("user-2", Some("asst-1"), "active branch"),
        pi_coding_agent::core::session_manager::SessionEntry::Known(
            pi_coding_agent::core::session_manager::KnownSessionEntry::ModelChange(
                pi_coding_agent::core::session_manager::ModelChangeEntry {
                    id: JsString::from("model-1"),
                    parent_id: Some(JsString::from("user-2")),
                    timestamp: JsString::from("2026-01-01T00:00:00.000Z"),
                    provider: JsString::from("anthropic"),
                    model_id: JsString::from("claude-sonnet-4"),
                },
            ),
        ),
        tree_entries_user("user-3", Some("asst-1"), "sibling branch"),
    ];

    let branching_entries = vec![
        tree_entries_user("user-1", None, "first message"),
        tree_entries_assistant("asst-1", Some("user-1"), "response 1"),
        tree_entries_user("user-2", Some("asst-1"), "second message"),
        tree_entries_assistant("asst-2", Some("user-2"), "response 2"),
        tree_entries_user("user-3a", Some("asst-2"), "branch A start"),
        tree_entries_assistant("asst-3a", Some("user-3a"), "branch A response"),
        tree_entries_user("user-4a", Some("asst-3a"), "branch A deep"),
        tree_entries_assistant("asst-4a", Some("user-4a"), "branch A leaf"),
        tree_entries_user("user-3b", Some("asst-2"), "branch B start"),
        tree_entries_assistant("asst-3b", Some("user-3b"), "branch B response"),
        tree_entries_user("user-4b", Some("asst-3b"), "branch B deep"),
    ];

    let mut numeric_entry = tree_entries_assistant("numeric", Some("numeric-user"), "tool args");
    if let pi_coding_agent::core::session_manager::SessionEntry::Known(
        pi_coding_agent::core::session_manager::KnownSessionEntry::Message(entry),
    ) = &mut numeric_entry
        && let pi_agent_core::agent::AgentMessage::Assistant(message) = entry.message.as_mut()
    {
        message.content.push(AssistantContent::ToolCall(ToolCall::new(
            "call",
            "unknown",
            serde_json::from_str(r#"{"2":2.0,"1":1.0,"pad":"abcdefghijklmnopqrstuvwxyz"}"#).unwrap(),
        )));
    }
    type TreeCase<'a> = (
        u64,
        Vec<pi_coding_agent::core::session_manager::SessionEntry>,
        &'a str,
        Option<FilterMode>,
        Vec<&'a str>,
        usize,
        Value,
    );
    let variants: Vec<TreeCase> = vec![
        (169, vec![tree_entries_user("numeric-user", None, "hello"), numeric_entry,
            serde_json::from_value(json!({"type":"message","id":"numeric-result","parentId":"numeric","timestamp":"2026-01-01T00:00:00.000Z",
                "message":{"role":"toolResult","toolCallId":"call","toolName":"unknown","content":[{"type":"text","text":"ok"}],"isError":false,"timestamp":1}})).unwrap()], "numeric-result", Some(FilterMode::All), vec![], 160, json!({})),
        (160, metadata_entries.clone(), "model-1", None, vec![], 120, json!({})),
        (
            161,
            branching_entries.clone(),
            "asst-4a",
            None,
            vec!["\u{1b}[1;5D", "\u{1b}[1;5D", "\u{1b}[B", "\u{1b}[1;5C"],
            100,
            json!({}),
        ),
        (
            162,
            branching_entries.clone(),
            "asst-4a",
            None,
            vec![
                "\u{1b}[1;5D",
                "\u{1b}[1;5D",
                "\u{1b}[1;5D",
                "\u{1b}[1;5D",
                "\u{1b}[1;5D",
                "\u{1b}[1;5C",
            ],
            100,
            json!({}),
        ),
        (
            163,
            branching_entries.clone(),
            "asst-4a",
            Some(FilterMode::UserOnly),
            vec![],
            120,
            json!({}),
        ),
        (
            164,
            branching_entries.clone(),
            "asst-4a",
            None,
            vec!["b", "\u{1b}"],
            100,
            json!({}),
        ),
        (
            165,
            branching_entries.clone(),
            "asst-4a",
            None,
            vec!["\u{18}"],
            100,
            json!({}),
        ),
        (
            166,
            branching_entries.clone(),
            "asst-4a",
            None,
            vec!["\u{1b}[1;5D", "\u{1b}[1;5D", "\u{14}"],
            100,
            json!({}),
        ),
        (
            167,
            metadata_entries.clone(),
            "user-3",
            None,
            vec!["\u{13}"],
            100,
            json!({"user-2": "active label"}),
        ),
        (
            168,
            metadata_entries,
            "model-1",
            None,
            vec!["\u{1b}[1;5D"],
            60,
            json!({}),
        ),
    ];

    let mut cases = Vec::new();
    for (id, entries, leaf, filter, inputs, width, labels) in variants {
        let entries_json = serde_json::to_value(&entries).expect("entries serialize");
        let request = json!({
            "id": id,
            "module": "modes/interactive/components/tree-selector.ts",
            "operation": "treeSelector",
            "entries": entries_json,
            "currentLeafId": leaf,
            "terminalHeight": 24,
            "initialFilterMode": filter.map(|filter| match filter {
                FilterMode::Default => "default",
                FilterMode::NoTools => "no-tools",
                FilterMode::UserOnly => "user-only",
                FilterMode::LabeledOnly => "labeled-only",
                FilterMode::All => "all",
            }),
            "inputs": inputs,
            "width": width,
            "labels": labels,
        });

        let mut selector = pi_coding_agent::modes::interactive::components::tree_selector::TreeSelectorComponent::new(
            build_tree_nodes(entries),
            Some(JsString::from(leaf.to_owned())),
            24.0,
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            None,
            pi_coding_agent::modes::interactive::components::tree_selector::TreeSelectorOptions {
                initial_selected_id: None,
                initial_filter_mode: filter,
            },
        );
        if let Some(labels) = request["labels"].as_object() {
            for (entry_id, label) in labels {
                selector.tree_list().borrow_mut().update_node_label(
                    &JsString::from(entry_id),
                    label.as_str().map(JsString::from).as_ref(),
                    None,
                );
            }
        }

        let mut renders = vec![json!(selector.render(width))];
        let mut selected_ids = vec![json!(selector.tree_list().borrow().selected_entry_id().cloned())];
        for input in &inputs {
            selector.handle_input(&JsString::from(*input));
            renders.push(json!(selector.render(width)));
            selected_ids.push(json!(selector.tree_list().borrow().selected_entry_id().cloned()));
        }
        let output = json!({"renders": renders, "selectedIds": selected_ids});
        cases.push((request, output));
    }
    cases
}

fn session_info_fixture(
    path: &str,
    name: Option<&str>,
    first_message: &str,
    modified_offset_ms: i64,
    archived: bool,
) -> pi_coding_agent::core::session_manager::SessionInfo {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or_default())
        .unwrap_or_default();
    pi_coding_agent::core::session_manager::SessionInfo {
        path: JsString::from(path.to_owned()),
        id: JsString::from(format!("id-{path}")),
        cwd: JsString::from("/repo"),
        name: name.map(str::to_owned).map(JsString::from),
        is_archived: archived.then_some(true),
        parent_session_path: None,
        created: (now - modified_offset_ms) as f64,
        modified: (now - modified_offset_ms) as f64,
        message_count: 4,
        first_message: JsString::from(first_message.to_owned()),
    }
}

fn session_json(session: &pi_coding_agent::core::session_manager::SessionInfo) -> Value {
    json!({
        "path": session.path,
        "id": session.id,
        "cwd": session.cwd,
        "name": session.name,
        "isArchived": session.is_archived,
        "parentSessionPath": session.parent_session_path,
        "created": session.created,
        "modified": session.modified,
        "messageCount": session.message_count,
        "firstMessage": session.first_message,

    })
}

fn session_selector_cases() -> Vec<(Value, Value)> {
    use pi_coding_agent::core::keybindings::KeybindingsManager;
    use pi_coding_agent::modes::interactive::components::session_selector::{
        SessionListLoader, SessionSelectorComponent,
    };

    fn loader(sessions: Vec<pi_coding_agent::core::session_manager::SessionInfo>) -> SessionListLoader {
        std::rc::Rc::new(move |_progress| {
            let sessions = sessions.clone();
            Box::pin(async move { Ok(sessions) })
        })
    }

    let current = vec![
        session_info_fixture("/sessions/alpha.jsonl", Some("alpha"), "alpha prompt", 7_200_000, false),
        session_info_fixture("/sessions/beta.jsonl", None, "beta prompt", 2 * 86_400_000, false),
        session_info_fixture("/sessions/gamma.jsonl", None, "gamma prompt", 180_000, false),
    ];
    let all = vec![
        session_info_fixture("/sessions/alpha.jsonl", Some("alpha"), "alpha prompt", 7_200_000, false),
        session_info_fixture("/sessions/beta.jsonl", None, "beta prompt", 2 * 86_400_000, false),
        session_info_fixture("/other/delta.jsonl", Some("delta"), "delta prompt", 3_600_000, false),
    ];
    let archived = vec![session_info_fixture(
        "/sessions/old.jsonl",
        Some("old"),
        "archived prompt",
        5 * 86_400_000,
        true,
    )];

    type SessionCase<'a> = (u64, Vec<&'a str>, Option<&'a str>, bool, usize);
    let variants: Vec<SessionCase> = vec![
        (180, vec![], Some("/sessions/beta.jsonl"), false, 120),
        (181, vec!["\t"], None, false, 120),
        (182, vec!["\t", "\t"], None, true, 120),
        (183, vec!["\u{13}", "\u{13}", "\u{13}"], None, false, 120),
        (184, vec!["\u{e}"], None, false, 120),
        (185, vec!["a", "l", "p"], None, false, 120),
        (186, vec!["\u{1b}[B", "\u{1b}[B", "\u{1b}[A"], None, false, 120),
        (187, vec!["\u{10}"], None, false, 120),
        (188, vec!["\t", "\u{13}"], None, false, 100),
    ];

    let mut cases = Vec::new();
    for (id, inputs, current_file, with_archived, width) in variants {
        let request = json!({
            "id": id,
            "module": "modes/interactive/components/session-selector.ts",
            "operation": "sessionSelector",
            "currentSessions": current.iter().map(session_json).collect::<Vec<_>>(),
            "allSessions": all.iter().map(session_json).collect::<Vec<_>>(),
            "archivedSessions": with_archived.then(|| archived.iter().map(session_json).collect::<Vec<_>>()),
            "currentSessionFile": current_file,
            "inputs": inputs,
            "width": width,
        });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let output = runtime.block_on(async {
            let mut selector = SessionSelectorComponent::new(
                loader(current.clone()),
                loader(all.clone()),
                std::rc::Rc::new(|_| {}),
                std::rc::Rc::new(|| {}),
                std::rc::Rc::new(|| {}),
                std::rc::Rc::new(|| {}),
                pi_coding_agent::modes::interactive::components::session_selector::SessionSelectorOptions {
                    rename_session: None,
                    show_rename_hint: None,
                    keybindings: Some(KeybindingsManager::new(Default::default(), None)),
                    control_db_path: None,
                    archived_sessions_loader: with_archived.then(|| loader(archived.clone())),
                    current_session_file_path: current_file.map(JsString::from).as_ref(),
                },
            );
            selector.load_current_sessions().await;

            let mut renders = vec![json!(selector.render(width))];
            let mut selected = vec![json!(
                selector.session_list().borrow().get_selected_session_path().cloned()
            )];
            for input in &inputs {
                selector.handle_input(&JsString::from(*input));
                if selector.has_pending_action() {
                    selector.run_pending_action().await;
                }
                renders.push(json!(selector.render(width)));
                selected.push(json!(
                    selector.session_list().borrow().get_selected_session_path().cloned()
                ));
            }
            json!({"renders": renders, "selected": selected})
        });
        cases.push((request, output));
    }
    cases
}

fn tool_content(value: &Value) -> Vec<pi_ai::types::TextOrImageContent> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .map(|item| {
                    pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                        item.get("text").and_then(Value::as_str).unwrap_or(""),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn custom_tool_definition(call: &'static str, result: &'static str, shell: RenderShell) -> ToolDefinition {
    use pi_agent_core::agent::types::AgentToolResult;
    ToolDefinition {
        name: "custom_tool".to_owned(),
        label: "custom_tool".into(),
        description: JsString::default(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {}}),
        render_shell: Some(shell),
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_tool_call_id, _params, _signal, _on_update, _context| {
            Box::pin(async move {
                Ok(AgentToolResult {
                    content: vec![],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: Some(Arc::new(move |_args, _theme, _context| {
            Ok(Rc::new(RefCell::new(Text::new(call, 0, 0))))
        })),
        render_result: Some(Arc::new(move |_result, options, _theme, _context| {
            Ok(Rc::new(RefCell::new(Text::new(
                format!("{result} expanded={}", options.expanded),
                0,
                0,
            ))))
        })),
    }
}

fn tool_execution_case(
    id: u64,
    tool_name: &str,
    definition: Option<ToolDefinition>,
    definition_json: Value,
    args: Value,
    mut events: Vec<Value>,
    width: usize,
) -> (Value, Value) {
    use pi_coding_agent::modes::interactive::components::tool_execution::{
        ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
    };

    for event in &mut events {
        if event["type"] == "result" {
            event["finishedAt"] = json!(1_767_225_600_000.0);
        }
    }
    let request = json!({
        "id": id,
        "module": "modes/interactive/components/tool-execution.ts",
        "operation": "toolExecution",
        "toolName": tool_name,
        "toolCallId": "call-1",
        "args": args,
        "options": {},
        "definition": definition_json,
        "events": events,
        "width": width,
        "cwd": "/tmp/pi-tool-execution",
    });

    let request_render = Arc::new(|| {});
    let component = ToolExecutionComponent::new(
        tool_name,
        "call-1",
        JsValue::from(args),
        ToolExecutionOptions::default(),
        definition,
        request_render,
        "/tmp/pi-tool-execution",
    );
    let mut component = component.borrow_mut();
    let mut renders = Vec::new();
    for event in events {
        match event.get("type").and_then(Value::as_str) {
            Some("args") => component.update_args(JsValue::from(event.get("args").cloned().unwrap_or(Value::Null))),
            Some("argsComplete") => component.set_args_complete(),
            Some("expanded") => component.set_expanded(event.get("expanded").and_then(Value::as_bool) == Some(true)),
            Some("result") => {
                let result = event.get("result").cloned().unwrap_or(Value::Null);
                component.update_result(
                    ToolExecutionResult {
                        content: tool_content(result.get("content").unwrap_or(&Value::Null)),
                        details: result.get("details").cloned().map(JsValue::from),
                        is_error: result.get("isError").and_then(Value::as_bool) == Some(true),
                    },
                    event.get("isPartial").and_then(Value::as_bool) == Some(true),
                    event.get("finishedAt").and_then(Value::as_f64),
                );
            }
            _ => {}
        }
        renders.push(json!(component.render(width)));
    }
    (request, json!(renders))
}

fn tool_execution_cases() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    cases.push(tool_execution_case(
        300,
        "mystery",
        None,
        Value::Null,
        json!({"answer": 42}),
        vec![
            json!({"type": "args", "args": {"answer": 43}}),
            json!({"type": "argsComplete"}),
            json!({"type": "result", "result": {"content": [{"type": "text", "text": "hello"}], "isError": false}}),
        ],
        60,
    ));
    cases.push(tool_execution_case(
        301,
        "list_sessions",
        None,
        Value::Null,
        json!({}),
        vec![
            json!({"type": "argsComplete"}),
            json!({"type": "result", "result": {"content": [{"type": "text", "text": "line1\nline2"}], "isError": false}}),
        ],
        80,
    ));
    cases.push(tool_execution_case(
        302,
        "list_sessions",
        None,
        Value::Null,
        json!({}),
        vec![
            json!({"type": "argsComplete"}),
            json!({"type": "result", "result": {"content": [], "isError": true}}),
        ],
        80,
    ));
    let long_text = (0..150)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    cases.push(tool_execution_case(
        303,
        "list_sessions",
        None,
        Value::Null,
        json!({}),
        vec![
            json!({"type": "argsComplete"}),
            json!({"type": "result", "result": {"content": [{"type": "text", "text": long_text}], "isError": false}}),
        ],
        80,
    ));
    let definition_json = json!({"name": "custom_tool", "shell": "self", "call": "CALL", "result": "RES"});
    cases.push(tool_execution_case(
        304,
        "custom_tool",
        Some(custom_tool_definition("CALL", "RES", RenderShell::SelfRender)),
        definition_json,
        json!({"x": 1}),
        vec![
            json!({"type": "result", "result": {"content": [{"type": "text", "text": "ignored"}], "isError": false}}),
            json!({"type": "expanded", "expanded": true}),
        ],
        70,
    ));
    let definition_json = json!({"name": "custom_tool", "shell": "default", "call": "CALL", "result": "RES"});
    cases.push(tool_execution_case(
        305,
        "custom_tool",
        Some(custom_tool_definition("CALL", "RES", RenderShell::Default)),
        definition_json,
        json!({}),
        vec![json!({"type": "result", "result": {"content": [], "isError": false}})],
        70,
    ));
    cases.push(tool_execution_case(
        306,
        "end_turn",
        None,
        Value::Null,
        json!({"reason": "done"}),
        vec![json!({"type": "result", "result": {"content": [{"type": "text", "text": "Turn ended: done"}], "isError": false}})],
        70,
    ));
    cases
}

fn tool_renderer_cases() -> Vec<(Value, Value)> {
    use pi_agent_core::agent::types::AgentToolResult;
    use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions};
    use pi_coding_agent::core::tools::{ToolsOptions, create_all_tool_definitions};

    let cases = vec![
        json!({"tool": "change_working_directory", "kind": "call", "args": {"path": "/repo/x"}}),
        json!({"tool": "change_working_directory", "kind": "call", "args": {"id": "abc"}}),
        json!({"tool": "change_working_directory", "kind": "result", "content": [{"type": "text", "text": "Changed working directory to /repo/x"}], "isError": false}),
        json!({"tool": "resume_session", "kind": "call", "args": {"path": "/s.jsonl", "starter_prompt": "go"}}),
        json!({"tool": "resume_session", "kind": "call", "args": {"name": "named"}}),
        json!({"tool": "resume_session", "kind": "result", "content": [{"type": "text", "text": "Resume cancelled"}], "isError": true}),
        json!({"tool": "search_current_session_history", "kind": "call", "args": {"query": "needle"}}),
        json!({"tool": "search_current_session_history", "kind": "call", "args": {}}),
        json!({"tool": "search_current_session_history", "kind": "result", "content": [{"type": "text", "text": "{\"x\":1}"}], "isError": false}),
        json!({"tool": "broadcast", "kind": "call", "args": {"session_ids": ["a", "b"], "cwd": "/repo", "name": "worker", "status": ["running", "idle"]}}),
        json!({"tool": "broadcast", "kind": "call", "args": {}}),
        json!({"tool": "broadcast", "kind": "result", "content": [{"type": "text", "text": "id outcome=ok check=alive"}], "isError": false}),
        json!({"tool": "channel_post", "kind": "call", "args": {"message": "hi"}}),
        json!({"tool": "channel_post", "kind": "result", "content": [], "isError": false}),
        json!({"tool": "ask_questions", "kind": "call", "args": {"questions": [{"question": "A?", "options": []}, {"question": "B?", "options": []}]}}),
        json!({"tool": "ask_questions", "kind": "result", "content": [], "details": {"questions": [], "answers": {"Which?": "Alpha", "Other?": ""}, "cancelled": false}}),
        json!({"tool": "ask_questions", "kind": "result", "content": [], "details": {"questions": [], "answers": {}, "cancelled": true}}),
        json!({"tool": "ask_questions", "kind": "result", "content": [], "details": null}),
        json!({"tool": "ask_supervisor", "kind": "call", "args": {"question": "help"}}),
        json!({"tool": "ask_supervisor", "kind": "result", "content": [{"type": "text", "text": "advice"}], "isError": false}),
        json!({"tool": "list_sessions", "kind": "call", "args": {}}),
        json!({"tool": "list_sessions", "kind": "result", "content": [{"type": "text", "text": "line1\nline2"}], "isError": false}),
    ];
    let width = 80;
    let definitions = create_all_tool_definitions(&JsString::from("/tmp/pi-tool-renderers"), ToolsOptions::default());
    let theme = pi_coding_agent::modes::interactive::theme::with_theme(std::clone::Clone::clone);
    let mut renders = Vec::new();
    for case in &cases {
        let tool = case.get("tool").and_then(Value::as_str).expect("tool name");
        let definition = definitions
            .iter()
            .find(|(name, _)| name.as_str() == tool)
            .map(|(_, definition)| definition)
            .expect("definition");
        let context = ToolRenderContext {
            args: JsValue::from(case.get("args").cloned().unwrap_or(Value::Null)),
            tool_call_id: "call-1".to_owned(),
            invalidate: Arc::new(|| {}),
            last_component: None,
            call_component: None,
            state: pi_coding_agent::core::extensions::types::ToolRenderState::new(),
            cwd: JsString::from("/tmp/pi-tool-renderers"),
            execution_started: false,
            args_complete: true,
            is_partial: false,
            expanded: false,
            show_images: true,
            is_error: false,
        };
        let component = if case.get("kind").and_then(Value::as_str) == Some("call") {
            let renderer = definition.render_call.as_ref().expect("call renderer");
            renderer(&context.args, &theme, &context)
        } else {
            let renderer = definition.render_result.as_ref().expect("result renderer");
            let result = AgentToolResult {
                content: tool_content(case.get("content").unwrap_or(&Value::Null)),
                details: (case.get("details").cloned().filter(|details| !details.is_null())).map(JsValue::from),
                is_error: case.get("isError").and_then(Value::as_bool),
                terminate: None,
            };
            let options = ToolRenderResultOptions {
                expanded: false,
                is_partial: false,
            };
            renderer(&result, &options, &theme, &context)
        };
        let component = component.expect("renderer failure");
        renders.push(json!(component.borrow_mut().render(width)));
    }
    let request = json!({
        "id": 320,
        "module": "modes/interactive/components/tool-execution.ts",
        "operation": "toolRenderers",
        "cases": cases,
        "width": width,
    });
    vec![(request, json!(renders))]
}

fn footer_cwd_cases() -> Vec<(Value, Value)> {
    use pi_coding_agent::modes::interactive::components::footer::format_cwd_for_footer;
    let cwd_cases = [
        ("/home/tester", Some("/home/tester")),
        ("/home/tester/projects/pi", Some("/home/tester")),
        ("/home/tester", None),
        ("/opt/other", Some("/home/tester")),
        ("/opt/other", Some("")),
    ];
    let request = json!({
        "id": 200,
        "module": "modes/interactive/components/footer.ts",
        "operation": "footerCwd",
        "cwdCases": cwd_cases.iter().map(|(cwd, home)| json!({"cwd": cwd, "home": home})).collect::<Vec<_>>(),
    });
    let output = json!({
        "cwd": cwd_cases
            .iter()
            .map(|(cwd, home)| format_cwd_for_footer(&JsString::from(*cwd), home.map(JsString::from).as_ref()))
            .collect::<Vec<_>>(),
    });
    vec![(request, output)]
}

fn footer_model(id: &str, provider: &str, reasoning: bool, context_window: f64) -> Value {
    json!({
        "id": id,
        "name": id,
        "api": "faux",
        "provider": provider,
        "baseUrl": "",
        "reasoning": reasoning,
        "input": ["text"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": context_window,
        "maxTokens": 4096.0,
    })
}

fn footer_cases() -> Vec<(Value, Value)> {
    use pi_coding_agent::core::footer_data_provider::{ContextUsage, FooterDataProvider, FooterSessionOverride};
    use pi_coding_agent::modes::interactive::components::footer::FooterComponent;

    struct FooterVariant {
        id: u64,
        input: f64,
        output: f64,
        cache_read: f64,
        cache_write: f64,
        cost: f64,
        percent: Option<f64>,
    }
    let variants = vec![
        FooterVariant {
            id: 210,
            input: 10_000.0,
            output: 2_000.0,
            cache_read: 3_000.0,
            cache_write: 4_000.0,
            cost: 0.1234,
            percent: Some(25.0),
        },
        FooterVariant {
            id: 211,
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cost: 0.0,
            percent: Some(75.0),
        },
        FooterVariant {
            id: 212,
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cost: 0.0,
            percent: Some(95.0),
        },
        FooterVariant {
            id: 213,
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cost: 0.0,
            percent: None,
        },
    ];
    let mut cases = Vec::new();
    for (index, variant) in variants.into_iter().enumerate() {
        let FooterVariant {
            id,
            input,
            output,
            cache_read,
            cache_write,
            cost,
            percent,
        } = variant;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _runtime_guard = runtime.enter();
        let directory = tempfile::tempdir().expect("temp dir");
        let (session, entries, override_manager) = runtime.block_on(async {
            let fixture = crate::agent_session_fixture::fixture(
                directory.path(),
                pi_agent_core::harness::types::ThinkingLevel::High,
            )
            .await;
            let session = std::sync::Arc::new(fixture.session);
            let entries = {
                let mut manager = session.session_manager();
                manager
                    .set_session_name(Some(&JsString::from("Test Session")))
                    .expect("session name");
                let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
                assistant.usage.input = input;
                assistant.usage.output = output;
                assistant.usage.cache_read = cache_read;
                assistant.usage.cache_write = cache_write;
                assistant.usage.cost.total = cost;
                assistant.stop_reason = StopReason::Stop;
                manager
                    .append_message(pi_agent_core::agent::AgentMessage::Assistant(Box::new(assistant)))
                    .expect("assistant");
                manager
                    .append_message(pi_agent_core::agent::AgentMessage::ToolResult(
                        pi_ai::types::ToolResultMessage {
                            role: pi_ai::types::ToolResultRole::Tag,
                            tool_call_id: "tool".to_owned(),
                            tool_name: "read".to_owned(),
                            content: vec![],
                            details: None,
                            is_error: false,
                            timestamp: 0.0,
                        },
                    ))
                    .expect("tool result");
                serde_json::to_value(manager.get_entries()).expect("entries")
            };
            let override_dir = directory.path().join("override");
            std::fs::create_dir_all(&override_dir).expect("override dir");
            let override_fixture = crate::agent_session_fixture::fixture(
                &override_dir,
                pi_agent_core::harness::types::ThinkingLevel::High,
            )
            .await;
            let override_manager = override_fixture.session.session_manager_handle();
            {
                let mut manager = override_manager.lock().expect("override manager");
                manager
                    .set_session_name(Some(&JsString::from("Override Session")))
                    .expect("override name");
                let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
                assistant.usage.input = 7000.0;
                assistant.usage.output = 8000.0;
                assistant.usage.cache_read = 9000.0;
                assistant.usage.cache_write = 1000.0;
                assistant.usage.cost.total = 0.5;
                assistant.stop_reason = StopReason::Stop;
                manager
                    .append_message(pi_agent_core::agent::AgentMessage::Assistant(Box::new(assistant)))
                    .expect("override assistant");
            }
            (session, entries, override_manager)
        });

        let override_session = json!({
            "sessionName": "Override Session",
            "sessionCwd": "/override",
            "entries": [{
                "type": "message",
                "id": "override-entry",
                "parentId": null,
                "timestamp": "2026-01-01T00:00:00.000Z",
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "override"}],
                    "api": "faux",
                    "provider": "faux",
                    "model": "faux-1",
                    "usage": {
                        "input": 7000,
                        "output": 8000,
                        "cacheRead": 9000,
                        "cacheWrite": 1000,
                        "totalTokens": 25000,
                        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0.5}
                    },
                    "stopReason": "stop",
                    "timestamp": 0
                }
            }]
        });
        let model = footer_model("gpt-x", "openai", true, 200_000.0);
        let override_model: pi_ai::types::Model = serde_json::from_value(model.clone()).expect("model deserializes");
        let width = if index == 0 { 120 } else { 40 };
        let request = json!({
            "id": id,
            "module": "modes/interactive/components/footer.ts",
            "operation": "footer",
            "model": model,
            "thinkingLevel": "high",
            "sessionName": "Test Session",
            "sessionCwd": directory.path().display().to_string(),
            "entries": entries,
            "usingOAuth": false,
            "contextUsage": null,
            "gitBranch": null,
            "executableName": null,
            "statuses": {"goal": "working on the port"},
            "providerCount": 0,
            "autoCompact": index == 0,
            "override": {
                "cwd": "/repo",
                "model": model,
                "thinkingLevel": "high",
                "contextUsage": percent.map(|percent| json!({
                    "tokens": 50_000.0,
                    "contextWindow": 200_000.0,
                    "percent": percent
                }))
            },
            "overrideSession": if index == 0 { override_session } else { Value::Null },
            "width": width,
        });

        let footer_data = std::sync::Arc::new(FooterDataProvider::new(JsString::from(
            (directory.path()).to_str().expect("fixture path must be UTF-8"),
        )));
        footer_data.set_extension_status(
            &JsString::from("goal"),
            Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(
                JsString::from("working on the port"),
            )),
        );
        let mut footer = FooterComponent::new(session, footer_data);
        footer.set_auto_compact_enabled(index == 0);
        footer.set_session_override(FooterSessionOverride {
            cwd: JsString::from("/repo"),
            session_manager: (index == 0).then(|| {
                pi_coding_agent::core::readonly_session_manager::create_readonly_session_manager(override_manager)
            }),
            model: Some(override_model),
            thinking_level: JsString::from("high"),
            context_usage: percent.map(|percent| ContextUsage {
                tokens: Some(50_000.0),
                context_window: 200_000.0,
                percent: Some(percent),
            }),
        });
        cases.push((request, json!(footer.render(width))));
    }
    cases
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
