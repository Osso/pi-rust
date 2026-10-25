//! Differential test: theme JSON loading, validation, and ANSI resolution
//! against the pinned TypeScript module through `oracle/coding-agent-theme-oracle.ts`.

use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};

use oracle::TsOracle;
use pi_coding_agent::modes::interactive::theme::{
    ColorMode, TerminalTheme, ThemeBg, ThemeColor, get_language_from_path, get_theme_for_rgb_color,
    load_theme_from_path, resolve_theme_setting,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const SOURCE_DIRECTORY: &str = "packages/coding-agent/src/modes/interactive/theme/theme.ts";

const THEME_COLORS: &[(&str, ThemeColor)] = &[
    ("accent", ThemeColor::Accent),
    ("border", ThemeColor::Border),
    ("borderAccent", ThemeColor::BorderAccent),
    ("borderMuted", ThemeColor::BorderMuted),
    ("success", ThemeColor::Success),
    ("error", ThemeColor::Error),
    ("warning", ThemeColor::Warning),
    ("muted", ThemeColor::Muted),
    ("dim", ThemeColor::Dim),
    ("text", ThemeColor::Text),
    ("thinkingText", ThemeColor::ThinkingText),
    ("userMessageText", ThemeColor::UserMessageText),
    ("customMessageText", ThemeColor::CustomMessageText),
    ("customMessageLabel", ThemeColor::CustomMessageLabel),
    ("toolTitle", ThemeColor::ToolTitle),
    ("toolOutput", ThemeColor::ToolOutput),
    ("mdHeading", ThemeColor::MdHeading),
    ("mdLink", ThemeColor::MdLink),
    ("mdLinkUrl", ThemeColor::MdLinkUrl),
    ("mdCode", ThemeColor::MdCode),
    ("mdCodeBlock", ThemeColor::MdCodeBlock),
    ("mdCodeBlockBorder", ThemeColor::MdCodeBlockBorder),
    ("mdQuote", ThemeColor::MdQuote),
    ("mdQuoteBorder", ThemeColor::MdQuoteBorder),
    ("mdHr", ThemeColor::MdHr),
    ("mdListBullet", ThemeColor::MdListBullet),
    ("toolDiffAdded", ThemeColor::ToolDiffAdded),
    ("toolDiffRemoved", ThemeColor::ToolDiffRemoved),
    ("toolDiffContext", ThemeColor::ToolDiffContext),
    ("syntaxComment", ThemeColor::SyntaxComment),
    ("syntaxKeyword", ThemeColor::SyntaxKeyword),
    ("syntaxFunction", ThemeColor::SyntaxFunction),
    ("syntaxVariable", ThemeColor::SyntaxVariable),
    ("syntaxString", ThemeColor::SyntaxString),
    ("syntaxNumber", ThemeColor::SyntaxNumber),
    ("syntaxType", ThemeColor::SyntaxType),
    ("syntaxOperator", ThemeColor::SyntaxOperator),
    ("syntaxPunctuation", ThemeColor::SyntaxPunctuation),
    ("thinkingOff", ThemeColor::ThinkingOff),
    ("thinkingMinimal", ThemeColor::ThinkingMinimal),
    ("thinkingLow", ThemeColor::ThinkingLow),
    ("thinkingMedium", ThemeColor::ThinkingMedium),
    ("thinkingHigh", ThemeColor::ThinkingHigh),
    ("thinkingXhigh", ThemeColor::ThinkingXhigh),
    ("bashMode", ThemeColor::BashMode),
];

const THEME_BACKGROUNDS: &[(&str, ThemeBg)] = &[
    ("selectedBg", ThemeBg::SelectedBg),
    ("userMessageBg", ThemeBg::UserMessageBg),
    ("customMessageBg", ThemeBg::CustomMessageBg),
    ("toolPendingBg", ThemeBg::ToolPendingBg),
    ("toolSuccessBg", ThemeBg::ToolSuccessBg),
    ("toolErrorBg", ThemeBg::ToolErrorBg),
];

fn required_colors() -> Value {
    json!({
        "accent": "#8abeb7",
        "border": "#5f87ff",
        "borderAccent": "#00d7ff",
        "borderMuted": "#505050",
        "success": "#b5bd68",
        "error": "#cc6666",
        "warning": "#ffff00",
        "muted": "#808080",
        "dim": "#666666",
        "text": "#d4d4d4",
        "thinkingText": "#808080",
        "selectedBg": "#3a3a4a",
        "userMessageBg": "#343541",
        "userMessageText": "#d4d4d4",
        "customMessageBg": "#2d2838",
        "customMessageText": "#d4d4d4",
        "customMessageLabel": "#9575cd",
        "toolPendingBg": "#282832",
        "toolSuccessBg": "#283228",
        "toolErrorBg": "#3c2828",
        "toolTitle": "#d4d4d4",
        "toolOutput": "#808080",
        "mdHeading": "#f0c674",
        "mdLink": "#81a2be",
        "mdLinkUrl": "#666666",
        "mdCode": "#8abeb7",
        "mdCodeBlock": "#b5bd68",
        "mdCodeBlockBorder": "#808080",
        "mdQuote": "#808080",
        "mdQuoteBorder": "#808080",
        "mdHr": "#808080",
        "mdListBullet": "#8abeb7",
        "toolDiffAdded": "#b5bd68",
        "toolDiffRemoved": "#cc6666",
        "toolDiffContext": "#808080",
        "syntaxComment": "#6A9955",
        "syntaxKeyword": "#569CD6",
        "syntaxFunction": "#DCDCAA",
        "syntaxVariable": "#9CDCFE",
        "syntaxString": "#CE9178",
        "syntaxNumber": "#B5CEA8",
        "syntaxType": "#4EC9B0",
        "syntaxOperator": "#D4D4D4",
        "syntaxPunctuation": "#D4D4D4",
        "thinkingOff": "#505050",
        "thinkingMinimal": "#6e6e6e",
        "thinkingLow": "#5f87af",
        "thinkingMedium": "#81a2be",
        "thinkingHigh": "#b294bb",
        "thinkingXhigh": "#d183e8",
        "bashMode": "#b5bd68"
    })
}

fn theme_case(name: &str, content: &str, mode: &str) -> Value {
    json!({"name": name, "content": content, "mode": mode})
}

fn load_cases(package_dir: &Path) -> Vec<Value> {
    let dark = fs::read_to_string(package_dir.join("theme/dark.json")).expect("dark theme");
    let light = fs::read_to_string(package_dir.join("theme/light.json")).expect("light theme");

    let mut full = json!({"name": "full-vars", "vars": {
        "cyan": "#00d7ff",
        "alias": "cyan",
        "number": 24,
        "empty": "",
        "upper": "#AABBCC"
    }, "colors": required_colors()});
    full["colors"]["accent"] = json!("alias");
    full["colors"]["border"] = json!("number");
    full["colors"]["text"] = json!("empty");
    full["colors"]["mdHeading"] = json!("upper");
    full["colors"]["selectedBg"] = json!(0);
    full["export"] = json!({"pageBg": "#112233", "cardBg": 24, "infoBg": ""});

    let mut fallbacks = json!({"name": "fallbacks", "colors": required_colors()});
    fallbacks["colors"]["thinkingMax"] = Value::Null;
    fallbacks["colors"].as_object_mut().unwrap().remove("thinkingMax");

    let mut missing = json!({"name": "missing", "colors": required_colors()});
    for key in ["accent", "bashMode"] {
        missing["colors"].as_object_mut().unwrap().remove(key);
    }

    let mut bad_types = json!({"name": "bad-types", "colors": required_colors()});
    bad_types["colors"]["accent"] = json!(true);
    bad_types["colors"]["border"] = json!(null);
    bad_types["colors"]["success"] = json!(300);
    bad_types["colors"]["error"] = json!(-1);
    bad_types["colors"]["warning"] = json!(1.5);

    let mut bad_hex = json!({"name": "bad-hex", "colors": required_colors()});
    bad_hex["colors"]["extraColor"] = json!("#fff");

    let mut circular = json!({"name": "circular", "vars": {"a": "b", "b": "a"}, "colors": required_colors()});
    circular["colors"]["accent"] = json!("a");

    let mut missing_var = json!({"name": "missing-var", "colors": required_colors()});
    missing_var["colors"]["accent"] = json!("doesnotexist");

    let slash = json!({"name": "a/b", "colors": required_colors()});

    let mut no_name = json!({"colors": required_colors()});
    no_name.as_object_mut().unwrap().remove("name");

    let mut no_colors = json!({"name": "no-colors"});
    no_colors.as_object_mut().unwrap().remove("colors");

    let non_object_colors = json!({"name": "colors-string", "colors": "nope"});

    let bad_vars = json!({"name": "bad-vars", "vars": {"a": true}, "colors": required_colors()});

    let bad_export = json!({"name": "bad-export", "colors": required_colors(), "export": {"pageBg": true}});

    let non_object_export = json!({"name": "export-string", "colors": required_colors(), "export": 5});

    let mut no_name_with_bad_colors = json!({"colors": {"accent": true}});
    no_name_with_bad_colors.as_object_mut().unwrap().remove("name");

    let mut many_bad = json!({"name": "many-bad", "colors": required_colors()});
    for key in [
        "accent", "border", "success", "error", "warning", "muted", "dim", "text",
    ] {
        many_bad["colors"][key] = json!(true);
    }

    let mut required_truncated = json!({"$schema": 5, "vars": {"a": true, "b": false}, "colors": required_colors()});
    for key in ["accent", "border", "success"] {
        required_truncated["colors"][key] = json!(null);
    }
    required_truncated.as_object_mut().unwrap().remove("name");

    vec![
        theme_case("many-bad", &many_bad.to_string(), "truecolor"),
        theme_case("required-truncated", &required_truncated.to_string(), "truecolor"),
        theme_case("dark", &dark, "truecolor"),
        theme_case("dark256", &dark, "256color"),
        theme_case("light", &light, "truecolor"),
        theme_case("light256", &light, "256color"),
        theme_case("full-vars", &full.to_string(), "truecolor"),
        theme_case("full-vars-256", &full.to_string(), "256color"),
        theme_case("fallbacks", &fallbacks.to_string(), "truecolor"),
        theme_case("missing", &missing.to_string(), "truecolor"),
        theme_case("bad-types", &bad_types.to_string(), "truecolor"),
        theme_case("bad-hex", &bad_hex.to_string(), "truecolor"),
        theme_case("circular", &circular.to_string(), "truecolor"),
        theme_case("missing-var", &missing_var.to_string(), "truecolor"),
        theme_case("slash", &slash.to_string(), "truecolor"),
        theme_case("no-name", &no_name.to_string(), "truecolor"),
        theme_case("no-colors", &no_colors.to_string(), "truecolor"),
        theme_case("colors-string", &non_object_colors.to_string(), "truecolor"),
        theme_case("bad-vars", &bad_vars.to_string(), "truecolor"),
        theme_case("bad-export", &bad_export.to_string(), "truecolor"),
        theme_case("export-string", &non_object_export.to_string(), "truecolor"),
        theme_case("no-name-bad-colors", &no_name_with_bad_colors.to_string(), "truecolor"),
        theme_case("root-array", "[]", "truecolor"),
        theme_case("root-string", "\"nope\"", "truecolor"),
        theme_case("invalid-json", "{ not json", "truecolor"),
        theme_case("truncated-json", "{\"name\": \"x\"", "truecolor"),
        theme_case("trailing-json", "[]x", "truecolor"),
        theme_case("empty-json", "", "truecolor"),
    ]
}

/// Substitute each side's temp directory prefix so only the message body is compared.
fn normalize_temp_path(value: Value, pattern: &str, replacement: &str) -> Value {
    match value {
        Value::String(text) => Value::String(
            regex::Regex::new(pattern)
                .expect("temp path pattern")
                .replace_all(&text, replacement)
                .into_owned(),
        ),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| normalize_temp_path(item, pattern, replacement))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, normalize_temp_path(item, pattern, replacement)))
                .collect(),
        ),
        other => other,
    }
}

