#[path = "../support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::Path;

use pi_coding_agent::core::permissions::policy::ApprovalPolicy;
use pi_coding_agent::core::permissions::presets::{ApprovalPresetName, SandboxProfileName};
use pi_coding_agent::core::settings_manager::{
    DefaultProjectTrust, DoubleEscapeAction, FullscreenExitOutput, MermaidRenderingMode, PackageSource,
    SettingsManager, SettingsManagerCreateOptions, SettingsScope, TreeFilterMode,
};
use pi_tui::ScrollViewScrollbar;
use pi_tui::tui::TuiMode;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Scenario {
    name: &'static str,
    global: Option<&'static str>,
    project: Option<&'static str>,
    steps: Vec<Value>,
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-settings-manager-oracle.ts",
        "packages/coding-agent/src/core/settings-manager.ts",
    );

    for (index, scenario) in scenarios().into_iter().enumerate() {
        run_scenario(&mut oracle, index, &scenario);
    }
}

fn run_scenario(oracle: &mut oracle::TsOracle, index: usize, scenario: &Scenario) {
    let rust_temp = TempDir::new().expect("temp dir");
    let rust_agent_dir = init_scenario_dir(rust_temp.path(), scenario);
    let rust_project_dir = rust_temp.path().join("project");

    let mut manager = SettingsManager::create(
        &rust_project_dir.to_string_lossy(),
        Some(&rust_agent_dir),
        SettingsManagerCreateOptions::default(),
    )
    .expect("settings manager");

    let mut snapshots = Vec::new();
    for step in &scenario.steps {
        let threw = apply_step(&mut manager, step, &rust_agent_dir, &rust_project_dir);
        snapshots.push(json!({
            "threw": threw,
            "snapshot": snapshot(&manager),
            "globalFile": read_optional(&rust_agent_dir.join("settings.json")),
            "projectFile": read_optional(&rust_project_dir.join(".pi").join("settings.json")),
        }));
    }
    let errors: Vec<Value> = manager
        .drain_errors()
        .into_iter()
        .map(|error| json!(error.scope.as_str()))
        .collect();
    let actual = json!({"snapshots": snapshots, "errors": errors});

    let oracle_temp = TempDir::new().expect("temp dir");
    let oracle_agent_dir = init_scenario_dir(oracle_temp.path(), scenario);
    let oracle_project_dir = oracle_temp.path().join("project");
    let response = oracle.request(&json!({
        "id": index,
        "agentDir": oracle_agent_dir.to_string_lossy(),
        "projectDir": oracle_project_dir.to_string_lossy(),
        "steps": scenario.steps,
    }));
    assert_eq!(response["ok"], true, "oracle failed for {}: {response}", scenario.name);
    for (step_index, step) in scenario.steps.iter().enumerate() {
        let mut differences = Vec::new();
        diff_values(
            &actual["snapshots"][step_index],
            &response["output"]["snapshots"][step_index],
            "",
            &mut differences,
        );
        assert!(
            differences.is_empty(),
            "{} step {step_index} ({}):\n{}",
            scenario.name,
            step["op"],
            differences.join("\n")
        );
    }
    assert_eq!(
        actual["errors"], response["output"]["errors"],
        "{} errors",
        scenario.name
    );
}

fn diff_values(left: &Value, right: &Value, path: &str, out: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                match right.get(key) {
                    Some(right_value) => diff_values(value, right_value, &format!("{path}.{key}"), out),
                    None => out.push(format!("{path}.{key}: missing on right")),
                }
            }
            for key in right.keys() {
                if !left.contains_key(key) {
                    out.push(format!("{path}.{key}: missing on left"));
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                diff_values(
                    left.get(index).unwrap_or(&Value::Null),
                    right.get(index).unwrap_or(&Value::Null),
                    &format!("{path}[{index}]"),
                    out,
                );
            }
        }
        _ if left != right => out.push(format!("{path}: {left} != {right}")),
        _ => {}
    }
}

