#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;
#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_coding_agent::core::extensions::types::{
    CustomMessageContent, CustomMessageValue, MarkdownMessageType, MarkdownTransformContext, MarkdownTransformer,
    MessageRenderOptions,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::messages::{CompactionSummaryMessage, CompactionSummaryRole};
use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
use pi_coding_agent::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_coding_agent::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions, ConfigWriteScope, ScopedResolvedPaths,
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
use pi_coding_agent::modes::interactive::components::markdown_transform::create_markdown_transform;
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
use pi_tui::Component;
use serde_json::{Value, json};

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

#[test]
fn interactive_leaf_helpers_match_the_pinned_typescript_source() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (request, actual) in requests() {
        let expected = oracle.request(&request);
        assert_eq!(actual, expected["output"], "mismatch for {request}");
    }
}

fn requests() -> Vec<(Value, Value)> {
    let mut cases = Vec::new();
    cases.extend(elapsed_cases());
    cases.extend(visual_truncate_cases());
    cases.extend(markdown_transform_cases());
    cases.extend(key_hint_cases());
    cases.extend(model_search_cases());
    cases.extend(diff_cases());
    cases.extend(config_selector_cases());
    cases.extend(branch_summary_cases());
    cases.extend(compaction_summary_cases());
    cases.extend(assistant_message_cases());
    cases.extend(user_message_cases());
    cases.extend(bordered_loader_cases());
    cases.extend(status_indicator_cases());
    cases.extend(extension_input_cases());
    cases.extend(extension_editor_cases());
    cases.extend(first_time_setup_cases());
    cases.extend(custom_entry_cases());
    cases.extend(custom_message_cases());
    cases.extend(bash_execution_cases());
    cases.extend(tree_selector_cases());
    cases.extend(session_selector_cases());
    cases.extend(footer_helper_cases());
    cases.extend(footer_cases());
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
        .map(|(binding, description)| key_hint(binding, description))
        .collect();
    let rust_raw_hints: Vec<String> = [("↑↓", "navigate"), ("Tab", "scope")]
        .iter()
        .map(|(key, description)| raw_key_hint(key, description))
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
    let output: Vec<String> = cases.iter().map(|value| render_diff(value)).collect();
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
    let cwd = "/tmp/pi-config-oracle/project";
    let agent_dir = "/tmp/pi-config-oracle/agent";
    let resolved = ResolvedPaths {
        extensions: vec![ResolvedResource {
            path: format!("{package_root}/extensions/bar.ts"),
            enabled: true,
            metadata: PathMetadata {
                source: "npm:pi-tools".to_string(),
                scope: SourceScope::User,
                origin: MetadataOrigin::Package,
                base_dir: Some(package_root.to_string()),
            },
        }],
        ..Default::default()
    };
    let mut global_settings = Settings::new();
    global_settings.insert("packages".to_string(), json!(["npm:pi-tools"]));
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        global_settings,
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let mut selector = ConfigSelectorComponent::new(ConfigSelectorOptions {
        resolved_paths: ScopedResolvedPaths {
            global: resolved.clone(),
            project: resolved,
        },
        settings_manager: manager.clone(),
        cwd: cwd.to_string(),
        agent_dir: agent_dir.to_string(),
        on_close: Rc::new(|| {}),
        on_exit: Rc::new(|| {}),
        request_render: Rc::new(|| {}),
        terminal_height: Some(24),
        write_scope: ConfigWriteScope::Project,
        project_mode_available: true,
    });
    let inputs = [" ", " ", " "];
    let mut renders = Vec::new();
    for input in inputs {
        selector.get_resource_list_mut().handle_input(input);
        renders.push(selector.render(100));
    }
    let project_packages = manager
        .borrow()
        .get_project_settings()
        .get("packages")
        .cloned()
        .unwrap_or(Value::Null);
    vec![(
        json!({
            "id": 7,
            "module": "modes/interactive/components/config-selector.ts",
            "operation": "configSelector",
            "packageRoot": package_root,
            "cwd": cwd,
            "agentDir": agent_dir,
            "writeScope": "project",
            "inputs": inputs,
            "globalPackages": ["npm:pi-tools"],
            "width": 100,
        }),
        json!({
            "projectPackages": project_packages,
            "renders": renders,
        }),
    )]
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
                summary: "The branch covered **two** files.\n\n- one\n- two".to_owned(),
                from_id: "entry-1".to_owned(),
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
        summary: "The summary text.".to_owned(),
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
        timestamp: 1_700_000_000_000,
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
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
                serde_json::json!({"path": "file.txt"})
                    .as_object()
                    .cloned()
                    .expect("object"),
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
    aborted.error_message = Some("Request was aborted".to_owned());
    let mut error = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("partial"))],
        StopReason::Error,
    );
    error.error_message = Some("boom".to_owned());

    let mut cases = Vec::new();
    let variants: Vec<(AssistantMessage, bool, usize, usize, Vec<Value>)> = vec![
        (text_only, false, 1, 80, Vec::new()),
        (with_tool, false, 1, 60, Vec::new()),
        (thinking_and_text.clone(), false, 1, 80, Vec::new()),
        (thinking_and_text, true, 1, 80, Vec::new()),
        (length, true, 1, 80, Vec::new()),
        (aborted, false, 1, 80, Vec::new()),
        (error, false, 0, 80, Vec::new()),
    ];
    for (index, (message, hide_thinking, output_pad, width, transformers)) in variants.into_iter().enumerate() {
        let request = json!({
            "id": 40 + index,
            "module": "modes/interactive/components/assistant-message.ts",
            "operation": "assistantMessage",
            "message": serde_json::to_value(&message).expect("message serializes"),
            "hideThinkingBlock": hide_thinking,
            "hiddenThinkingLabel": "Thinking...",
            "outputPad": output_pad,
            "transformers": transformers,
            "width": width,
        });
        let mut component = AssistantMessageComponent::new(
            Some(message),
            hide_thinking,
            None,
            Some("Thinking...".to_owned()),
            output_pad,
            Vec::new(),
        );
        cases.push((request, json!(component.render(width))));
    }

    let transformed = assistant_fixture(
        vec![AssistantContent::Text(TextContent::new("mixed Case"))],
        StopReason::Stop,
    );
    let transformers = json!([{"kind": "upper"}, {"kind": "append", "text": "!"}]);
    let rust_transformers: Vec<MarkdownTransformer> = vec![
        Arc::new(|markdown: &str, _context| markdown.to_uppercase()),
        Arc::new(|markdown: &str, _context| format!("{markdown}!")),
    ];
    let request = json!({
        "id": 50,
        "module": "modes/interactive/components/assistant-message.ts",
        "operation": "assistantMessage",
        "message": serde_json::to_value(&transformed).expect("message serializes"),
        "hideThinkingBlock": false,
        "hiddenThinkingLabel": "Thinking...",
        "outputPad": 1,
        "transformers": transformers,
        "width": 80,
    });
    let mut component = AssistantMessageComponent::new(
        Some(transformed),
        false,
        None,
        Some("Thinking...".to_owned()),
        1,
        rust_transformers,
    );
    cases.push((request, json!(component.render(80))));
    cases
}

