use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::permissions::presets::{ApprovalPresetName, SandboxProfileName};
use pi_coding_agent::core::settings_manager::{
    DefaultProjectTrust, DoubleEscapeAction, Settings, SettingsManager, SettingsManagerCreateOptions, SettingsScope,
    TreeFilterMode,
};
use serde_json::{Value, json};
use tempfile::TempDir;

struct Workspace {
    _temp: TempDir,
    agent_dir: PathBuf,
    project_dir: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let agent_dir = temp.path().join("agent");
        let project_dir = temp.path().join("project");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(project_dir.join(".pi")).unwrap();
        Self {
            _temp: temp,
            agent_dir,
            project_dir,
        }
    }

    fn manager(&self) -> SettingsManager {
        SettingsManager::create(
            &JsString::from(self.project_dir.to_string_lossy().as_ref()),
            Some(&JsString::from(self.agent_dir.to_string_lossy().as_ref())),
            SettingsManagerCreateOptions::default(),
        )
        .unwrap()
    }

    fn untrusted(&self) -> SettingsManager {
        SettingsManager::create(
            &JsString::from(self.project_dir.to_string_lossy().as_ref()),
            Some(&JsString::from(self.agent_dir.to_string_lossy().as_ref())),
            SettingsManagerCreateOptions {
                project_trusted: Some(false),
            },
        )
        .unwrap()
    }

    fn write_global(&self, content: &str) {
        fs::write(self.agent_dir.join("settings.json"), content).unwrap();
    }

    fn write_project(&self, content: &str) {
        fs::write(self.project_dir.join(".pi").join("settings.json"), content).unwrap();
    }

    fn global(&self) -> Value {
        read_json(&self.agent_dir.join("settings.json"))
    }

    fn project(&self) -> Value {
        read_json(&self.project_dir.join(".pi").join("settings.json"))
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn settings(value: Value) -> Settings {
    JsValue::from(value).as_object().unwrap().clone()
}

#[test]
fn mermaid_mode_defaults_merges_and_persists_without_losing_siblings() {
    use pi_coding_agent::core::settings_manager::MermaidRenderingMode::{Final, Off, Streaming};
    let workspace = Workspace::new();
    assert_eq!(workspace.manager().get_mermaid_rendering_mode(), Streaming);
    workspace.write_global(r#"{"markdown":{"mermaid":"off","codeBlockIndent":"--","extra":1}}"#);
    workspace.write_project(r#"{"markdown":{"mermaid":"final","projectExtra":2}}"#);
    let mut manager = workspace.manager();
    assert_eq!(manager.get_mermaid_rendering_mode(), Final);
    assert_eq!(workspace.untrusted().get_mermaid_rendering_mode(), Off);
    manager.set_mermaid_rendering_mode(Streaming);
    manager.flush();
    assert_eq!(
        workspace.global()["markdown"],
        json!({"mermaid":"streaming","codeBlockIndent":"--","extra":1})
    );
    assert_eq!(manager.get_mermaid_rendering_mode(), Final);
    workspace.write_project(r#"{"markdown":{"mermaid":"invalid"}}"#);
    assert_eq!(workspace.manager().get_mermaid_rendering_mode(), Streaming);
}

#[test]
fn historical_mermaid_settings_are_bidirectionally_compatible() {
    use pi_coding_agent::core::settings_manager::MermaidRenderingMode::{Final, Off};
    let workspace = Workspace::new();
    workspace.write_global(r#"{"markdown":{"mermaid":"off","codeBlockIndent":"--","extra":1},"unknown":true}"#);
    let invoke = |action: &str| -> Value {
        let output = std::process::Command::new("/home/osso/.worktrees/pi-rust-source-532f7d12/node_modules/.bin/tsx")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../oracle/historical-mermaid-settings.ts"
            ))
            .arg(&workspace.project_dir)
            .arg(&workspace.agent_dir)
            .arg(action)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    };
    assert_eq!(invoke("write-final"), json!({"mode":"final","indent":"--"}));
    let mut rust = workspace.manager();
    assert_eq!(rust.get_mermaid_rendering_mode(), Final);
    assert_eq!(
        fs::read_to_string(workspace.agent_dir.join("settings.json")).unwrap(),
        "{\n  \"markdown\": {\n    \"mermaid\": \"final\",\n    \"codeBlockIndent\": \"--\",\n    \"extra\": 1\n  },\n  \"unknown\": true\n}"
    );
    rust.set_mermaid_rendering_mode(Off);
    rust.flush();
    assert_eq!(invoke("read"), json!({"mode":"off","indent":"--"}));
    assert_eq!(
        fs::read_to_string(workspace.agent_dir.join("settings.json")).unwrap(),
        "{\n  \"markdown\": {\n    \"mermaid\": \"off\",\n    \"codeBlockIndent\": \"--\",\n    \"extra\": 1\n  },\n  \"unknown\": true\n}"
    );
}

#[test]
fn reminder_delivery_session_uses_merged_settings_and_preserves_utf16() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"reminders":{"deliverySessionId":"global","extra":1},"unknown":true}"#);
    workspace.write_project(r#"{"reminders":{"deliverySessionId":"\ud800","projectExtra":2}}"#);
    let mut manager = workspace.manager();

    let expected = pi_js_compat::js_json::parse_lossless(r#""\ud800""#).unwrap();
    assert_eq!(manager.get_reminders_delivery_session_id(), expected.as_string());
    let merged = manager.get_merged_settings();
    assert_eq!(merged["reminders"]["extra"], JsValue::from(json!(1)));
    assert_eq!(merged["reminders"]["projectExtra"], JsValue::from(json!(2)));
    assert_eq!(
        workspace.untrusted().get_reminders_delivery_session_id(),
        Some(&JsString::from("global"))
    );

    manager.set_theme(&JsString::from("light"));
    manager.flush();
    assert_eq!(
        workspace.global()["reminders"],
        json!({"deliverySessionId":"global","extra":1})
    );
    assert_eq!(workspace.global()["unknown"], true);
}

#[test]
fn preserves_external_settings_added_after_startup() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"dark","defaultModel":"claude-sonnet"}"#);
    let mut manager = workspace.manager();

    workspace.write_global(r#"{"theme":"dark","defaultModel":"claude-sonnet","enabledModels":["claude-opus-4-5"]}"#);
    manager.set_default_thinking_level(pi_agent_core::harness::ThinkingLevel::High);
    manager.flush();

    let saved = workspace.global();
    assert_eq!(saved["enabledModels"], json!(["claude-opus-4-5"]));
    assert_eq!(saved["defaultThinkingLevel"], "high");
    assert_eq!(saved["theme"], "dark");
    assert_eq!(saved["defaultModel"], "claude-sonnet");
}

#[test]
fn in_memory_changes_override_file_changes_for_the_same_key() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"dark"}"#);
    let mut manager = workspace.manager();

    workspace.write_global(r#"{"theme":"dark","defaultThinkingLevel":"low"}"#);
    manager.set_default_thinking_level(pi_agent_core::harness::ThinkingLevel::High);
    manager.flush();

    assert_eq!(workspace.global()["defaultThinkingLevel"], "high");
}

#[test]
fn retry_defaults_match_the_source() {
    let manager = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    let retry = manager.get_retry_settings().unwrap();
    assert!(retry.enabled);
    assert_eq!(retry.max_retries, 30.0);
    assert_eq!(retry.base_delay_ms, 30_000.0);
    assert_eq!(manager.get_provider_retry_settings().max_retry_delay_ms, 60_000.0);
}

#[test]
fn project_enabled_models_override_global() {
    let mut storage = pi_coding_agent::core::settings_manager::InMemorySettingsStorage::new();
    storage.set(
        SettingsScope::Global,
        Some(r#"{"enabledModels":["global/model"]}"#.to_owned()),
    );
    storage.set(SettingsScope::Project, Some(r#"{"enabledModels":[]}"#.to_owned()));
    let mut manager = SettingsManager::from_storage(Box::new(storage), SettingsManagerCreateOptions::default());

    assert_eq!(manager.get_enabled_models(), Some(Vec::new()));
    manager
        .set_enabled_models(Some(&[JsString::from("faux/faux-1")]), SettingsScope::Project)
        .unwrap();
    assert_eq!(manager.get_enabled_models(), Some(vec![JsString::from("faux/faux-1")]));
}

#[test]
fn agent_profiles_use_builtin_defaults_and_overrides() {
    let manager = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    let explore = manager.get_agent_profile(&JsString::from("explore")).unwrap();
    assert_eq!(explore["model"], JsValue::String("openai/gpt-5-mini".into()));
    assert_eq!(explore["thinkingLevel"], JsValue::String("low".into()));
    let browser = manager.get_agent_profile(&JsString::from("browser")).unwrap();
    assert_eq!(
        browser["tools"],
        JsValue::Array(vec![JsValue::String("browser-cli".into())])
    );
    assert!(manager.get_agent_profile(&JsString::from("unknown")).is_none());
    assert!(manager.get_agent_profile(&JsString::from("   ")).is_none());

    let manager = SettingsManager::in_memory(
        settings(json!({"agents": {"explore": {"model": "faux/explore-model", "thinkingLevel": "minimal"}}})),
        SettingsManagerCreateOptions::default(),
    );
    let explore = manager.get_agent_profile(&JsString::from("explore")).unwrap();
    assert_eq!(explore["model"], JsValue::String("faux/explore-model".into()));
}

#[test]
fn approval_policy_defaults_and_persists_presets() {
    let workspace = Workspace::new();
    let mut manager = workspace.manager();
    assert_eq!(
        manager.get_approval_policy(),
        pi_coding_agent::core::permissions::policy::ApprovalPolicy::OnRequest
    );

    manager.set_approval_policy(pi_coding_agent::core::permissions::policy::ApprovalPolicy::Never);
    manager.flush();
    assert_eq!(workspace.global()["approvalPolicy"], "never");

    manager
        .set_approval_preset(ApprovalPresetName::LlmApprovedAsk, SettingsScope::Global)
        .unwrap();
    manager.flush();
    let saved = workspace.global();
    assert_eq!(saved["approvalPreset"], "llm-approved-ask");
    assert_eq!(saved["approvalPolicy"], "on-request");
    assert_eq!(manager.get_approval_preset(), ApprovalPresetName::LlmApprovedAsk);
}

#[test]
fn legacy_llm_approved_preset_migrates_to_deny() {
    let manager = SettingsManager::in_memory(
        settings(json!({"approvalPreset": "llm-approved"})),
        SettingsManagerCreateOptions::default(),
    );
    assert_eq!(manager.get_approval_preset(), ApprovalPresetName::LlmApprovedDeny);
}

#[test]
fn project_approval_preset_writes_project_settings_only() {
    let workspace = Workspace::new();
    let mut manager = workspace.manager();

    manager
        .set_approval_preset(ApprovalPresetName::NeverAskDeny, SettingsScope::Project)
        .unwrap();
    manager.flush();

    assert!(!workspace.agent_dir.join("settings.json").exists());
    let project = workspace.project();
    assert_eq!(project["approvalPreset"], "never-ask-deny");
    assert_eq!(project["approvalPolicy"], "never");
    assert_eq!(
        manager.get_approval_policy(),
        pi_coding_agent::core::permissions::policy::ApprovalPolicy::Never
    );
}

#[test]
fn sandbox_profile_precedence_and_persistence() {
    let manager = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    assert_eq!(manager.get_sandbox_profile(), SandboxProfileName::WorkspaceWrite);
    assert_eq!(manager.get_explicit_sandbox_profile(), None);

    let workspace = Workspace::new();
    workspace.write_global(r#"{"sandboxProfile":"read-only"}"#);
    workspace.write_project(r#"{"sandboxProfile":"full-access"}"#);
    let mut manager = workspace.manager();
    assert_eq!(manager.get_sandbox_profile(), SandboxProfileName::FullAccess);
    manager.set_session_sandbox_profile(Some(SandboxProfileName::WorkspaceWrite));
    assert_eq!(
        manager.get_session_sandbox_profile(),
        Some(SandboxProfileName::WorkspaceWrite)
    );
    assert_eq!(manager.get_configured_sandbox_profile(), SandboxProfileName::FullAccess);
    assert_eq!(
        manager.get_explicit_sandbox_profile(),
        Some(SandboxProfileName::WorkspaceWrite)
    );
    assert_eq!(
        manager.get_merged_settings()["sandboxProfile"],
        JsValue::String("workspace-write".into())
    );

    manager.set_session_sandbox_profile(None);
    assert_eq!(
        manager.get_explicit_sandbox_profile(),
        Some(SandboxProfileName::FullAccess)
    );

    manager
        .set_sandbox_profile(SandboxProfileName::ReadOnly, SettingsScope::Project)
        .unwrap();
    manager.flush();
    assert_eq!(workspace.project()["sandboxProfile"], "read-only");
}

#[test]
fn permission_membership_rejects_unpaired_settings_values_without_repair() {
    for (global, project, expected) in [
        (
            r#"{"approvalPolicy":"never","approvalPreset":"ask-me\ud800","sandboxProfile":"full-access"}"#,
            r#"{"sandboxProfile":"read-only\udc00"}"#,
            Some(SandboxProfileName::FullAccess),
        ),
        (
            r#"{"approvalPolicy":"never","approvalPreset":"\udc00","sandboxProfile":"full-access\ud800"}"#,
            r#"{}"#,
            None,
        ),
        (
            r#"{"approvalPolicy":"never","approvalPreset":"\ud800","sandboxProfile":"\udc00"}"#,
            r#"{"sandboxProfile":"read-only"}"#,
            Some(SandboxProfileName::ReadOnly),
        ),
    ] {
        let workspace = Workspace::new();
        workspace.write_global(global);
        workspace.write_project(project);
        let manager = workspace.manager();
        assert_eq!(manager.get_approval_preset(), ApprovalPresetName::NeverAskDeny);
        assert_eq!(manager.get_explicit_sandbox_profile(), expected);
    }
}

#[test]
fn packages_migration_keeps_local_extensions_and_filter_objects() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"extensions":["/local/ext.ts","./relative/ext.ts"]}"#);
    let manager = workspace.manager();
    assert_eq!(manager.get_packages().unwrap(), Vec::new());
    assert_eq!(
        manager.get_extension_paths(),
        vec!["/local/ext.ts", "./relative/ext.ts"]
    );

    workspace.write_global(
        r#"{"packages":["npm:simple-pkg",{"source":"npm:shitty-extensions","extensions":["extensions/oracle.ts"],"skills":[]}]}"#,
    );
    let manager = workspace.manager();
    let packages = manager.get_packages().unwrap();
    assert_eq!(packages.len(), 2);
    assert_eq!(
        packages[0],
        pi_coding_agent::core::settings_manager::PackageSource::Name("npm:simple-pkg".to_owned().into())
    );
    let pi_coding_agent::core::settings_manager::PackageSource::Filter(filter) = &packages[1] else {
        panic!("expected filter package");
    };
    assert_eq!(filter.source, "npm:shitty-extensions");
    assert_eq!(filter.extensions, Some(vec![JsString::from("extensions/oracle.ts")]));
    assert_eq!(filter.skills, Some(Vec::new()));
}

#[test]
fn reload_reads_disk_and_keeps_previous_settings_on_invalid_json() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"dark","extensions":["/before.ts"]}"#);
    let mut manager = workspace.manager();

    workspace.write_global(r#"{"theme":"light","extensions":["/after.ts"],"defaultModel":"claude-sonnet"}"#);
    manager.reload();
    assert_eq!(manager.get_theme(), Some(&JsString::from("light")));
    assert_eq!(manager.get_extension_paths(), vec!["/after.ts"]);
    assert_eq!(manager.get_default_model(), Some(&JsString::from("claude-sonnet")));

    workspace.write_global("{ invalid json");
    manager.reload();
    assert_eq!(manager.get_theme(), Some(&JsString::from("light")));
}

#[test]
fn slash_separated_themes_are_settings_not_fixed_themes() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"light/dark"}"#);
    let manager = workspace.manager();
    assert_eq!(manager.get_theme(), None);
    assert_eq!(manager.get_theme_setting(), Some(&JsString::from("light/dark")));
}

#[test]
fn drains_load_errors_once() {
    let workspace = Workspace::new();
    workspace.write_global("{ invalid global json");
    workspace.write_project("{ invalid project json");
    let mut manager = workspace.manager();

    let mut scopes: Vec<&str> = manager
        .drain_errors()
        .into_iter()
        .map(|error| error.scope.as_str())
        .collect();
    scopes.sort_unstable();
    assert_eq!(scopes, vec!["global", "project"]);
    assert!(manager.drain_errors().is_empty());
}

#[test]
fn untrusted_projects_skip_reads_and_reject_writes() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"global","defaultProjectTrust":"always"}"#);
    workspace.write_project(r#"{"theme":"project","defaultProjectTrust":"never","packages":["npm:existing"]}"#);
    let mut manager = workspace.untrusted();

    assert!(!manager.is_project_trusted());
    assert_eq!(manager.get_theme(), Some(&JsString::from("global")));
    assert_eq!(manager.get_project_settings(), Settings::new());
    assert_eq!(manager.get_default_project_trust(), DefaultProjectTrust::Always);

    let error = manager
        .set_project_packages(vec![pi_coding_agent::core::settings_manager::PackageSource::Name(
            "npm:new".to_owned().into(),
        )])
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Project is not trusted; refusing to write project settings"
    );
    assert_eq!(manager.get_project_settings(), Settings::new());
    assert_eq!(workspace.project()["packages"], json!(["npm:existing"]));

    manager.set_project_trusted(true);
    assert!(manager.is_project_trusted());
    assert_eq!(manager.get_theme(), Some(&JsString::from("project")));
}