fn rust_load_theme(directory: &Path, case: &Value) -> Value {
    let name = case["name"].as_str().unwrap();
    let content = case["content"].as_str().unwrap();
    let mode = match case["mode"].as_str().unwrap() {
        "truecolor" => ColorMode::Truecolor,
        _ => ColorMode::Color256,
    };
    let path = directory.join(format!("{name}.json"));
    fs::write(&path, content).expect("write differential theme");
    match load_theme_from_path(
        &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
        Some(mode),
    ) {
        Ok(theme) => json!({
            "name": name,
            "ok": true,
            "theme": {
                "name": theme.name,
                "colorMode": match theme.get_color_mode() {
                    ColorMode::Truecolor => "truecolor",
                    ColorMode::Color256 => "256color",
                },
                "thinkingBorders": ([
                    ("xhigh", pi_agent_core::ThinkingLevel::XHigh),
                    ("max", pi_agent_core::ThinkingLevel::Max),
                    ("ultra", pi_agent_core::ThinkingLevel::Ultra),
                ].into_iter().map(|(level, value)| {
                    let style = std::sync::Arc::new(theme.clone()).get_thinking_border_color(value);
                    json!([level, style(&JsString::from("border"))])
                }).collect::<Vec<_>>()),
                "fg": THEME_COLORS
                    .iter()
                    .map(|(key, color)| json!([key, theme.get_fg_ansi(*color)]))
                    .collect::<Vec<_>>(),
                "bg": THEME_BACKGROUNDS
                    .iter()
                    .map(|(key, background)| json!([key, theme.get_bg_ansi(*background)]))
                    .collect::<Vec<_>>(),
            },
        }),
        Err(error) => json!({
            "name": name,
            "ok": false,
            "error": {"name": "Error", "message": error.to_string()},
        }),
    }
}