fn init_scenario_dir(root: &Path, scenario: &Scenario) -> std::path::PathBuf {
    let agent_dir = root.join("agent");
    let project_dir = root.join("project");
    fs::create_dir_all(&agent_dir).unwrap();
    fs::create_dir_all(project_dir.join(".pi")).unwrap();
    if let Some(global) = scenario.global {
        fs::write(agent_dir.join("settings.json"), global).unwrap();
    }
    if let Some(project) = scenario.project {
        fs::write(project_dir.join(".pi").join("settings.json"), project).unwrap();
    }
    agent_dir
}

fn read_optional(path: &Path) -> Value {
    fs::read_to_string(path).map_or(Value::Null, Value::String)
}

fn apply_step(manager: &mut SettingsManager, step: &Value, agent_dir: &Path, project_dir: &Path) -> bool {
    let op = step["op"].as_str().expect("step op");
    let value = &step["value"];
    let scope = step
        .get("scope")
        .and_then(Value::as_str)
        .map_or(SettingsScope::Global, |scope| {
            if scope == "project" {
                SettingsScope::Project
            } else {
                SettingsScope::Global
            }
        });

    match op {
        "set_theme" => manager.set_theme(value.as_str().unwrap()),
        "set_default_provider" => manager.set_default_provider(value.as_str().unwrap()),
        "set_default_model" => manager.set_default_model(value.as_str().unwrap()),
        "set_default_model_and_provider" => {
            manager.set_default_model_and_provider(step["provider"].as_str().unwrap(), step["model"].as_str().unwrap())
        }
        "set_default_thinking_level" => {
            manager.set_default_thinking_level(parse_thinking_level(value.as_str().unwrap()))
        }
        "set_steering_mode" => manager.set_steering_mode(parse_queue_mode(value.as_str().unwrap())),
        "set_follow_up_mode" => manager.set_follow_up_mode(parse_queue_mode(value.as_str().unwrap())),
        "set_transport" => manager.set_transport(parse_transport(value.as_str().unwrap())),
        "set_approval_policy" => manager.set_approval_policy(ApprovalPolicy::parse(value.as_str().unwrap()).unwrap()),
        "set_approval_preset" => {
            let result =
                manager.set_approval_preset(ApprovalPresetName::parse(value.as_str().unwrap()).unwrap(), scope);
            return result.is_err();
        }
        "set_sandbox_profile" => {
            let result =
                manager.set_sandbox_profile(SandboxProfileName::parse(value.as_str().unwrap()).unwrap(), scope);
            return result.is_err();
        }
        "set_session_sandbox_profile" => {
            manager.set_session_sandbox_profile(value.as_str().and_then(SandboxProfileName::parse))
        }
        "set_project_trusted" => manager.set_project_trusted(value.as_bool().unwrap()),
        "set_compaction_enabled" => manager.set_compaction_enabled(value.as_bool().unwrap()),
        "set_retry_enabled" => manager.set_retry_enabled(value.as_bool().unwrap()),
        "set_http_idle_timeout_ms" => {
            let result = manager.set_http_idle_timeout_ms(value.as_f64().unwrap());
            return result.is_err();
        }
        "set_hide_thinking_block" => manager.set_hide_thinking_block(value.as_bool().unwrap()),
        "set_show_cache_miss_notices" => manager.set_show_cache_miss_notices(value.as_bool().unwrap()),
        "set_shell_path" => manager.set_shell_path(value.as_str()),
        "set_quiet_startup" => manager.set_quiet_startup(value.as_bool().unwrap()),
        "set_default_project_trust" => manager.set_default_project_trust(match value.as_str().unwrap() {
            "always" => DefaultProjectTrust::Always,
            "never" => DefaultProjectTrust::Never,
            _ => DefaultProjectTrust::Ask,
        }),
        "set_shell_command_prefix" => manager.set_shell_command_prefix(value.as_str()),
        "set_npm_command" => {
            let command = value.as_array().map(|values| {
                values
                    .iter()
                    .map(|value| value.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            });
            manager.set_npm_command(command.as_deref());
        }
        "set_collapse_changelog" => manager.set_collapse_changelog(value.as_bool().unwrap()),
        "set_enable_install_telemetry" => manager.set_enable_install_telemetry(value.as_bool().unwrap()),
        "set_packages" => manager.set_packages(parse_packages(value)),
        "set_project_packages" => {
            let result = manager.set_project_packages(parse_packages(value));
            return result.is_err();
        }
        "set_extension_paths" => manager.set_extension_paths(&parse_strings(value)),
        "set_project_extension_paths" => {
            let result = manager.set_project_extension_paths(&parse_strings(value));
            return result.is_err();
        }
        "set_skill_paths" => manager.set_skill_paths(&parse_strings(value)),
        "set_project_skill_paths" => {
            let result = manager.set_project_skill_paths(&parse_strings(value));
            return result.is_err();
        }
        "set_prompt_template_paths" => manager.set_prompt_template_paths(&parse_strings(value)),
        "set_project_prompt_template_paths" => {
            let result = manager.set_project_prompt_template_paths(&parse_strings(value));
            return result.is_err();
        }
        "set_theme_paths" => manager.set_theme_paths(&parse_strings(value)),
        "set_project_theme_paths" => {
            let result = manager.set_project_theme_paths(&parse_strings(value));
            return result.is_err();
        }
        "set_enable_skill_commands" => manager.set_enable_skill_commands(value.as_bool().unwrap()),
        "set_show_images" => manager.set_show_images(value.as_bool().unwrap()),
        "set_image_width_cells" => manager.set_image_width_cells(value.as_f64().unwrap()),
        "set_clear_on_shrink" => manager.set_clear_on_shrink(value.as_bool().unwrap()),
        "set_show_terminal_progress" => manager.set_show_terminal_progress(value.as_bool().unwrap()),
        "set_tui_mode" => manager.set_tui_mode(if value.as_str().unwrap() == "fullscreen" {
            TuiMode::Fullscreen
        } else {
            TuiMode::Regular
        }),
        "set_fullscreen_exit_output" => {
            manager.set_fullscreen_exit_output(if value.as_str().unwrap() == "resume-hint" {
                FullscreenExitOutput::ResumeHint
            } else {
                FullscreenExitOutput::Transcript
            })
        }
        "set_fullscreen_scrollbar" => manager.set_fullscreen_scrollbar(match value.as_str().unwrap() {
            "always" => ScrollViewScrollbar::Always,
            "hidden" => ScrollViewScrollbar::Hidden,
            _ => ScrollViewScrollbar::Auto,
        }),
        "set_image_auto_resize" => manager.set_image_auto_resize(value.as_bool().unwrap()),
        "set_block_images" => manager.set_block_images(value.as_bool().unwrap()),
        "set_enabled_models" => {
            let patterns = value.as_array().map(|_| parse_strings(value));
            let result = manager.set_enabled_models(patterns.as_deref(), scope);
            return result.is_err();
        }
        "set_double_escape_action" => manager.set_double_escape_action(match value.as_str().unwrap() {
            "fork" => DoubleEscapeAction::Fork,
            "none" => DoubleEscapeAction::None,
            _ => DoubleEscapeAction::Tree,
        }),
        "set_tree_filter_mode" => manager.set_tree_filter_mode(match value.as_str().unwrap() {
            "no-tools" => TreeFilterMode::NoTools,
            "user-only" => TreeFilterMode::UserOnly,
            "labeled-only" => TreeFilterMode::LabeledOnly,
            "all" => TreeFilterMode::All,
            _ => TreeFilterMode::Default,
        }),
        "set_show_hardware_cursor" => manager.set_show_hardware_cursor(value.as_bool().unwrap()),
        "set_editor_padding_x" => manager.set_editor_padding_x(value.as_f64().unwrap()),
        "set_output_pad" => manager.set_output_pad(value.as_u64().unwrap() as u8),
        "set_autocomplete_max_visible" => manager.set_autocomplete_max_visible(value.as_f64().unwrap()),
        "set_mermaid_rendering_mode" => manager.set_mermaid_rendering_mode(match value.as_str().unwrap() {
            "off" => MermaidRenderingMode::Off,
            "final" => MermaidRenderingMode::Final,
            _ => MermaidRenderingMode::Streaming,
        }),
        "set_warnings" => manager.set_warnings(value.as_object().expect("warnings object")),
        "apply_overrides" => manager.apply_overrides(value.as_object().expect("overrides object")),
        "reload" => manager.reload(),
        "flush" => manager.flush(),
        "external_edit" => {
            let path = if step["scope"].as_str() == Some("project") {
                project_dir.join(".pi").join("settings.json")
            } else {
                agent_dir.join("settings.json")
            };
            fs::write(path, step["content"].as_str().unwrap()).unwrap();
        }
        other => panic!("unknown step: {other}"),
    }
    false
}

fn parse_strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect()
}