#[test]
fn invalid_project_trust_defaults_to_ask() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"defaultProjectTrust":"sometimes"}"#);
    let manager = workspace.manager();
    assert_eq!(manager.get_default_project_trust(), DefaultProjectTrust::Ask);
}

#[test]
fn reading_project_settings_does_not_create_the_config_directory() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"theme":"dark"}"#);
    fs::remove_dir_all(workspace.project_dir.join(".pi")).unwrap();

    let mut manager = workspace.manager();
    assert!(!workspace.project_dir.join(".pi").exists());
    assert_eq!(manager.get_theme(), Some(&JsString::from("dark")));

    manager
        .set_project_packages(vec![pi_coding_agent::core::settings_manager::PackageSource::Name(
            "npm:test-pkg".to_owned().into(),
        )])
        .unwrap();
    manager.flush();
    assert!(workspace.project_dir.join(".pi").join("settings.json").exists());
}

#[test]
fn http_idle_timeout_defaults_merges_and_rejects_invalid_values() {
    let workspace = Workspace::new();
    let manager = workspace.manager();
    assert_eq!(
        manager.get_http_idle_timeout_ms().unwrap(),
        pi_coding_agent::core::http_dispatcher::DEFAULT_HTTP_IDLE_TIMEOUT_MS
    );

    workspace.write_global(r#"{"httpIdleTimeoutMs":300000}"#);
    workspace.write_project(r#"{"httpIdleTimeoutMs":0}"#);
    let manager = workspace.manager();
    assert_eq!(manager.get_http_idle_timeout_ms().unwrap(), 0);

    workspace.write_global(r#"{"httpIdleTimeoutMs":-1}"#);
    workspace.write_project("{}");
    let manager = workspace.manager();
    assert!(manager.get_http_idle_timeout_ms().is_err());
}

#[test]
fn output_pad_falls_back_for_unsupported_values() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"outputPad":2}"#);
    let manager = workspace.manager();
    assert_eq!(manager.get_output_pad(), 1);

    let mut manager = workspace.manager();
    manager.set_output_pad(0);
    manager.flush();
    let saved = workspace.global();
    assert_eq!(saved["outputPad"], 0);
}

#[test]
fn shell_command_prefix_loads_and_is_preserved_on_unrelated_saves() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"shellCommandPrefix":"shopt -s expand_aliases"}"#);
    let mut manager = workspace.manager();
    assert_eq!(
        manager.get_shell_command_prefix(),
        Some(&JsString::from("shopt -s expand_aliases"))
    );

    manager.set_theme(&JsString::from("light"));
    manager.flush();
    let saved = workspace.global();
    assert_eq!(saved["shellCommandPrefix"], "shopt -s expand_aliases");
    assert_eq!(saved["theme"], "light");

    let manager = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    assert_eq!(manager.get_shell_command_prefix(), None);
}

