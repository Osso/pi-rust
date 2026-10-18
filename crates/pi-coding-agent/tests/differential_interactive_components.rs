#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_coding_agent::core::extensions::types::{MarkdownMessageType, MarkdownTransformContext, MarkdownTransformer};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::messages::{CompactionSummaryMessage, CompactionSummaryRole};
use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::assistant_message::AssistantMessageComponent;
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_coding_agent::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions, ConfigWriteScope, ScopedResolvedPaths,
};
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