fn user_message_cases() -> Vec<(Value, Value)> {
    let variants = [
        ("hello", 1usize, 40usize, Vec::new()),
        ("# Heading\n\nSome **bold** text.", 1, 40, Vec::new()),
        ("1. first\n2. second", 0, 30, Vec::new()),
        ("mixed Case", 1, 80, vec![json!({"kind": "upper"})]),
        ("suffix", 1, 80, vec![json!({"kind": "append", "text": "!"})]),
    ];
    let mut cases = Vec::new();
    for (index, (text, output_pad, width, transformers)) in variants.into_iter().enumerate() {
        let request = json!({
            "id": 60 + index,
            "module": "modes/interactive/components/user-message.ts",
            "operation": "userMessage",
            "text": text,
            "outputPad": output_pad,
            "transformers": transformers,
            "width": width,
        });
        let rust_transformers: Vec<MarkdownTransformer> = transformers
            .iter()
            .map(|transformer| match transformer["kind"].as_str() {
                Some("upper") => {
                    Arc::new(|markdown: &str, _context: &MarkdownTransformContext| markdown.to_uppercase())
                        as MarkdownTransformer
                }
                Some("append") => {
                    let suffix = transformer["text"].as_str().unwrap_or_default().to_owned();
                    Arc::new(move |markdown: &str, _context: &MarkdownTransformContext| format!("{markdown}{suffix}"))
                        as MarkdownTransformer
                }
                other => panic!("unknown transformer {other:?}"),
            })
            .collect();
        let mut component = UserMessageComponent::new(text, None, output_pad, rust_transformers);
        cases.push((request, json!(component.render(width))));
    }
    cases
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

    let mut retry = RetryStatusIndicator::new(std::rc::Rc::new(|| {}), 1, 3, 2500);
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
            title,
            None,
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            ExtensionInputOptions::default(),
        );
        for data in &inputs {
            input.handle_input(data);
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
            title,
            prefill,
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            pi_tui::components::editor::EditorOptions::default(),
            Some("nano".to_owned()),
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
    fn update(&self, _value: &str) -> bool {
        true
    }
}