#[test]
fn session_dir_expands_tilde_but_shell_path_is_literal() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"sessionDir":"~/sessions","shellPath":"~"}"#);
    let manager = workspace.manager();
    let home = std::env::var("HOME").unwrap();
    assert_eq!(
        manager.get_session_dir(),
        (Some(format!("{home}/sessions"))).map(JsString::from)
    );
    assert_eq!(manager.get_shell_path(), Some(JsString::from("~".to_owned())));

    workspace.write_global(r#"{"sessionDir":"/tmp/sessions","shellPath":"/bin/zsh"}"#);
    let manager = workspace.manager();
    assert_eq!(
        manager.get_session_dir(),
        Some(JsString::from("/tmp/sessions".to_owned()))
    );
    assert_eq!(manager.get_shell_path(), Some(JsString::from("/bin/zsh".to_owned())));

    let manager = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    assert_eq!(manager.get_session_dir(), None);
    assert_eq!(manager.get_shell_path(), None);
}

#[test]
fn miscellaneous_toggles_round_trip() {
    let workspace = Workspace::new();
    let mut manager = workspace.manager();
    manager.set_double_escape_action(DoubleEscapeAction::Fork);
    manager.set_tree_filter_mode(TreeFilterMode::LabeledOnly);
    manager.set_show_hardware_cursor(true);
    manager.set_editor_padding_x(9.0);
    manager.set_autocomplete_max_visible(1.0);
    manager.set_image_width_cells(3.9);
    manager.set_show_terminal_progress(true);
    manager.set_image_auto_resize(false);
    manager.set_block_images(true);
    manager.flush();

    let saved = workspace.global();
    assert_eq!(saved["doubleEscapeAction"], "fork");
    assert_eq!(saved["treeFilterMode"], "labeled-only");
    assert_eq!(saved["showHardwareCursor"], true);
    assert_eq!(saved["editorPaddingX"], 3);
    assert_eq!(saved["autocompleteMaxVisible"], 3);
    assert_eq!(saved["terminal"]["imageWidthCells"], 3);
    assert_eq!(saved["terminal"]["showTerminalProgress"], true);
    assert_eq!(saved["images"]["autoResize"], false);
    assert_eq!(saved["images"]["blockImages"], true);
}

