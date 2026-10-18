#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{MarkdownMessageType, MarkdownTransformer};
use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions, ConfigWriteScope, ScopedResolvedPaths,
};
use pi_coding_agent::modes::interactive::components::diff::render_diff;
use pi_coding_agent::modes::interactive::components::elapsed_time::format_elapsed_duration;
use pi_coding_agent::modes::interactive::components::keybinding_hints::{
    KeyTextFormatOptions, format_key_text, key_display_text, key_hint, key_text, raw_key_hint,
};
use pi_coding_agent::modes::interactive::components::markdown_transform::create_markdown_transform;
use pi_coding_agent::modes::interactive::components::visual_truncate::truncate_to_visual_lines;
use pi_coding_agent::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
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