impl ExtensionEditorHost for TestEditorHost {
    fn suspend(&self) {}
    fn resume(&self) {}
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
            setup.handle_input(data);
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
                        format!("custom {} expanded={}", entry.custom_type, options.expanded),
                        0,
                        0,
                    )));
                Some(component)
            },
        );
        let entry = pi_coding_agent::core::session_manager::CustomEntry {
            custom_type: "test".to_owned(),
            data: None,
            id: "entry".to_owned(),
            parent_id: None,
            timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        };
        let mut component = CustomEntryComponent::new(entry, renderer);
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
        custom_type: "empty".to_owned(),
        data: None,
        id: "entry".to_owned(),
        parent_id: None,
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
    };
    let mut component = CustomEntryComponent::new(entry, renderer);
    cases.push((
        request,
        json!({"lines": component.render(40), "hasContent": component.has_content()}),
    ));
    cases
}

fn custom_message_value(content: CustomMessageContent) -> CustomMessageValue {
    CustomMessageValue {
        custom_type: "test".to_owned(),
        content,
        display: true,
        details: None,
        timestamp: 0.0,
    }
}

fn custom_message_cases() -> Vec<(Value, Value)> {
    let variants = [
        (json!("hello **world**"), false, false, 1usize, 1usize, 40usize),
        (
            json!([{"type": "text", "text": "first"}, {"type": "image", "data": "AA==", "mimeType": "image/png"}, {"type": "text", "text": "second"}]),
            false,
            false,
            1,
            1,
            50,
        ),
        (json!("custom"), true, true, 1, 0, 40),
    ];
    let mut cases = Vec::new();
    for (index, (content, custom_renderer, expanded, output_pad, final_output_pad, width)) in
        variants.into_iter().enumerate()
    {
        let request = json!({
            "id": 130 + index,
            "module": "modes/interactive/components/custom-message.ts",
            "operation": "customMessage",
            "customType": "test",
            "content": content,
            "customRenderer": custom_renderer,
            "expanded": expanded,
            "outputPad": output_pad,
            "finalOutputPad": final_output_pad,
            "width": width,
        });
        let rust_content = match &content {
            Value::String(text) => CustomMessageContent::Text(text.clone()),
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
                |_message: &CustomMessageValue,
                 options: &MessageRenderOptions,
                 _theme: &pi_coding_agent::modes::interactive::theme::Theme| {
                    let component: pi_tui::tui::ComponentRef =
                        std::rc::Rc::new(std::cell::RefCell::new(pi_tui::Text::new(
                            format!("custom pad={} expanded={}", options.output_pad, options.expanded),
                            options.output_pad as usize,
                            0,
                        )));
                    Some(component)
                },
            ) as pi_coding_agent::core::extensions::types::MessageRenderer
        });
        let mut component = CustomMessageComponent::new(custom_message_value(rust_content), renderer, None, output_pad);
        component.set_output_pad(final_output_pad);
        component.set_expanded(expanded);
        cases.push((request, json!(component.render(width))));
    }
    cases
}

fn bash_execution_cases() -> Vec<(Value, Value)> {
    type BashCase = (bool, Vec<&'static str>, bool, Option<i32>, bool, bool, usize);
    let variants: Vec<BashCase> = vec![
        (false, vec!["hello\n"], true, Some(0), false, false, 60),
        (true, vec!["hello\n"], true, Some(0), false, false, 60),
        (
            false,
            vec!["\u{1b}[31mred\u{1b}[0m\r\nline\r"],
            true,
            Some(3),
            false,
            false,
            60,
        ),
        (false, vec!["partial"], true, None, true, false, 60),
        (false, vec!["one\ntwo\n"], false, Some(0), false, false, 60),
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
            component.append_output(chunk);
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
    let ids: Vec<(String, usize)> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.entry.id().unwrap_or_default().to_owned(), index))
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
        id: id.to_owned(),
        parent_id: parent.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
        message: Box::new(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(content.to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 0,
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
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4", 0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        id: id.to_owned(),
        parent_id: parent.map(str::to_owned),
        timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
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
                    id: "model-1".to_owned(),
                    parent_id: Some("user-2".to_owned()),
                    timestamp: "2026-01-01T00:00:00.000Z".to_owned(),
                    provider: "anthropic".to_owned(),
                    model_id: "claude-sonnet-4".to_owned(),
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
            Some(leaf.to_owned()),
            24,
            std::rc::Rc::new(|_| {}),
            std::rc::Rc::new(|| {}),
            None,
            None,
            filter,
        );
        let copied_sink = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let sink = copied_sink.clone();
        selector.set_on_copy(Some(std::rc::Rc::new(move |text| {
            *sink.borrow_mut() = text;
        })));
        if let Some(labels) = request["labels"].as_object() {
            for (entry_id, label) in labels {
                selector
                    .tree_list()
                    .borrow_mut()
                    .update_node_label(entry_id, label.as_str(), None);
            }
        }

        let mut renders = vec![json!(selector.render(width))];
        let mut selected_ids = vec![json!(
            selector.tree_list().borrow().selected_entry_id().map(str::to_owned)
        )];
        for input in &inputs {
            selector.handle_input(input);
            renders.push(json!(selector.render(width)));
            selected_ids.push(json!(
                selector.tree_list().borrow().selected_entry_id().map(str::to_owned)
            ));
        }
        let copied = copied_sink.borrow().clone();
        let output = json!({"renders": renders, "selectedIds": selected_ids, "copied": copied});
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
        path: path.to_owned(),
        id: format!("id-{path}"),
        cwd: "/repo".to_owned(),
        name: name.map(str::to_owned),
        is_archived: archived.then_some(true),
        parent_session_path: None,
        created: now - modified_offset_ms,
        modified: now - modified_offset_ms,
        message_count: 4,
        first_message: first_message.to_owned(),
        all_messages_text: first_message.to_owned(),
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
        "allMessagesText": session.all_messages_text,
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
                None,
                None,
                Some(KeybindingsManager::new(Default::default(), None)),
                None,
                with_archived.then(|| loader(archived.clone())),
                current_file,
            );
            selector.load_current_sessions().await;

            let mut renders = vec![json!(selector.render(width))];
            let mut selected = vec![json!(
                selector.session_list().get_selected_session_path().map(str::to_owned)
            )];
            for input in &inputs {
                selector.handle_input(input);
                if selector.has_pending_action() {
                    selector.run_pending_action().await;
                }
                renders.push(json!(selector.render(width)));
                selected.push(json!(
                    selector.session_list().get_selected_session_path().map(str::to_owned)
                ));
            }
            json!({"renders": renders, "selected": selected})
        });
        cases.push((request, output));
    }
    cases
}

