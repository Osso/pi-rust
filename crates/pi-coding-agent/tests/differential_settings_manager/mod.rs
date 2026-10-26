use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsValue;
use std::fs;
use std::path::Path;

use pi_coding_agent::core::permissions::policy::ApprovalPolicy;
use pi_coding_agent::core::permissions::presets::{ApprovalPresetName, SandboxProfileName};
use pi_coding_agent::core::settings_manager::{
    DefaultProjectTrust, DoubleEscapeAction, PackageSource, SettingsManager, SettingsManagerCreateOptions,
    SettingsScope, TreeFilterMode,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Scenario {
    name: &'static str,
    global: Option<&'static str>,
    project: Option<&'static str>,
    steps: Vec<Value>,
}

#[test]
fn legacy_tui_mode_keys_remain_unknown_settings_on_load_and_save() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-settings-manager-oracle.ts",
        "packages/coding-agent/src/core/settings-manager.ts",
    );
    let scenario = Scenario {
        name: "legacy-tui-mode-is-unknown-data",
        global: Some(
            r#"{"tuiMode":"fullscreen","fullscreenExitOutput":"transcript","fullscreenScrollbar":"hidden","theme":"dark"}"#,
        ),
        project: Some(r#"{"tuiMode":{"retained":"unknown object"}}"#),
        steps: vec![
            json!({"op":"flush"}),
            json!({"op":"set_theme","value":"light"}),
            json!({"op":"external_edit","scope":"global","content":"{\"tuiMode\":17,\"fullscreenExitOutput\":null,\"fullscreenScrollbar\":[\"unknown\"],\"theme\":\"dark\"}"}),
            json!({"op":"reload"}),
            json!({"op":"set_theme","value":"light"}),
        ],
    };
    run_scenario(&mut oracle, 999, &scenario);
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
        &JsString::from(rust_project_dir.to_string_lossy().as_ref()),
        Some(&JsString::from(rust_agent_dir.to_string_lossy().as_ref())),
        SettingsManagerCreateOptions::default(),
    )
    .expect("settings manager");

    let mut snapshots = Vec::new();
    for step in &scenario.steps {
        let threw = apply_step(&mut manager, step, &rust_agent_dir, &rust_project_dir);
        manager.flush();
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
        "set_last_changelog_version" => manager.set_last_changelog_version(&JsString::from(value.as_str().unwrap())),
        "set_enable_analytics" => return manager.set_enable_analytics(value.as_bool().unwrap()).is_err(),
        "set_theme" => manager.set_theme(&JsString::from(value.as_str().unwrap())),
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
        "set_hide_tool_output" => manager.set_hide_tool_output(value.as_bool().unwrap()),
        "set_hide_coordination_messages" => manager.set_hide_coordination_messages(value.as_bool().unwrap()),
        "set_hide_thinking_block" => manager.set_hide_thinking_block(value.as_bool().unwrap()),
        "set_shell_path" => manager.set_shell_path(value.as_str().map(JsString::from).as_ref()),
        "set_quiet_startup" => manager.set_quiet_startup(value.as_bool().unwrap()),
        "set_default_project_trust" => manager.set_default_project_trust(match value.as_str().unwrap() {
            "always" => DefaultProjectTrust::Always,
            "never" => DefaultProjectTrust::Never,
            _ => DefaultProjectTrust::Ask,
        }),
        "set_shell_command_prefix" => manager.set_shell_command_prefix(value.as_str().map(JsString::from).as_ref()),
        "set_npm_command" => {
            let command = value.as_array().map(|values| {
                values
                    .iter()
                    .map(|value| JsString::from(value.as_str().unwrap()))
                    .collect::<Vec<_>>()
            });
            manager.set_npm_command(command.as_deref());
        }
        "set_collapse_changelog" => manager.set_collapse_changelog(value.as_bool().unwrap()),
        "set_enable_install_telemetry" => manager.set_enable_install_telemetry(value.as_bool().unwrap()),
        "set_packages" => manager.set_packages(parse_packages(value)),
        "roundtrip_packages" => manager.set_packages(manager.get_packages().unwrap()),
        "set_project_packages" => {
            let result = manager.set_project_packages(parse_packages(value));
            return result.is_err();
        }
        "set_extension_paths" => {
            manager.set_extension_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>())
        }
        "set_project_extension_paths" => {
            let result = manager
                .set_project_extension_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>());
            return result.is_err();
        }
        "set_skill_paths" => {
            manager.set_skill_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>())
        }
        "set_project_skill_paths" => {
            let result =
                manager.set_project_skill_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>());
            return result.is_err();
        }
        "set_prompt_template_paths" => {
            manager.set_prompt_template_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>())
        }
        "set_project_prompt_template_paths" => {
            let result = manager.set_project_prompt_template_paths(
                &parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>(),
            );
            return result.is_err();
        }
        "set_theme_paths" => {
            manager.set_theme_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>())
        }
        "set_project_theme_paths" => {
            let result =
                manager.set_project_theme_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>());
            return result.is_err();
        }
        "set_enable_skill_commands" => manager.set_enable_skill_commands(value.as_bool().unwrap()),
        "set_show_images" => manager.set_show_images(value.as_bool().unwrap()),
        "set_image_width_cells" => manager.set_image_width_cells(value.as_f64().unwrap()),
        "set_clear_on_shrink" => manager.set_clear_on_shrink(value.as_bool().unwrap()),
        "set_show_terminal_progress" => manager.set_show_terminal_progress(value.as_bool().unwrap()),
        "set_image_auto_resize" => manager.set_image_auto_resize(value.as_bool().unwrap()),
        "set_block_images" => manager.set_block_images(value.as_bool().unwrap()),
        "set_enabled_models" => {
            let patterns = value
                .as_array()
                .map(|_| parse_strings(value).into_iter().map(JsString::from).collect::<Vec<_>>());
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
        "set_warnings" => manager.set_warnings(
            &(value.as_object().expect("warnings object"))
                .iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect(),
        ),
        "apply_overrides" => manager.apply_overrides(
            &(value.as_object().expect("overrides object"))
                .iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect(),
        ),
        "reload" => manager.reload(),
        "flush" => manager.flush(),
        "project_write_then_revoke_same_turn" => {
            manager
                .set_project_extension_paths(&parse_strings(value).iter().map(JsString::from).collect::<Vec<_>>())
                .unwrap();
            manager.set_project_trusted(false);
            manager.flush();
        }
        "setter_external_write_same_turn" => {
            manager.set_theme(&JsString::from(value.as_str().unwrap()));
            fs::write(agent_dir.join("settings.json"), step["content"].as_str().unwrap()).unwrap();
            manager.flush();
        }
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
        "globalSettings": json!(JsValue::Object(manager.get_global_settings())),
        "projectSettings": json!(JsValue::Object(manager.get_project_settings())),
        "mergedSettings": json!(JsValue::Object(manager.get_merged_settings())),
        "projectTrusted": manager.is_project_trusted(),
        "lastChangelogVersion": option_str(manager.get_last_changelog_version()),
        "externalEditorCommand": manager.get_external_editor_command(),
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
        "compactionEnabled": manager.get_compaction_enabled(),
        "compactionReserveTokens": number(manager.get_compaction_reserve_tokens()),
        "compactionKeepRecentTokens": number(manager.get_compaction_keep_recent_tokens()),
        "branchSummarySkipPrompt": manager.get_branch_summary_skip_prompt(),
        "retryEnabled": manager.get_retry_enabled(),
        "compaction": manager.get_compaction_settings().map_or(json!("ERROR"), |view| {
            let mut output = json!({"enabled":view.enabled,"reserveTokens":number(view.reserve_tokens),"keepRecentTokens":number(view.keep_recent_tokens)});
            if let Some(percent) = view.threshold_percent { output["thresholdPercent"] = number(percent); }
            output
        }),
        "branchSummary": {
            "reserveTokens": number(manager.get_branch_summary_settings().reserve_tokens),
            "skipPrompt": manager.get_branch_summary_settings().skip_prompt,
        },
        "retry": manager.get_retry_settings().map_or(json!("ERROR"), |view| json!({"enabled":view.enabled,"maxRetries":number(view.max_retries),"baseDelayMs":number(view.base_delay_ms),"maxBackoffMs":number(view.max_backoff_ms)})),
        "providerRetry": provider_retry_snapshot(manager),
        "httpIdleTimeoutMs": manager.get_http_idle_timeout_ms().map_or(Value::String("ERROR".to_owned()), |value| json!(value)),
        "websocketConnectTimeoutMs": manager.get_websocket_connect_timeout_ms().map_or(Value::String("ERROR".to_owned()), |value| json!(value)),
        "hideThinkingBlock": manager.get_hide_thinking_block(),
        "hideToolOutput": manager.get_hide_tool_output(),
        "hideCoordinationMessages": manager.get_hide_coordination_messages(),
        "idleCompaction": manager.get_idle_compaction_enabled(),
        "shellPath": option_str(manager.get_shell_path().as_ref()),
        "sessionDir": option_str(manager.get_session_dir().as_ref()),
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
        "warnings": json!(JsValue::Object(manager.get_warnings())),
        "permissionPromptTool": manager.get_permission_prompt_tool(),
        "agentProfiles": {
            "browser": manager.get_agent_profile(&JsString::from("browser")),
            "explore": manager.get_agent_profile(&JsString::from("explore")),
            "implement": manager.get_agent_profile(&JsString::from("implement")),
            "verifier": manager.get_agent_profile(&JsString::from("verifier")),
            "documentation-update": manager.get_agent_profile(&JsString::from("documentation-update")),
            "reviewer": manager.get_agent_profile(&JsString::from("reviewer")),
            "unknown": manager.get_agent_profile(&JsString::from("unknown")),
            "empty": manager.get_agent_profile(&JsString::from("   ")),
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

fn option_str(value: Option<impl serde::Serialize>) -> Value {
    json!(value)
}

/// JavaScript numbers are compared as integers when integral.
fn number(value: f64) -> Value {
    pi_js_compat::number::js_number_value(value)
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "repin-validation-and-catalog",
            global: None,
            project: None,
            steps: vec![
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":-1},"compaction":{"thresholdPercent":0}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":0,"maxBackoffMs":0},"compaction":{"thresholdPercent":100,"idle":false}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":1.5,"maxBackoffMs":300000},"compaction":{"thresholdPercent":100.01}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":2147483647,"maxBackoffMs":2147483647},"compaction":{"thresholdPercent":0.01}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":2147483648_i64},"compaction":{"thresholdPercent":null}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":9007199254740992_i64,"maxBackoffMs":-1},"compaction":{"thresholdPercent":"50"}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":30,"maxBackoffMs":29}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":null,"maxBackoffMs":null}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":1,"maxBackoffMs":1.5}}}),
                json!({"op":"apply_overrides","value":{"retry":{"baseDelayMs":1,"maxBackoffMs":2147483648_i64}}}),
            ],
        },
        Scenario {
            name: "one-level-overlay",
            global: Some(
                r#"{"agents":{"implement":{"model":"global","tools":["read"]}},"retry":{"provider":{"timeoutMs":10,"maxRetries":2}},"permissionRules":{"allow":{"read":["global"]}},"extensions":["global"],"terminal":{"showImages":false}}"#,
            ),
            project: Some(
                r#"{"agents":{"implement":{"thinkingLevel":"low"}},"retry":{"provider":{"maxRetries":3}},"permissionRules":{"allow":{"write":["project"]}},"extensions":[],"terminal":null}"#,
            ),
            steps: vec![
                json!({"op":"flush"}),
                json!({"op":"apply_overrides","value":{"agents":{"implement":null},"retry":null,"permissionRules":{"allow":[]}}}),
            ],
        },
        Scenario {
            name: "remaining-resource-and-identity-setters",
            global: Some(r#"{"unknown":{"preserve":true},"externalEditor":"configured-editor"}"#),
            project: Some("{}"),
            steps: vec![
                json!({"op":"set_last_changelog_version","value":"0.80.3"}),
                json!({"op":"set_enable_analytics","value":false}),
                json!({"op":"set_extension_paths","value":["global-extension"]}),
                json!({"op":"set_project_extension_paths","value":["project-extension"]}),
                json!({"op":"set_skill_paths","value":["global-skill"]}),
                json!({"op":"set_project_skill_paths","value":["project-skill"]}),
                json!({"op":"set_prompt_template_paths","value":["global-prompt"]}),
                json!({"op":"set_project_prompt_template_paths","value":["project-prompt"]}),
                json!({"op":"set_theme_paths","value":["global-theme"]}),
                json!({"op":"set_project_theme_paths","value":["project-theme"]}),
                json!({"op":"set_shell_path","value":null}),
                json!({"op":"set_shell_command_prefix","value":null}),
                json!({"op":"set_npm_command","value":null}),
            ],
        },
        Scenario {
            name: "reload-errors-and-recovery",
            global: Some(r#"{"theme":"global"}"#),
            project: Some(r#"{"theme":"project"}"#),
            steps: vec![
                json!({"op":"external_edit","scope":"global","content":"invalid"}),
                json!({"op":"external_edit","scope":"project","content":"invalid"}),
                json!({"op":"reload"}),
                json!({"op":"set_theme","value":"blocked-global-write"}),
                json!({"op":"set_project_extension_paths","value":["blocked-project-write"]}),
                json!({"op":"external_edit","scope":"global","content":"{}"}),
                json!({"op":"external_edit","scope":"project","content":"{}"}),
                json!({"op":"reload"}),
                json!({"op":"set_theme","value":"recovered"}),
                json!({"op":"set_project_extension_paths","value":["recovered"]}),
            ],
        },
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
                json!({"op": "flush"}),
            ],
        },
        Scenario {
            name: "frozen-field-catalog",
            global: Some(
                r#"{"relay":{"peer":"fixture-only","remoteCommand":"never-run"},"lastChangelogVersion":"0.80.3","defaultProvider":"fake","defaultModel":"fake-model","defaultThinkingLevel":"ultra","defaultCodexFastMode":"priority","transport":"auto","steeringMode":"all","followUpMode":"all","theme":"catalog","compaction":{"thresholdPercent":50,"enabled":true,"reserveTokens":1000,"keepRecentTokens":2000,"idle":false,"unknownChild":9},"branchSummary":{"reserveTokens":123,"skipPrompt":true},"retry":{"enabled":true,"maxRetries":4,"baseDelayMs":10,"maxBackoffMs":100,"provider":{"timeoutMs":5,"maxRetries":6,"maxRetryDelayMs":7}},"hideThinkingBlock":true,"hideToolOutput":true,"hideCoordinationMessages":true,"externalEditor":"fixture-editor","shellPath":"~/literal","quietStartup":true,"defaultProjectTrust":"never","shellCommandPrefix":"never-executed","npmCommand":["never-executed"],"collapseChangelog":true,"enableInstallTelemetry":false,"enableAnalytics":true,"trackingId":"synthetic-tracking-id","packages":["npm:fixture"],"extensions":["fixture.ts"],"disabledExtensions":["disabled"],"skills":["fixture-skill"],"prompts":["fixture-prompt"],"themes":["fixture-theme"],"enableSkillCommands":false,"terminal":{"showImages":false,"imageWidthCells":42,"clearOnShrink":true,"showTerminalProgress":true},"images":{"autoResize":false,"blockImages":true},"enabledModels":["fixture/model"],"doubleEscapeAction":"none","treeFilterMode":"all","thinkingBudgets":{"minimal":1,"low":2,"medium":3,"high":4},"editorPaddingX":2,"outputPad":0,"autocompleteMaxVisible":10,"showHardwareCursor":true,"markdown":{"codeBlockIndent":"    "},"warnings":{"anthropicExtraUsage":false},"agents":{"implement":{"context":"fresh","model":"fake/fixture","thinkingLevel":"high","tools":["read"]}},"sessionDir":"relative-fixture-dir","httpProxy":"http://invalid.invalid:1","httpIdleTimeoutMs":0,"websocketConnectTimeoutMs":0,"permissionPromptTool":"never-called","permissionRules":{"allow":{"read":["fixture"]}},"approvalPolicy":"on-request","approvalPreset":"llm-approved-ask","sandboxProfile":"read-only","unknownRoot":{"keep":[1,2]},"tuiMode":"fullscreen","fullscreenExitOutput":"resume-hint","fullscreenScrollbar":"hidden","showCacheMissNotices":true}"#,
            ),
            project: Some("{}"),
            steps: vec![
                json!({"op":"flush"}),
                json!({"op":"set_compaction_enabled","value":false}),
                json!({"op":"set_theme","value":"persisted-catalog"}),
            ],
        },
        Scenario {
            name: "migration-precedence",
            global: Some(
                r#"{"queueMode":"all","steeringMode":"one-at-a-time","websockets":false,"transport":"auto","skills":{"enableSkillCommands":true,"customDirectories":[]},"enableSkillCommands":false,"retry":{"maxDelayMs":10,"provider":{"maxRetryDelayMs":99,"unknownChild":1}}}"#,
            ),
            project: Some(
                r#"{"retry":{"maxDelayMs":20,"provider":{"maxRetryDelayMs":null}},"skills":{"customDirectories":["project-skill"]}}"#,
            ),
            steps: vec![
                json!({"op":"flush"}),
                json!({"op":"set_theme","value":"migrated"}),
                json!({"op":"set_project_theme_paths","value":["migrated"]}),
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
                json!({"op": "set_hide_tool_output", "value": true}),
                json!({"op": "set_hide_coordination_messages", "value": true}),
                json!({"op": "set_quiet_startup", "value": true}),
                json!({"op": "set_default_project_trust", "value": "never"}),
                json!({"op": "set_collapse_changelog", "value": true}),
                json!({"op": "set_enable_install_telemetry", "value": false}),
                json!({"op": "set_enable_skill_commands", "value": false}),
                json!({"op": "set_double_escape_action", "value": "fork"}),
                json!({"op": "set_tree_filter_mode", "value": "labeled-only"}),
                json!({"op": "set_default_model_and_provider", "provider": "openai", "model": "gpt-5.5"}),
            ],
        },
    ]
}