fn parse_packages(value: &Value) -> Vec<PackageSource> {
    serde_json::from_value(value.clone()).expect("package sources")
}

fn parse_thinking_level(value: &str) -> pi_agent_core::harness::ThinkingLevel {
    match value {
        "off" => pi_agent_core::harness::ThinkingLevel::Off,
        "minimal" => pi_agent_core::harness::ThinkingLevel::Minimal,
        "low" => pi_agent_core::harness::ThinkingLevel::Low,
        "medium" => pi_agent_core::harness::ThinkingLevel::Medium,
        "high" => pi_agent_core::harness::ThinkingLevel::High,
        "xhigh" => pi_agent_core::harness::ThinkingLevel::XHigh,
        "max" => pi_agent_core::harness::ThinkingLevel::Max,
        _ => pi_agent_core::harness::ThinkingLevel::Ultra,
    }
}

fn parse_queue_mode(value: &str) -> pi_agent_core::harness::QueueMode {
    if value == "all" {
        pi_agent_core::harness::QueueMode::All
    } else {
        pi_agent_core::harness::QueueMode::OneAtATime
    }
}

fn parse_transport(value: &str) -> pi_ai::types::Transport {
    match value {
        "sse" => pi_ai::types::Transport::Sse,
        "websocket" => pi_ai::types::Transport::Websocket,
        "websocket-cached" => pi_ai::types::Transport::WebsocketCached,
        _ => pi_ai::types::Transport::Auto,
    }
}