#[test]
fn analytics_generates_a_tracking_id_on_first_opt_in() {
    let workspace = Workspace::new();
    let mut manager = workspace.manager();
    assert!(!manager.get_enable_analytics());
    manager.set_enable_analytics(true).unwrap();
    manager.flush();
    assert!(manager.get_enable_analytics());
    let tracking_id = manager.get_tracking_id().unwrap().to_owned();
    assert_eq!(tracking_id.len(), 36);

    manager.set_enable_analytics(true).unwrap();
    manager.flush();
    assert_eq!(manager.get_tracking_id(), Some(&tracking_id));
}

#[test]
fn nested_scope_overlay_replaces_grandchildren() {
    let workspace = Workspace::new();
    workspace.write_global(r#"{"agents":{"implement":{"model":"global","tools":["read"]}},"retry":{"provider":{"timeoutMs":10,"maxRetries":2}},"permissionRules":{"allow":{"read":["global"]}}}"#);
    workspace.write_project(r#"{"agents":{"implement":{"thinkingLevel":"low"}},"retry":{"provider":{"maxRetries":3}},"permissionRules":{"allow":{"write":["project"]}}}"#);
    let manager = workspace.manager();
    let merged = manager.get_merged_settings();
    assert_eq!(merged["agents"]["implement"], json!({"thinkingLevel":"low"}).into());
    assert_eq!(merged["retry"]["provider"], json!({"maxRetries":3}).into());
    assert_eq!(merged["permissionRules"]["allow"], json!({"write":["project"]}).into());
}

#[test]
fn package_filter_round_trip_keeps_unknown_properties_and_original_order() {
    let workspace = Workspace::new();
    let expected = "{\n  \"packages\": [\n    {\n      \"skills\": [\n        \"skill\"\n      ],\n      \"unknown\": {\n        \"keep\": true\n      },\n      \"source\": \"npm:original\"\n    }\n  ]\n}";
    workspace.write_global(expected);
    let mut manager = workspace.manager();
    let packages = manager.get_packages().unwrap();
    manager.set_packages(packages);
    manager.flush();
    assert_eq!(
        fs::read_to_string(workspace.agent_dir.join("settings.json")).unwrap(),
        expected
    );
    let mut packages = manager.get_packages().unwrap();
    let pi_coding_agent::core::settings_manager::PackageSource::Filter(filter) = &mut packages[0] else {
        panic!("filter fixture");
    };
    filter.source = "npm:edited".into();
    manager.set_packages(packages);
    manager.flush();
    assert_eq!(
        fs::read_to_string(workspace.agent_dir.join("settings.json")).unwrap(),
        expected.replace("npm:original", "npm:edited")
    );
}

#[test]
fn invalid_timeout_text_uses_javascript_string_conversion() {
    let manager = SettingsManager::in_memory(
        settings(json!({"httpIdleTimeoutMs":"typo","websocketConnectTimeoutMs":"later"})),
        Default::default(),
    );
    assert_eq!(
        manager.get_http_idle_timeout_ms().unwrap_err().message(),
        "Invalid httpIdleTimeoutMs setting: typo"
    );
    assert_eq!(
        manager.get_websocket_connect_timeout_ms().unwrap_err().message(),
        "Invalid websocketConnectTimeoutMs setting: later"
    );
}

#[test]
fn settings_directory_read_reports_node_error() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.agent_dir.join("settings.json")).unwrap();
    let errors = workspace.manager().drain_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].scope, SettingsScope::Global);
    assert_eq!(errors[0].message, "EISDIR: illegal operation on a directory, read");
}