fn footer_helper_cases() -> Vec<(Value, Value)> {
    use pi_coding_agent::modes::interactive::components::footer::{format_cwd_for_footer, format_tokens};

    let tokens = [
        0.0,
        999.0,
        1000.0,
        1500.0,
        9999.0,
        10_000.0,
        12_345.0,
        999_999.0,
        1_000_000.0,
        1_500_000.0,
        9_999_999.0,
        10_000_000.0,
        12_345_678.0,
    ];
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
        "operation": "footerHelpers",
        "tokens": tokens,
        "cwdCases": cwd_cases.iter().map(|(cwd, home)| json!({"cwd": cwd, "home": home})).collect::<Vec<_>>(),
    });
    let output = json!({
        "tokens": tokens.iter().map(|count| format_tokens(*count)).collect::<Vec<_>>(),
        "cwd": cwd_cases
            .iter()
            .map(|(cwd, home)| format_cwd_for_footer(cwd, *home))
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

    let variants: Vec<(u64, f64, f64, f64, f64, f64)> = vec![
        (210, 10_000.0, 2_000.0, 3_000.0, 4_000.0, 0.1234),
        (211, 0.0, 0.0, 0.0, 0.0, 0.0),
    ];
    let mut cases = Vec::new();
    for (index, (id, input, output, cache_read, cache_write, cost)) in variants.into_iter().enumerate() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _runtime_guard = runtime.enter();
        let directory = tempfile::tempdir().expect("temp dir");
        let (session, entries) = runtime.block_on(async {
            let fixture = crate::agent_session_fixture::fixture(
                directory.path(),
                pi_agent_core::harness::types::ThinkingLevel::High,
            )
            .await;
            let session = std::sync::Arc::new(fixture.session);
            let entries = {
                let mut manager = session.session_manager();
                manager.append_session_info("Test Session").expect("session name");
                let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux-1", 0);
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
                            usage: Some(pi_ai::types::Usage {
                                input: 500.0,
                                output: 100.0,
                                ..Default::default()
                            }),
                            added_tool_names: None,
                            is_error: false,
                            timestamp: 0,
                        },
                    ))
                    .expect("tool result");
                serde_json::to_value(manager.get_entries()).expect("entries")
            };
            (session, entries)
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
            "usingSubscription": false,
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
                "contextUsage": {
                    "tokens": 50_000.0,
                    "contextWindow": 200_000.0,
                    "percent": 25.0
                }
            },
            "width": width,
        });

        let footer_data = std::sync::Arc::new(FooterDataProvider::new(directory.path()));
        footer_data.set_extension_status("goal", Some("working on the port"));
        let mut footer = FooterComponent::new(session, footer_data);
        footer.set_auto_compact_enabled(index == 0);
        footer.set_session_override(FooterSessionOverride {
            cwd: "/repo".to_owned(),
            session_manager: None,
            model: Some(override_model),
            thinking_level: "high".to_owned(),
            context_usage: Some(ContextUsage {
                tokens: Some(50_000.0),
                context_window: 200_000.0,
                percent: Some(25.0),
            }),
        });
        cases.push((request, json!(footer.render(width))));
    }
    cases
}