fn snapshot(manager: &SettingsManager) -> Value {
    json!({
        "globalSettings": Value::Object(manager.get_global_settings()),
        "projectSettings": Value::Object(manager.get_project_settings()),
        "mergedSettings": Value::Object(manager.get_merged_settings()),
        "projectTrusted": manager.is_project_trusted(),
        "theme": option_str(manager.get_theme()),
        "themeSetting": option_str(manager.get_theme_setting()),
        "defaultProvider": option_str(manager.get_default_provider()),
        "defaultModel": option_str(manager.get_default_model()),
        "defaultThinkingLevel": manager.get_default_thinking_level().map(|level| level.as_str()),
        "steeringMode": manager.get_steering_mode().as_str(),
        "followUpMode": manager.get_follow_up_mode().as_str(),
        "transport": match manager.get_transport() {
            pi_ai::types::Transport::Sse => "sse",
            pi_ai::types::Transport::Websocket => "websocket",
            pi_ai::types::Transport::WebsocketCached => "websocket-cached",
            pi_ai::types::Transport::Auto => "auto",
        },
        "approvalPolicy": manager.get_approval_policy().as_str(),
        "approvalPreset": manager.get_approval_preset().as_str(),
        "sandboxProfile": manager.get_sandbox_profile().as_str(),
        "configuredSandboxProfile": manager.get_configured_sandbox_profile().as_str(),
        "explicitSandboxProfile": manager.get_explicit_sandbox_profile().map(|profile| profile.as_str()),
        "sessionSandboxProfile": manager.get_session_sandbox_profile().map(|profile| profile.as_str()),
        "compaction": {
            "enabled": manager.get_compaction_settings().enabled,
            "reserveTokens": number(manager.get_compaction_settings().reserve_tokens),
            "keepRecentTokens": number(manager.get_compaction_settings().keep_recent_tokens),
        },
        "branchSummary": {
            "reserveTokens": number(manager.get_branch_summary_settings().reserve_tokens),
            "skipPrompt": manager.get_branch_summary_settings().skip_prompt,
        },
        "retry": {
            "enabled": manager.get_retry_settings().enabled,
            "maxRetries": number(manager.get_retry_settings().max_retries),
            "baseDelayMs": number(manager.get_retry_settings().base_delay_ms),
        },
        "providerRetry": provider_retry_snapshot(manager),
        "httpIdleTimeoutMs": manager.get_http_idle_timeout_ms().map_or(Value::String("ERROR".to_owned()), |value| json!(value)),
        "websocketConnectTimeoutMs": manager.get_websocket_connect_timeout_ms().map_or(Value::String("ERROR".to_owned()), |value| json!(value)),
        "hideThinkingBlock": manager.get_hide_thinking_block(),
        "showCacheMissNotices": manager.get_show_cache_miss_notices(),
        "shellPath": option_str(manager.get_shell_path().as_deref()),
        "sessionDir": option_str(manager.get_session_dir().as_deref()),
        "quietStartup": manager.get_quiet_startup(),
        "defaultProjectTrust": match manager.get_default_project_trust() {
            DefaultProjectTrust::Ask => "ask",
            DefaultProjectTrust::Always => "always",
            DefaultProjectTrust::Never => "never",
        },
        "shellCommandPrefix": option_str(manager.get_shell_command_prefix()),
        "npmCommand": manager.get_npm_command(),
        "collapseChangelog": manager.get_collapse_changelog(),
        "enableInstallTelemetry": manager.get_enable_install_telemetry(),
        "enableAnalytics": manager.get_enable_analytics(),
        "hasTrackingId": manager.get_tracking_id().is_some(),
        "packages": manager.get_packages().map_or(Value::String("ERROR".to_owned()), |packages| serde_json::to_value(packages).unwrap()),
        "extensionPaths": manager.get_extension_paths(),
        "disabledExtensions": manager.get_disabled_extensions(),
        "skillPaths": manager.get_skill_paths(),
        "promptTemplatePaths": manager.get_prompt_template_paths(),
        "themePaths": manager.get_theme_paths(),
        "enableSkillCommands": manager.get_enable_skill_commands(),
        "thinkingBudgets": manager.get_thinking_budgets(),
        "showImages": manager.get_show_images(),
        "imageWidthCells": number(manager.get_image_width_cells()),
        "clearOnShrink": manager.get_clear_on_shrink(),
        "showTerminalProgress": manager.get_show_terminal_progress(),
        "tuiMode": match manager.get_tui_mode() { TuiMode::Fullscreen => "fullscreen", TuiMode::Regular => "regular" },
        "fullscreenExitOutput": match manager.get_fullscreen_exit_output() { FullscreenExitOutput::ResumeHint => "resume-hint", FullscreenExitOutput::Transcript => "transcript" },
        "fullscreenScrollbar": match manager.get_fullscreen_scrollbar() { ScrollViewScrollbar::Always => "always", ScrollViewScrollbar::Hidden => "hidden", ScrollViewScrollbar::Auto => "auto" },
        "imageAutoResize": manager.get_image_auto_resize(),
        "blockImages": manager.get_block_images(),
        "enabledModels": manager.get_enabled_models(),
        "doubleEscapeAction": match manager.get_double_escape_action() { DoubleEscapeAction::Fork => "fork", DoubleEscapeAction::Tree => "tree", DoubleEscapeAction::None => "none" },
        "treeFilterMode": match manager.get_tree_filter_mode() { TreeFilterMode::Default => "default", TreeFilterMode::NoTools => "no-tools", TreeFilterMode::UserOnly => "user-only", TreeFilterMode::LabeledOnly => "labeled-only", TreeFilterMode::All => "all" },
        "showHardwareCursor": manager.get_show_hardware_cursor(),
        "editorPaddingX": number(manager.get_editor_padding_x()),
        "outputPad": manager.get_output_pad(),
        "autocompleteMaxVisible": number(manager.get_autocomplete_max_visible()),
        "codeBlockIndent": manager.get_code_block_indent(),
        "mermaidRenderingMode": match manager.get_mermaid_rendering_mode() { MermaidRenderingMode::Off => "off", MermaidRenderingMode::Final => "final", MermaidRenderingMode::Streaming => "streaming" },
        "warnings": Value::Object(manager.get_warnings()),
        "permissionPromptTool": manager.get_permission_prompt_tool(),
        "agentProfiles": {
            "browser": manager.get_agent_profile("browser"),
            "explore": manager.get_agent_profile("explore"),
            "implement": manager.get_agent_profile("implement"),
            "verifier": manager.get_agent_profile("verifier"),
            "documentation-update": manager.get_agent_profile("documentation-update"),
            "reviewer": manager.get_agent_profile("reviewer"),
            "unknown": manager.get_agent_profile("unknown"),
            "empty": manager.get_agent_profile("   "),
        },
    })
}