#[test]
fn settings_write_failure_reports_node_open_error_and_keeps_later_scope() {
    let workspace = Workspace::new();
    let mut manager = workspace.manager();
    fs::create_dir(workspace.agent_dir.join("settings.json")).unwrap();
    manager.set_theme(&"light".into());
    manager.set_project_theme_paths(&["project-theme".into()]).unwrap();
    manager.flush();
    let errors = manager.drain_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "EISDIR: illegal operation on a directory, read");
    assert_eq!(workspace.project()["themes"], json!(["project-theme"]));
}

#[test]
fn shell_path_is_literal() {
    let manager = SettingsManager::in_memory(
        settings(json!({"shellPath":"~/bin/../shell"})),
        SettingsManagerCreateOptions::default(),
    );
    assert_eq!(
        manager.get_shell_path().as_ref(),
        Some(&JsString::from("~/bin/../shell"))
    );
}

#[test]
fn compaction_threshold_helper_matches_json_value_contract() {
    use pi_coding_agent::core::compaction_threshold::parse_compaction_threshold_percent;
    assert_eq!(parse_compaction_threshold_percent(None).unwrap(), None);
    for percent in [0.001, 50.0, 100.0] {
        assert_eq!(
            parse_compaction_threshold_percent(Some(&JsValue::from(&json!(percent)))).unwrap(),
            Some(percent)
        );
    }
    for (value, text) in [
        (json!(null), "null"),
        (json!(0), "0"),
        (json!(-1), "-1"),
        (json!(100.001), "100.001"),
        (json!("50"), "50"),
        (json!(true), "true"),
        (json!([]), ""),
        (json!({}), "[object Object]"),
    ] {
        assert_eq!(
            parse_compaction_threshold_percent(Some(&JsValue::from(&value)))
                .unwrap_err()
                .message(),
            &pi_js_compat::js_format!(
                "Invalid compaction.thresholdPercent setting: {}. Use a finite number greater than 0 and at most 100, or remove the setting.",
                text
            )
        );
    }
}