/// TS fixture producer writes fields without product setters (unknowns, agents and provider settings).
/// Every known mutation below uses the actual manager API, in both runtimes.
pub(crate) fn bidirectional() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-settings-manager-oracle.ts",
        "packages/coding-agent/src/core/settings-manager.ts",
    );
    let temp = TempDir::new().unwrap();
    let agent = temp.path().join("agent");
    let project = temp.path().join("project");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(project.join(".pi")).unwrap();
    let global = json!({"packages":[{"skills":["skill"],"unknown":{"keep":true},"source":"npm:original"}],"unknownRoot":{"producer":"TS"},"agents":{"implement":{"model":"global","tools":["read"]}},"retry":{"provider":{"timeoutMs":100,"maxRetries":2},"sentinel":{"ts":true}},"compaction":{"thresholdPercent":70,"idle":false,"unknownChild":{"ts":1}},"terminal":{"unknownChild":{"ts":2}},"permissionRules":{"allow":{"read":["global"]}},"relay":{"peer":"isolated"},"reminders":{"deliverySessionId":"global-session","globalExtra":true}});
    let local = json!({"agents":{"implement":{"thinkingLevel":"low"}},"retry":{"provider":{"maxRetries":3}},"permissionRules":{"allow":{"write":["project"]}},"unknownProject":{"producer":"TS"},"reminders":{"deliverySessionId":"project-session","projectExtra":true}});
    let ask = |oracle: &mut oracle::TsOracle, steps: Vec<Value>| {
        let response = oracle.request(&json!({"id":"bidirectional","managerKey":"shared-fixture","agentDir":agent,"projectDir":project,"steps":steps}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["output"]["errors"], json!([]), "{response}");
        response["output"]["snapshots"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone()
    };
    let ts = ask(
        &mut oracle,
        vec![
            json!({"op":"external_edit","scope":"global","content":global.to_string()}),
            json!({"op":"external_edit","scope":"project","content":local.to_string()}),
            json!({"op":"reload"}),
            json!({"op":"set_theme","value":"TS-created"}),
            json!({"op":"roundtrip_packages"}),
            json!({"op":"set_show_images","value":false}),
            json!({"op":"set_project_skill_paths","value":["TS-skill"]}),
        ],
    );
    let mut rust = SettingsManager::create(
        &JsString::from(project.to_string_lossy().as_ref()),
        Some(&JsString::from(agent.to_string_lossy().as_ref())),
        SettingsManagerCreateOptions::default(),
    )
    .unwrap();
    assert_eq!(snapshot(&rust), ts["snapshot"]);
    assert_eq!(
        rust.get_reminders_delivery_session_id(),
        Some(&JsString::from("project-session"))
    );
    assert_eq!(
        rust.get_merged_settings()["reminders"]["globalExtra"],
        JsValue::from(json!(true))
    );
    rust.set_packages(rust.get_packages().unwrap());
    rust.flush();
    assert_eq!(
        fs::read_to_string(agent.join("settings.json")).unwrap(),
        ts["globalFile"].as_str().unwrap()
    );
    let ts_roundtrip = ask(
        &mut oracle,
        vec![json!({"op":"roundtrip_packages"}), json!({"op":"reload"})],
    );
    assert_eq!(ts_roundtrip["globalFile"], ts["globalFile"]);
    assert_eq!(rust.get_provider_retry_settings().timeout_ms, None);
    assert_eq!(rust.get_compaction_settings().unwrap().threshold_percent, Some(70.0));
    assert!(!rust.get_idle_compaction_enabled());
    assert_eq!(
        rust.get_merged_settings()["permissionRules"]["allow"],
        json!({"write":["project"]}).into()
    );

    // Two stale product managers plus a fixture-only independent writer. None may lose siblings.
    let mut disk: Value = serde_json::from_str(&fs::read_to_string(agent.join("settings.json")).unwrap()).unwrap();
    disk["outOfBand"] = json!({"producer":"fixture-only"});
    disk["compaction"]["outOfBand"] = json!(17);
    disk["terminal"]["outOfBand"] = json!({"keep":true});
    fs::write(agent.join("settings.json"), disk.to_string()).unwrap();
    rust.set_compaction_enabled(false);
    rust.set_hide_tool_output(true);
    rust.set_hide_coordination_messages(true);
    rust.set_shell_path(Some(&JsString::from("~/literal/../shell")));
    rust.set_project_extension_paths(&["Rust-extension".to_owned().into()])
        .unwrap();
    rust.flush();
    let ts = ask(
        &mut oracle,
        vec![
            json!({"op":"set_show_terminal_progress","value":true}),
            json!({"op":"set_project_prompt_template_paths","value":["TS-prompt"]}),
            json!({"op":"reload"}),
        ],
    );
    let saved: Value = serde_json::from_str(ts["globalFile"].as_str().unwrap()).unwrap();
    assert_eq!(saved["unknownRoot"], global["unknownRoot"]);
    assert_eq!(saved["relay"], global["relay"]);
    assert_eq!(saved["reminders"], global["reminders"]);
    assert_eq!(saved["outOfBand"], json!({"producer":"fixture-only"}));
    assert_eq!(
        saved["compaction"],
        json!({"thresholdPercent":70,"idle":false,"unknownChild":{"ts":1},"outOfBand":17,"enabled":false})
    );
    assert_eq!(
        saved["terminal"],
        json!({"unknownChild":{"ts":2},"outOfBand":{"keep":true},"showImages":false,"showTerminalProgress":true})
    );
    assert_eq!(saved["hideToolOutput"], true);
    assert_eq!(saved["hideCoordinationMessages"], true);
    assert_eq!(saved["shellPath"], "~/literal/../shell");
    assert_eq!(saved["retry"], global["retry"]);
    let saved_project: Value = serde_json::from_str(ts["projectFile"].as_str().unwrap()).unwrap();
    assert_eq!(saved_project["unknownProject"], local["unknownProject"]);
    assert_eq!(saved_project["reminders"], local["reminders"]);
    assert_eq!(saved_project["extensions"], json!(["Rust-extension"]));
    assert_eq!(saved_project["skills"], json!(["TS-skill"]));
    assert_eq!(saved_project["prompts"], json!(["TS-prompt"]));
    rust.reload();
    assert_eq!(snapshot(&rust), ts["snapshot"]);

    // Reverse: a stale Rust manager writes after the TS product writer; nested siblings survive.
    let ts = ask(
        &mut oracle,
        vec![
            json!({"op":"set_hide_tool_output","value":false}),
            json!({"op":"set_theme","value":"TS-reverse"}),
            json!({"op":"set_block_images","value":true}),
        ],
    );
    assert_eq!(ts["snapshot"]["hideToolOutput"], false);
    rust.set_show_images(true);
    rust.set_retry_enabled(false);
    rust.flush();
    let ts = ask(&mut oracle, vec![json!({"op":"reload"})]);
    rust.reload();
    assert_eq!(snapshot(&rust), ts["snapshot"]);
    assert_eq!(rust.get_theme(), Some(&JsString::from("TS-reverse")));
    assert!(!rust.get_hide_tool_output());
    assert!(rust.get_show_images());
    assert!(rust.get_block_images());
    assert!(!rust.get_retry_enabled());
    let final_global: Value = serde_json::from_str(ts["globalFile"].as_str().unwrap()).unwrap();
    assert_eq!(final_global["retry"]["sentinel"], global["retry"]["sentinel"]);
    assert_eq!(final_global["compaction"], saved["compaction"]);
    assert_eq!(final_global["terminal"]["outOfBand"], saved["terminal"]["outOfBand"]);

    // Real process overlap, not only stale sequential handles: independent children of terminal.
    std::thread::scope(|scope| {
        let ts_writer = scope.spawn(|| {
            let mut steps = vec![json!({"op":"fixture_barrier"})];
            steps.extend((0..40).map(|index| json!({"op":"set_show_terminal_progress","value":index == 39})));
            ask(&mut oracle, steps)
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !agent.join("barrier-ready").exists() {
            assert!(std::time::Instant::now() < deadline, "TS writer never reached barrier");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        fs::write(agent.join("barrier-release"), "release").unwrap();
        for index in 0..40 {
            rust.set_show_images(index == 39);
        }
        ts_writer.join().unwrap();
    });
    let ts = ask(&mut oracle, vec![json!({"op":"reload"})]);
    rust.reload();
    assert_eq!(snapshot(&rust), ts["snapshot"]);
    assert!(rust.get_show_images());
    assert!(rust.get_show_terminal_progress());
    let concurrent: Value = serde_json::from_str(ts["globalFile"].as_str().unwrap()).unwrap();
    assert_eq!(
        concurrent["terminal"]["unknownChild"],
        saved["terminal"]["unknownChild"]
    );
    assert_eq!(concurrent["terminal"]["outOfBand"], saved["terminal"]["outOfBand"]);
    assert!(rust.drain_errors().is_empty());
}

pub(crate) fn queued_write_ordering() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-settings-manager-oracle.ts",
        "packages/coding-agent/src/core/settings-manager.ts",
    );
    run_scenario(
        &mut oracle,
        0,
        &Scenario {
            name: "queued-save-before-external-write-before-flush",
            global: Some(r#"{"theme":"initial","unmodified":"initial"}"#),
            project: None,
            steps: vec![
                json!({"op":"setter_external_write_same_turn","value":"setter-wins","content":r#"{"theme":"external","unmodified":"competing"}"#}),
            ],
        },
    );
    run_scenario(
        &mut oracle,
        1,
        &Scenario {
            name: "queued-project-write-rechecks-trust",
            global: None,
            project: Some(r#"{"extensions":["original"]}"#),
            steps: vec![json!({"op":"project_write_then_revoke_same_turn","value":["must-not-persist"]})],
        },
    );
    run_scenario(
        &mut oracle,
        2,
        &Scenario {
            name: "queued-write-parse-error-is-drained",
            global: Some(r#"{"theme":"original"}"#),
            project: None,
            steps: vec![json!({"op":"setter_external_write_same_turn","value":"must-not-persist","content":"invalid"})],
        },
    );
}

pub(crate) fn threshold_helper() {
    use pi_coding_agent::core::compaction_threshold::parse_compaction_threshold_percent;
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-compaction-threshold-oracle.ts",
        "packages/coding-agent/src/core/compaction-threshold.ts",
    );
    let mut values = vec![None];
    values.extend(
        [
            json!(0.00001),
            json!(100),
            json!(70),
            json!(0),
            json!(-1),
            json!(100.00001),
            json!(null),
            json!("70"),
            json!(false),
            json!([]),
            json!([1, null, 2]),
            json!({}),
            json!({"toString":1}),
        ]
        .into_iter()
        .map(Some),
    );
    for (index, value) in values.iter().enumerate() {
        let response = oracle.request(&json!({"id":index,"missing":value.is_none(),"value":value}));
        assert_eq!(response["ok"], true, "{response}");
        let actual = match parse_compaction_threshold_percent(value.as_ref().map(JsValue::from).as_ref()) {
            Ok(result) => json!({"result":result.map(number)}),
            Err(error) => json!({"error":error.message()}),
        };
        assert_eq!(actual, response["output"], "input {value:?}");
    }
}