fn provider_retry_snapshot(manager: &SettingsManager) -> Value {
    let settings = manager.get_provider_retry_settings();
    let mut object = serde_json::Map::new();
    if let Some(timeout_ms) = settings.timeout_ms {
        object.insert("timeoutMs".to_owned(), number(timeout_ms));
    }
    if let Some(max_retries) = settings.max_retries {
        object.insert("maxRetries".to_owned(), number(max_retries));
    }
    object.insert("maxRetryDelayMs".to_owned(), number(settings.max_retry_delay_ms));
    Value::Object(object)
}

fn option_str(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |value| Value::String(value.to_owned()))
}

/// JavaScript numbers are compared as integers when integral.
fn number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return json!(value as i64);
    }
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "defaults-and-global-writes",
            global: None,
            project: None,
            steps: vec![
                json!({"op": "set_theme", "value": "light"}),
                json!({"op": "set_default_thinking_level", "value": "high"}),
                json!({"op": "set_default_model", "value": "claude-sonnet"}),
                json!({"op": "set_default_provider", "value": "anthropic"}),
                json!({"op": "set_show_images", "value": false}),
                json!({"op": "set_mermaid_rendering_mode", "value": "final"}),
                json!({"op": "set_tui_mode", "value": "fullscreen"}),
                json!({"op": "flush"}),
            ],
        },
        Scenario {
            name: "migrations",
            global: Some(
                r#"{"queueMode":"all","websockets":true,"skills":{"enableSkillCommands":false,"customDirectories":["a"]},"approvalPreset":"llm-approved","retry":{"maxDelayMs":1234}}"#,
            ),
            project: Some(
                r#"{"skills":{"customDirectories":[]},"retry":{"provider":{"maxRetries":2},"maxDelayMs":5}}"#,
            ),
            steps: vec![json!({"op": "reload"}), json!({"op": "set_theme", "value": "dark"})],
        },
        Scenario {
            name: "external-edit-preservation",
            global: Some(r#"{"theme":"dark","packages":["npm:some-pkg"]}"#),
            project: Some(r#"{"extensions":["./old-extension.ts"],"prompts":["./old-prompt.md"]}"#),
            steps: vec![
                json!({"op": "external_edit", "scope": "global", "content": "{\n  \"theme\": \"dark\",\n  \"packages\": []\n}"}),
                json!({"op": "set_theme", "value": "light"}),
                json!({"op": "external_edit", "scope": "project", "content": "{\n  \"extensions\": [\"./old-extension.ts\"],\n  \"prompts\": [\"./new-prompt.md\"]\n}"}),
                json!({"op": "set_project_extension_paths", "value": ["./updated-extension.ts"]}),
                json!({"op": "flush"}),
            ],
        },
        Scenario {
            name: "project-trust",
            global: Some(r#"{"theme":"global","defaultProjectTrust":"always"}"#),
            project: Some(r#"{"theme":"project","defaultProjectTrust":"never","packages":["npm:existing"]}"#),
            steps: vec![
                json!({"op": "set_project_trusted", "value": false}),
                json!({"op": "set_project_extension_paths", "value": ["./blocked.ts"]}),
                json!({"op": "set_project_trusted", "value": true}),
                json!({"op": "set_project_packages", "value": ["npm:new"]}),
                json!({"op": "reload"}),
            ],
        },
        Scenario {
            name: "approval-sandbox",
            global: None,
            project: None,
            steps: vec![
                json!({"op": "set_approval_preset", "value": "llm-approved-ask"}),
                json!({"op": "set_sandbox_profile", "value": "read-only", "scope": "project"}),
                json!({"op": "set_approval_policy", "value": "auto-approve"}),
                json!({"op": "set_session_sandbox_profile", "value": "read-only"}),
                json!({"op": "apply_overrides", "value": {"sandboxProfile": "full-access"}}),
                json!({"op": "set_session_sandbox_profile", "value": null}),
            ],
        },
        Scenario {
            name: "invalid-stored-values",
            global: Some(
                r#"{"tuiMode":"other","outputPad":2,"markdown":{"mermaid":"sometimes","codeBlockIndent":"\t"},"httpIdleTimeoutMs":-1,"websocketConnectTimeoutMs":"disabled","fullscreenExitOutput":"nothing","fullscreenScrollbar":"sometimes","terminal":{"imageWidthCells":-5,"showImages":false},"editorPaddingX":9,"autocompleteMaxVisible":1,"enabledModels":[],"agents":{"explore":{"model":"faux/explore","thinkingLevel":"minimal"}},"thinkingBudgets":{"low":10},"warnings":{"anthropicExtraUsage":false},"permissionPromptTool":"mcp__x"}"#,
            ),
            project: None,
            steps: vec![json!({"op": "flush"})],
        },
        Scenario {
            name: "numeric-setters",
            global: None,
            project: None,
            steps: vec![
                json!({"op": "set_image_width_cells", "value": -5}),
                json!({"op": "set_image_width_cells", "value": 3.9}),
                json!({"op": "set_editor_padding_x", "value": 9}),
                json!({"op": "set_autocomplete_max_visible", "value": 1}),
                json!({"op": "set_output_pad", "value": 0}),
                json!({"op": "set_http_idle_timeout_ms", "value": 1500.5}),
                json!({"op": "set_show_hardware_cursor", "value": true}),
                json!({"op": "set_clear_on_shrink", "value": true}),
                json!({"op": "set_show_terminal_progress", "value": true}),
                json!({"op": "set_image_auto_resize", "value": false}),
                json!({"op": "set_block_images", "value": true}),
            ],
        },
        Scenario {
            name: "enabled-models-and-paths",
            global: Some(r#"{"enabledModels":["global/model"],"extensions":["/before.ts"]}"#),
            project: Some(r#"{"enabledModels":[]}"#),
            steps: vec![
                json!({"op": "set_enabled_models", "value": ["faux/faux-1", "faux/faux-2"], "scope": "project"}),
                json!({"op": "set_npm_command", "value": ["mise", "exec", "node@20", "--", "npm"]}),
                json!({"op": "set_shell_path", "value": "~/bin/shell"}),
                json!({"op": "set_shell_command_prefix", "value": "shopt -s expand_aliases"}),
                json!({"op": "apply_overrides", "value": {"sessionDir": "~/sessions"}}),
                json!({"op": "set_warnings", "value": {"anthropicExtraUsage": false}}),
                json!({"op": "set_packages", "value": ["npm:simple-pkg", {"source": "npm:filtered", "extensions": ["extensions/oracle.ts"], "skills": []}]}),
                json!({"op": "set_enabled_models", "value": null}),
            ],
        },
        Scenario {
            name: "text-and-toggle-setters",
            global: None,
            project: None,
            steps: vec![
                json!({"op": "set_steering_mode", "value": "all"}),
                json!({"op": "set_follow_up_mode", "value": "all"}),
                json!({"op": "set_transport", "value": "websocket-cached"}),
                json!({"op": "set_compaction_enabled", "value": false}),
                json!({"op": "set_retry_enabled", "value": false}),
                json!({"op": "set_hide_thinking_block", "value": true}),
                json!({"op": "set_show_cache_miss_notices", "value": true}),
                json!({"op": "set_quiet_startup", "value": true}),
                json!({"op": "set_default_project_trust", "value": "never"}),
                json!({"op": "set_collapse_changelog", "value": true}),
                json!({"op": "set_enable_install_telemetry", "value": false}),
                json!({"op": "set_enable_skill_commands", "value": false}),
                json!({"op": "set_fullscreen_exit_output", "value": "resume-hint"}),
                json!({"op": "set_fullscreen_scrollbar", "value": "hidden"}),
                json!({"op": "set_double_escape_action", "value": "fork"}),
                json!({"op": "set_tree_filter_mode", "value": "labeled-only"}),
                json!({"op": "set_default_model_and_provider", "provider": "openai", "model": "gpt-5.5"}),
            ],
        },
    ]
}