#[test]
fn retry_delay_validation_reports_exact_setting_and_range() {
    for name in ["baseDelayMs", "maxBackoffMs"] {
        for (value, text) in [
            (json!(null), "null"),
            (json!(-1), "-1"),
            (json!(1.5), "1.5"),
            (json!(2147483648_i64), "2147483648"),
            (json!(9007199254740992_i64), "9007199254740992"),
            (json!("30"), "30"),
            (json!(true), "true"),
        ] {
            let mut retry = serde_json::Map::new();
            retry.insert(name.to_owned(), value);
            let manager = SettingsManager::in_memory(
                settings(json!({"retry":retry})),
                SettingsManagerCreateOptions::default(),
            );
            assert_eq!(
                manager.get_retry_settings().unwrap_err().message(),
                &pi_js_compat::js_format!(
                    "Invalid retry.{} setting: {}; expected integer milliseconds between 0 and 2147483647",
                    name,
                    text
                )
            );
        }
    }
    let manager = SettingsManager::in_memory(
        settings(json!({"retry":{"baseDelayMs":30,"maxBackoffMs":29}})),
        SettingsManagerCreateOptions::default(),
    );
    assert_eq!(
        manager.get_retry_settings().unwrap_err().message(),
        "retry.maxBackoffMs must be at least retry.baseDelayMs"
    );
}