#[test]
fn theme_loading_matches_typescript() {
    let mut oracle = TsOracle::spawn("coding-agent-theme-oracle.ts", SOURCE_DIRECTORY);
    let package_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cases = load_cases(&package_dir);
    let response = oracle.request(&json!({"id": 1, "operation": "load_theme", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let directory: TempDir = tempfile::tempdir().expect("theme differential dir");
    let actual: Vec<Value> = cases
        .iter()
        .map(|case| rust_load_theme(directory.path(), case))
        .collect();

    assert_eq!(actual.len(), expected.len());
    let actual_prefix = format!("{}/", directory.path().display());
    // The oracle uses mkdtemp(join(os.tmpdir(), "pi-theme-oracle-")).
    let oracle_prefix = std::env::temp_dir().join("pi-theme-oracle-");
    let oracle_pattern = format!(r"{}[^/]+/", regex::escape(&oracle_prefix.to_string_lossy()));
    let expected = expected
        .into_iter()
        .map(|case| normalize_temp_path(case, &oracle_pattern, "<tmp>/"))
        .collect::<Vec<_>>();
    let actual = actual
        .into_iter()
        .map(|case| normalize_temp_path(case, &regex::escape(&actual_prefix), "<tmp>/"))
        .collect::<Vec<_>>();
    for (index, (actual_case, expected_case)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_case, expected_case,
            "theme case {} ({}) diverged",
            index, expected_case["name"]
        );
    }
}

#[test]
fn theme_helpers_match_typescript() {
    let mut oracle = TsOracle::spawn("coding-agent-theme-oracle.ts", SOURCE_DIRECTORY);
    let helper_cases = vec![
        json!({"colorfgbg": "0;15"}),
        json!({"colorfgbg": "15;0"}),
        json!({"colorfgbg": "0;7;15"}),
        json!({"colorfgbg": ""}),
        json!({"colorfgbg": "bogus"}),
        json!({"colorfgbg": "12;9"}),
        json!({"colorfgbg": "0;300"}),
        json!({"colorfgbg": " 15 ; 2 "}),
        json!({"rgb": {"r": 0, "g": 0, "b": 0}}),
        json!({"rgb": {"r": 8, "g": 8, "b": 8}}),
        json!({"rgb": {"r": 250, "g": 250, "b": 250}}),
        json!({"rgb": {"r": 128, "g": 128, "b": 128}}),
        json!({"rgb": {"r": 255, "g": 0, "b": 0}}),
        json!({"themeSetting": "light/dark", "terminalTheme": "light"}),
        json!({"themeSetting": "light/dark", "terminalTheme": "dark"}),
        json!({"themeSetting": "light/dark/extra", "terminalTheme": "dark"}),
        json!({"themeSetting": "dark", "terminalTheme": "light"}),
        json!({"themeSetting": "", "terminalTheme": "dark"}),
        json!({"themeSetting": " light / dark ", "terminalTheme": "light"}),
        json!({"themeSetting": null, "terminalTheme": "light"}),
        json!({"path": "src/main.rs"}),
        json!({"path": "src/app.TSX"}),
        json!({"path": "Makefile"}),
        json!({"path": "Dockerfile"}),
        json!({"path": "notes.unknown"}),
        json!({"path": "no-extension"}),
        json!({"path": "trailing."}),
        json!({"path": "a.b.c"}),
    ];
    let response = oracle.request(&json!({"id": 2, "operation": "helpers", "cases": helper_cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let actual: Vec<Value> = helper_cases.iter().map(rust_helper).collect();
    for (index, (actual_case, expected_case)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(actual_case, expected_case, "helper case {index} diverged");
    }
}

fn rust_helper(case: &Value) -> Value {
    let mut result = serde_json::Map::new();
    if let Some(colorfgbg) = case.get("colorfgbg").and_then(Value::as_str) {
        let mut env = std::collections::BTreeMap::new();
        env.insert("COLORFGBG".to_owned(), colorfgbg.to_owned());
        let detection = pi_coding_agent::modes::interactive::theme::detect_terminal_background_from_env(&env);
        result.insert(
            "detection".to_owned(),
            json!({
                "theme": detection.theme.as_str(),
                "source": detection.source.as_str(),
                "detail": detection.detail,
                "confidence": detection.confidence.as_str(),
            }),
        );
    }
    if let Some(rgb) = case.get("rgb") {
        let rgb = pi_tui::terminal_colors::RgbColor {
            r: rgb["r"].as_f64().unwrap(),
            g: rgb["g"].as_f64().unwrap(),
            b: rgb["b"].as_f64().unwrap(),
        };
        result.insert("theme".to_owned(), json!(get_theme_for_rgb_color(rgb).as_str()));
    }
    if let Some(terminal_theme) = case.get("terminalTheme").and_then(Value::as_str) {
        let terminal_theme = match terminal_theme {
            "light" => TerminalTheme::Light,
            _ => TerminalTheme::Dark,
        };
        let setting = case.get("themeSetting").and_then(Value::as_str);
        result.insert(
            "resolved".to_owned(),
            match resolve_theme_setting(setting.map(JsString::from).as_ref(), terminal_theme) {
                Some(theme) => json!(theme),
                None => Value::Null,
            },
        );
    }
    if let Some(path) = case.get("path").and_then(Value::as_str) {
        result.insert(
            "language".to_owned(),
            match get_language_from_path(&JsString::from(path)) {
                Some(language) => json!(language),
                None => Value::Null,
            },
        );
    }
    Value::Object(result)
}
