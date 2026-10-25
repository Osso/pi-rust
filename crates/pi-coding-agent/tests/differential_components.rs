#![recursion_limit = "256"]

//! Differential test: coding-agent selector components render and handle input
//! exactly like the pinned TypeScript components.
//!
//! The TypeScript side runs through `oracle/coding-agent-components-oracle.ts`.
//! The Rust side runs in a probe child process so `PI_PACKAGE_DIR` /
//! `PI_CODING_AGENT_DIR` can point at fixture directories.

#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use oracle::TsOracle;
use serde_json::{Value, json};
use tempfile::TempDir;

const SOURCE_DIRECTORY: &str = "packages/coding-agent/src/modes/interactive/components";
const PROBE_NAME: &str = "COMPONENT_PROBE_NAME";
const PROBE_OUT: &str = "COMPONENT_PROBE_OUT";

fn fixture_package_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("package fixture dir");
    fs::create_dir(directory.path().join("theme")).expect("theme dir");
    for name in ["dark.json", "light.json"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join(name),
            directory.path().join("theme").join(name),
        )
        .expect("copy built-in theme");
    }
    directory
}

fn fixture_agent_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("agent fixture dir");
    fs::create_dir(directory.path().join("themes")).expect("themes dir");
    directory
}

fn thinking_cases() -> Vec<Value> {
    vec![json!({
        "name": "thinking",
        "component": "thinking",
        "props": {
            "currentLevel": "medium",
            "availableLevels": ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"],
        },
        "width": 80,
        "steps": [
            {"key": "\u{1b}[B"},
            {"key": "\u{1b}[A"},
            {"key": "\u{1b}[A"},
            {"width": 40},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn theme_cases() -> Vec<Value> {
    vec![json!({
        "name": "theme",
        "component": "theme",
        "props": {"currentTheme": "dark"},
        "width": 80,
        "steps": [
            {"key": "\u{1b}[B"},
            {"key": "\u{1b}[A"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn show_images_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "show-images-yes",
            "component": "show_images",
            "props": {"currentValue": true},
            "width": 60,
            "steps": [{"key": "\u{1b}[B"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "show-images-no",
            "component": "show_images",
            "props": {"currentValue": false},
            "width": 60,
            "steps": [{"key": "\u{1b}[A"}, {"key": "\u{1b}"}],
        }),
    ]
}

fn extension_cases() -> Vec<Value> {
    vec![json!({
        "name": "extension",
        "component": "extension",
        "props": {"title": "Choose an action", "options": ["alpha", "beta", "gamma"]},
        "width": 70,
        "steps": [
            {"key": "j"},
            {"key": "j"},
            {"key": "j"},
            {"key": "k"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn approval_cases() -> Vec<Value> {
    vec![json!({
        "name": "approval",
        "component": "approval",
        "props": {"currentPreset": "llm-approved-deny"},
        "width": 100,
        "steps": [
            {"key": "j"},
            {"key": "\t"},
            {"key": "\r"},
            {"key": "\t"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn trust_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "trust-saved-parent",
            "component": "trust",
            "props": {
                "cwd": "/parent/project",
                "savedDecision": {"path": "/parent", "decision": true},
                "projectTrusted": true,
            },
            "width": 100,
            "steps": [{"key": "j"}, {"key": "k"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "trust-none",
            "component": "trust",
            "props": {"cwd": "/project", "savedDecision": null, "projectTrusted": false},
            "width": 100,
            "steps": [{"key": "j"}, {"key": "\r"}],
        }),
    ]
}

fn sandbox_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "sandbox-global",
            "component": "sandbox",
            "props": {"currentProfile": "workspace-write"},
            "width": 100,
            "steps": [{"key": "j"}, {"key": "\t"}, {"key": "\t"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "sandbox-session-override",
            "component": "sandbox",
            "props": {"currentProfile": "full-access", "sessionProfile": "read-only"},
            "width": 100,
            "steps": [{"key": "\t"}, {"key": "\t"}, {"key": "j"}, {"key": "\r"}],
        }),
    ]
}

fn user_message_cases() -> Vec<Value> {
    let messages: Vec<Value> = (1..=12)
        .map(|index| {
            json!({
                "id": format!("m{index}"),
                "text": format!("message {index}\nwith a second line that is quite long and should be truncated at narrow widths"),
            })
        })
        .collect();
    vec![
        json!({
            "name": "user-message-narrow",
            "component": "user_message",
            "props": {"messages": messages, "initialSelectedId": "m2"},
            "width": 30,
            "steps": [
                {"key": "\u{1b}[A"},
                {"key": "\u{1b}[A"},
                {"key": "\u{1b}[B"},
                {"key": "\r"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "user-message-empty",
            "component": "user_message",
            "props": {"messages": [], "initialSelectedId": null},
            "width": 60,
            "steps": [{"key": "\u{1b}"}],
        }),
    ]
}

fn agent_node(id: &str, parent_id: Option<&str>, display_name: &str, lifecycle: &str, slot: Option<i64>) -> Value {
    json!({
        "id": id,
        "parentId": parent_id,
        "displayName": display_name,
        "agentType": "general",
        "lifecycle": lifecycle,
        "revision": 1,
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-01T00:00:01.000Z",
        "cwd": "/tmp",
        "permission": {"policy": "on-request", "narrowed": false},
        "slot": slot.map(|index| json!({"index": index, "pinned": false})),
    })
}

fn agent_switcher_cases() -> Vec<Value> {
    let tree = vec![
        agent_node("root-a", None, "Root A", "running", Some(1)),
        agent_node("child-a1", Some("root-a"), "Child A1", "running", Some(2)),
        agent_node("child-a2", Some("root-a"), "Child A2", "waiting_for_input", None),
        agent_node("root-b", None, "Root B", "running", Some(3)),
        agent_node("child-b1", Some("root-b"), "Child B1", "steering_pending", None),
    ];
    let mut many = Vec::new();
    many.push(agent_node("root", None, "Root", "running", Some(1)));
    for index in 1..=12 {
        many.push(agent_node(
            &format!("agent-{index:02}"),
            Some("root"),
            &format!("Agent {index}"),
            "running",
            None,
        ));
    }
    let with_closed = vec![
        agent_node("root", None, "Root", "running", Some(1)),
        agent_node("done", Some("root"), "Completed agent", "completed", Some(2)),
        agent_node("failed", Some("root"), "Failed agent", "failed", None),
        agent_node("active", Some("root"), "Active agent", "running", None),
        agent_node("aborted", Some("root"), "Aborted agent", "aborted", None),
        agent_node(
            "a-very-long-agent-identifier",
            Some("root"),
            "Long identifier agent",
            "running",
            None,
        ),
    ];
    vec![
        json!({
            "name": "agent-switcher-tree",
            "component": "agent_switcher",
            "props": {"agents": tree, "selectedAgentId": "child-a1"},
            "width": 100,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[A"},
                {"key": "\r"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "agent-switcher-closed",
            "component": "agent_switcher",
            "props": {"agents": with_closed, "selectedAgentId": "active"},
            "width": 100,
            "steps": [
                {"key": "\u{8}"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\r"},
                {"key": "\u{8}"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "agent-switcher-scroll",
            "component": "agent_switcher",
            "props": {"agents": many, "selectedAgentId": "agent-01"},
            "width": 60,
            "steps": [
                {"key": "\u{1b}[6~"},
                {"key": "\u{1b}[6~"},
                {"key": "\u{1b}[5~"},
                {"key": "\u{1b}[B"},
            ],
        }),
    ]
}

fn oauth_provider(id: &str, name: &str, auth_type: &str, method: Option<&str>, status: Option<Value>) -> Value {
    json!({
        "id": id,
        "name": name,
        "authType": auth_type,
        "methodName": method,
        "status": status,
    })
}

fn crate_auth_status(status: Option<&Value>) -> pi_coding_agent::core::auth_storage::AuthStatus {
    pi_coding_agent::core::auth_storage::AuthStatus {
        configured: status.and_then(|value| value["configured"].as_bool()).unwrap_or(false),
        source: status.and_then(|value| value["source"].as_str()).map(str::to_owned),
        label: status.and_then(|value| value["label"].as_str()).map(JsString::from),
    }
}

fn oauth_selector_cases() -> Vec<Value> {
    let mixed = vec![
        oauth_provider(
            "anthropic",
            "Anthropic",
            "oauth",
            Some("Claude Pro"),
            Some(json!({"type": "oauth", "source": "OAuth"})),
        ),
        oauth_provider(
            "github",
            "GitHub",
            "api_key",
            Some("GitHub token"),
            Some(json!({"type": "api_key", "source": "GITHUB_TOKEN"})),
        ),
        oauth_provider(
            "openai",
            "OpenAI",
            "api_key",
            None,
            Some(json!({"type": "api_key", "source": "stored credential"})),
        ),
        oauth_provider(
            "google",
            "Google",
            "oauth",
            Some("Google"),
            Some(json!({"type": "api_key", "source": "GOOGLE_KEY"})),
        ),
        oauth_provider(
            "custom",
            "Custom Provider",
            "api_key",
            Some("Custom"),
            Some(json!({"type": "api_key", "source": "FOO, BAR_2"})),
        ),
        oauth_provider(
            "plain",
            "Plain Provider",
            "api_key",
            None,
            Some(json!({"type": "api_key", "source": "weird source"})),
        ),
        oauth_provider("noconfig", "Unconfigured", "api_key", None, None),
    ];
    let many: Vec<Value> = (1..=12)
        .map(|index| {
            oauth_provider(
                &format!("provider-{index:02}"),
                &format!("Provider {index}"),
                "api_key",
                None,
                None,
            )
        })
        .collect();
    vec![
        json!({
            "name": "oauth-mixed",
            "component": "oauth_selector",
            "props": {"mode": "login", "providers": mixed, "initialSearchInput": null},
            "width": 100,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[A"},
                {"key": "\r"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "oauth-search",
            "component": "oauth_selector",
            "props": {"mode": "login", "providers": mixed, "initialSearchInput": null},
            "width": 100,
            "steps": [
                {"key": "g"},
                {"key": "i"},
                {"key": "t"},
                {"key": "\u{7f}"},
                {"key": "\r"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "oauth-logout-empty",
            "component": "oauth_selector",
            "props": {"mode": "logout", "providers": [], "initialSearchInput": null},
            "width": 80,
            "steps": [{"key": "\u{1b}[B"}, {"key": "\r"}],
        }),
        json!({
            "name": "oauth-no-match",
            "component": "oauth_selector",
            "props": {"mode": "login", "providers": mixed, "initialSearchInput": "zzz"},
            "width": 80,
            "steps": [{"key": "z"}, {"key": "z"}, {"key": "z"}, {"key": "\r"}, {"key": "\u{1b}[B"}],
        }),
        json!({
            "name": "oauth-stored-and-ambient-statuses",
            "component": "oauth_selector",
            "props": {"mode":"login", "providers":[
                {"id":"stored","name":"Stored match","authType":"api_key","storedType":"api_key","status":{"source":"environment","label":"IGNORED"}},
                {"id":"mismatch","name":"Stored mismatch","authType":"api_key","storedType":"oauth"},
                {"id":"env","name":"Environment","authType":"api_key","status":{"source":"environment","label":"FIXTURE_TOKEN"}},
                {"id":"env-default","name":"Environment default label","authType":"api_key","status":{"source":"environment"}},
                {"id":"runtime","name":"Runtime","authType":"api_key","status":{"source":"runtime"}},
                {"id":"fallback","name":"Custom","authType":"api_key","status":{"source":"fallback"}},
                {"id":"json-key","name":"Models key","authType":"api_key","status":{"source":"models_json_key"}},
                {"id":"json-command","name":"Models command","authType":"api_key","status":{"source":"models_json_command"}},
                {"id":"oauth-ambient","name":"OAuth ambient excluded","authType":"oauth","status":{"source":"runtime"}},
                {"id":"unknown","name":"Unknown source","authType":"api_key","status":{"source":"unsupported"}}
            ]},
            "width":100,
            "steps":[{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\u{1b}[B"},{"key":"\r"}]
        }),
        json!({
            "name": "oauth-scroll",
            "component": "oauth_selector",
            "props": {"mode": "login", "providers": many, "initialSearchInput": null},
            "width": 60,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
            ],
        }),
    ]
}

fn login_dialog_cases() -> Vec<Value> {
    let device_code = json!({
        "userCode": "ABCD-1234",
        "verificationUri": "https://example.com/device",
        "intervalSeconds": 5,
        "expiresInSeconds": 900,
    });
    vec![
        json!({
            "name": "login-device-code",
            "component": "login_dialog",
            "props": {"providerId": "github-copilot", "providerNameOverride": "GitHub Copilot", "titleOverride": null},
            "width": 90,
            "actions": [{"kind": "show_device_code", "info": device_code}],
            "steps": [{"width": 60}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "login-details-info-waiting",
            "component": "login_dialog",
            "props": {"providerId": "anthropic", "providerNameOverride": null, "titleOverride": "Custom Title"},
            "width": 90,
            "actions": [
                {"kind": "show_info", "lines": ["First detail", "Second detail"]},
                {"kind": "show_info", "lines": ["Provider info", "Docs: https://example.com/a", "https://example.com/b"]},
                {"kind": "show_waiting", "message": "Waiting for authorization..."},
                {"kind": "show_progress", "message": "Step 1 of 2"},
            ],
            "steps": [{"key": "\u{1b}"}],
        }),
        json!({
            "name": "login-prompt-submit",
            "component": "login_dialog",
            "props": {"providerId": "anthropic", "providerNameOverride": null, "titleOverride": null},
            "width": 90,
            "actions": [
                {"kind": "show_info", "lines": ["Enter your key"]},
                {"kind": "show_prompt", "message": "API key:", "placeholder": "sk-..."},
            ],
            "steps": [
                {"key": "s"},
                {"key": "k"},
                {"key": "-"},
                {"key": "1"},
                {"key": "\r"},
                {"width": 50},
                {"key": "\u{1b}"},
            ],
        }),
    ]
}

fn model_fixture(id: &str, name: &str, provider: &str) -> Value {
    json!({"id": id, "name": name, "provider": provider})
}

fn scoped_models_cases() -> Vec<Value> {
    let models = vec![
        model_fixture("gpt-5", "GPT-5", "openai"),
        model_fixture("gpt-5-mini", "GPT-5 mini", "openai"),
        model_fixture("claude-sonnet-4", "Claude Sonnet 4", "anthropic"),
        model_fixture("gemini-2.5-pro", "Gemini 2.5 Pro", "google"),
        model_fixture("grok-4", "Grok 4", "xai"),
    ];
    let many: Vec<Value> = (1..=12)
        .map(|index| model_fixture(&format!("model-{index:02}"), &format!("Model {index}"), "test"))
        .collect();
    vec![
        json!({
            "name": "scoped-all",
            "component": "scoped_models",
            "props": {"models": models, "enabledModelIds": null},
            "width": 100,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\r"},
                {"key": "\r"},
                {"key": "\u{01}"},
                {"key": "\u{18}"},
                {"key": "\u{13}"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "scoped-subset",
            "component": "scoped_models",
            "props": {
                "models": models,
                "enabledModelIds": ["claude-sonnet-4/anthropic", "missing/model", "gpt-5/openai"],
            },
            "width": 100,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[1;3A"},
                {"key": "\u{1b}[1;3B"},
                {"key": "\u{10}"},
                {"key": "\u{13}"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "scoped-search",
            "component": "scoped_models",
            "props": {"models": models, "enabledModelIds": null},
            "width": 80,
            "steps": [
                {"key": "g"},
                {"key": "p"},
                {"key": "t"},
                {"key": "\r"},
                {"key": "\u{18}"},
                {"key": "\u{03}"},
                {"key": "\u{03}"},
            ],
        }),
        json!({
            "name": "scoped-scroll",
            "component": "scoped_models",
            "props": {"models": many, "enabledModelIds": null},
            "width": 60,
            "steps": [
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
                {"key": "\u{1b}[B"},
            ],
        }),
    ]
}

fn cases_for(component: &str) -> Vec<Value> {
    match component {
        "thinking" => thinking_cases(),
        "theme" => theme_cases(),
        "show_images" => show_images_cases(),
        "extension" => extension_cases(),
        "approval" => approval_cases(),
        "trust" => trust_cases(),
        "sandbox" => sandbox_cases(),
        "user_message" => user_message_cases(),
        "agent_switcher" => agent_switcher_cases(),
        "oauth_selector" => oauth_selector_cases(),
        "login_dialog" => login_dialog_cases(),
        "scoped_models" => scoped_models_cases(),
        other => panic!("unknown component {other}"),
    }
}

fn compare_component(component: &str) {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    let cases = cases_for(component);

    let mut oracle = TsOracle::spawn_with_environment(
        "coding-agent-components-oracle.ts",
        SOURCE_DIRECTORY,
        &[("PI_CODING_AGENT_DIR", agent.path().as_os_str().to_owned())],
    );
    let response = oracle.request(&json!({"id": 1, "operation": "render", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let output_path = agent.path().join(format!("{component}.json"));
    let environment: Vec<(&str, OsString)> = vec![
        ("PI_PACKAGE_DIR", package.path().as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR", agent.path().as_os_str().to_owned()),
        (PROBE_NAME, OsString::from(component)),
        (PROBE_OUT, output_path.as_os_str().to_owned()),
    ];
    env_probe::run_probe("components_probe_child", &environment);

    let content = fs::read_to_string(&output_path).expect("probe output must exist");
    let actual: Vec<Value> = serde_json::from_str(&content).expect("probe output must be JSON");
    assert_eq!(actual.len(), expected.len());
    for (index, (actual_case, expected_case)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_case, expected_case,
            "{component} case {index} ({}) diverged",
            expected_case["name"]
        );
    }
}

#[test]
fn thinking_selector_matches_typescript() {
    compare_component("thinking");
}

#[test]
fn theme_selector_matches_typescript() {
    compare_component("theme");
}

#[test]
fn show_images_selector_matches_typescript() {
    compare_component("show_images");
}

#[test]
fn extension_selector_matches_typescript() {
    compare_component("extension");
}

#[test]
fn approval_selector_matches_typescript() {
    compare_component("approval");
}

#[test]
fn trust_selector_matches_typescript() {
    compare_component("trust");
}

#[test]
fn sandbox_selector_matches_typescript() {
    compare_component("sandbox");
}

#[test]
fn user_message_selector_matches_typescript() {
    compare_component("user_message");
}

#[test]
fn agent_switcher_matches_typescript() {
    compare_component("agent_switcher");
}

#[test]
fn oauth_selector_matches_typescript() {
    compare_component("oauth_selector");
}

#[test]
fn login_dialog_matches_typescript() {
    compare_component("login_dialog");
}

#[test]
fn scoped_models_selector_matches_typescript() {
    compare_component("scoped_models");
}

// ============================================================================
// Probe child: renders the Rust components for the same case list.
// ============================================================================

#[test]
#[ignore = "child process probe"]
fn components_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let component = std::env::var(PROBE_NAME).expect("COMPONENT_PROBE_NAME");
    let output_path = std::env::var(PROBE_OUT).expect("COMPONENT_PROBE_OUT");

    pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None).install();
    pi_tui::terminal_image::set_capabilities(pi_tui::terminal_image::TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    let theme_path = PathBuf::from(std::env::var("PI_PACKAGE_DIR").expect("PI_PACKAGE_DIR")).join("theme/dark.json");
    let theme = pi_coding_agent::modes::interactive::theme::load_theme_from_path(
        &JsString::from(theme_path.to_str().expect("fixture path must be UTF-8")),
        None,
    )
    .expect("fixture theme must load");
    pi_coding_agent::modes::interactive::theme::set_theme_instance(std::sync::Arc::new(theme));

    let results: Vec<Value> = cases_for(&component)
        .into_iter()
        .map(|case| run_component_case(&case))
        .collect();
    fs::write(output_path, serde_json::to_string(&results).unwrap()).expect("write probe output");
}

fn run_component_case(case: &Value) -> Value {
    use std::cell::RefCell;
    use std::rc::Rc;

    let events = Rc::new(RefCell::new(Vec::<JsString>::new()));
    let width = case["width"].as_u64().unwrap() as usize;
    let steps = case["steps"].as_array().unwrap().clone();
    let component = case["component"].as_str().unwrap();
    let props = &case["props"];

    let mut renders: Vec<Vec<JsString>> = Vec::new();
    match component {
        "thinking" => {
            use pi_agent_core::ThinkingLevel;
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::thinking_selector::ThinkingSelectorComponent;

            let levels: Vec<ThinkingLevel> = props["availableLevels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|level| match level.as_str().unwrap() {
                    "off" => ThinkingLevel::Off,
                    "minimal" => ThinkingLevel::Minimal,
                    "low" => ThinkingLevel::Low,
                    "medium" => ThinkingLevel::Medium,
                    "high" => ThinkingLevel::High,
                    "xhigh" => ThinkingLevel::XHigh,
                    "max" => ThinkingLevel::Max,
                    _ => ThinkingLevel::Ultra,
                })
                .collect();
            let current = levels
                .iter()
                .copied()
                .find(|level| level.as_str() == props["currentLevel"].as_str().unwrap())
                .unwrap();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ThinkingSelectorComponent::new(
                current,
                &levels,
                Rc::new(move |level| {
                    event_sink
                        .borrow_mut()
                        .push(format!("select:{}", level.as_str()).into())
                }),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "theme" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::theme_selector::ThemeSelectorComponent;

            let event_sink = Rc::clone(&events);
            let preview_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ThemeSelectorComponent::new(
                &JsString::from(props["currentTheme"].as_str().unwrap()),
                Rc::new(move |name| event_sink.borrow_mut().push(js_format!("select:{}", name))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
                Rc::new(move |name| preview_sink.borrow_mut().push(js_format!("preview:{}", name))),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "show_images" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::show_images_selector::ShowImagesSelectorComponent;

            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ShowImagesSelectorComponent::new(
                props["currentValue"].as_bool().unwrap(),
                Rc::new(move |value| event_sink.borrow_mut().push(format!("select:{value}").into())),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "extension" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::extension_selector::{
                ExtensionSelectorComponent, ExtensionSelectorOptions,
            };

            let options: Vec<JsString> = props["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option.as_str().unwrap().to_owned())
                .map(JsString::from)
                .collect();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let toggle_sink = Rc::clone(&events);
            let mut selector = ExtensionSelectorComponent::new(
                &JsString::from(props["title"].as_str().unwrap()),
                options.into_iter().collect::<Vec<_>>(),
                Rc::new(move |option| event_sink.borrow_mut().push(js_format!("select:{}", option))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
                ExtensionSelectorOptions {
                    on_toggle_tools_expanded: Some(Rc::new(move || {
                        toggle_sink.borrow_mut().push("toggle".to_owned().into());
                    })),
                    ..ExtensionSelectorOptions::default()
                },
            );
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "approval" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::permissions::presets::ApprovalPresetName;
            use pi_coding_agent::modes::interactive::components::approval_selector::{
                ApprovalSelectorComponent, ApprovalSelectorOptions,
            };

            let current = ApprovalPresetName::parse(props["currentPreset"].as_str().unwrap()).unwrap();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ApprovalSelectorComponent::new(ApprovalSelectorOptions {
                current_preset: current,
                on_select: Rc::new(move |selection| {
                    event_sink
                        .borrow_mut()
                        .push(format!("select:{}:{}", selection.preset.as_str(), selection.scope.as_str()).into());
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
            });
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "trust" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::trust_manager::{
                ProjectTrustDecision, ProjectTrustStoreEntry, ProjectTrustUpdate,
            };
            use pi_coding_agent::modes::interactive::components::trust_selector::{
                TrustSelectorComponent, TrustSelectorOptions,
            };

            let saved_decision = if props["savedDecision"].is_null() {
                None
            } else {
                Some(ProjectTrustStoreEntry {
                    path: JsString::from(props["savedDecision"]["path"].as_str().unwrap().to_owned()),
                    decision: if props["savedDecision"]["decision"].as_bool().unwrap() {
                        ProjectTrustDecision::Trusted
                    } else {
                        ProjectTrustDecision::Untrusted
                    },
                })
            };
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = TrustSelectorComponent::new(TrustSelectorOptions {
                cwd: JsString::from(props["cwd"].as_str().unwrap().to_owned()),
                saved_decision,
                project_trusted: props["projectTrusted"].as_bool().unwrap(),
                on_select: Rc::new(move |selection| {
                    let updates: Vec<Value> = selection
                        .updates
                        .iter()
                        .map(|update: &ProjectTrustUpdate| {
                            json!({
                                "path": update.path,
                                "decision": match update.decision {
                                    ProjectTrustDecision::Unknown => Value::Null,
                                    ProjectTrustDecision::Trusted => json!(true),
                                    ProjectTrustDecision::Untrusted => json!(false),
                                },
                            })
                        })
                        .collect();
                    event_sink
                        .borrow_mut()
                        .push(format!("select:{}:{}", selection.trusted, Value::Array(updates)).into());
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
            })
            .expect("fixture trust selector");
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "sandbox" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::permissions::presets::SandboxProfileName;
            use pi_coding_agent::modes::interactive::components::sandbox_selector::{
                SandboxSelectorComponent, SandboxSelectorOptions, SandboxSelectorSelection,
            };

            let parse_profile =
                |value: &Value| SandboxProfileName::parse(value.as_str().unwrap()).expect("known sandbox profile");
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = SandboxSelectorComponent::new(SandboxSelectorOptions {
                current_profile: parse_profile(&props["currentProfile"]),
                session_profile: props["sessionProfile"]
                    .as_str()
                    .map(|value| SandboxProfileName::parse(value).expect("known sandbox profile")),
                on_select: Rc::new(move |selection| {
                    let event = match selection {
                        SandboxSelectorSelection::Saved { profile, scope } => {
                            format!("select:{}:{}", profile.as_str(), scope.as_str())
                        }
                        SandboxSelectorSelection::Session { profile } => format!(
                            "select:{}:session",
                            profile.map(SandboxProfileName::as_str).unwrap_or("none")
                        ),
                    };
                    event_sink.borrow_mut().push(event.into());
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
            });
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "scoped_models" => {
            use pi_ai::types::{Modality, Model, ModelCost};
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::scoped_models_selector::{
                ModelsCallbacks, ModelsConfig, ScopedModelsSelectorComponent,
            };

            let models: Vec<Model> = props["models"]
                .as_array()
                .unwrap()
                .iter()
                .map(|model| Model {
                    properties: Default::default(),
                    id: model["id"].as_str().unwrap().to_owned(),
                    name: model["name"].as_str().unwrap().into(),
                    api: "openai-completions".to_owned(),
                    provider: model["provider"].as_str().unwrap().to_owned(),
                    base_url: JsString::default(),
                    reasoning: false,
                    thinking_level_map: None,
                    input: vec![Modality::Text],
                    cost: ModelCost::default(),
                    context_window: 128000.0,
                    auto_compaction_threshold: None,
                    max_tokens: 4096.0,
                    headers: None,
                    compat: None,
                })
                .collect();
            let enabled_model_ids = match props.get("enabledModelIds") {
                Some(Value::Array(ids)) => {
                    Some(ids.iter().map(|id| id.as_str().unwrap().to_owned()).collect::<Vec<_>>())
                }
                _ => None,
            };
            let change_sink = Rc::clone(&events);
            let persist_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ScopedModelsSelectorComponent::new(
                ModelsConfig {
                    all_models: models,
                    enabled_model_ids: enabled_model_ids.map(|ids| ids.into_iter().map(JsString::from).collect()),
                },
                ModelsCallbacks {
                    on_change: Rc::new(move |enabled| {
                        change_sink
                            .borrow_mut()
                            .push(format!("change:{}", serde_json::to_string(&enabled).unwrap()).into());
                    }),
                    on_persist: Rc::new(move |enabled| {
                        persist_sink
                            .borrow_mut()
                            .push(format!("persist:{}", serde_json::to_string(&enabled).unwrap()).into());
                    }),
                    on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
                },
            );
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "oauth_selector" => {
            use pi_coding_agent::modes::interactive::login_options::LoginAuthType;
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::oauth_selector::{
                AuthSelectorMode, AuthSelectorProvider, OAuthSelectorComponent,
            };

            let providers: Vec<AuthSelectorProvider> = props["providers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|provider| {
                    let auth_type = match provider["authType"].as_str().unwrap() {
                        "oauth" => LoginAuthType::Oauth,
                        _ => LoginAuthType::ApiKey,
                    };
                    AuthSelectorProvider {
                        id: provider["id"].as_str().unwrap().to_owned(),
                        name: provider["name"].as_str().unwrap().into(),
                        auth_type,
                    }
                })
                .collect();
            let mode = match props["mode"].as_str().unwrap() {
                "logout" => AuthSelectorMode::Logout,
                _ => AuthSelectorMode::Login,
            };
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let stored_providers = props["providers"].as_array().unwrap().clone();
            let status_providers = stored_providers.clone();
            let mut selector = OAuthSelectorComponent::new(
                mode,
                pi_coding_agent::core::auth_storage::AuthStorage::in_memory(stored_providers.iter().filter_map(
                    |provider| {
                        provider["storedType"]
                            .as_str()
                            .map(|kind| (provider["id"].as_str().unwrap(), json!({"type":kind})))
                    },
                )),
                providers,
                Rc::new(move |id| event_sink.borrow_mut().push(js_format!("select:{}", id))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".into())),
                Some(Rc::new(move |id| {
                    let status = status_providers
                        .iter()
                        .find(|provider| provider["id"] == id)
                        .map(|provider| &provider["status"]);
                    crate_auth_status(status)
                })),
            );
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "login_dialog" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::login_dialog::LoginDialogComponent;

            let event_sink = Rc::clone(&events);
            let render_sink = Rc::clone(&events);
            let mut dialog = LoginDialogComponent::new(
                Rc::new(move || render_sink.borrow_mut().push("render".to_owned().into())),
                props["providerId"].as_str().unwrap(),
                Rc::new(move |success, message| {
                    event_sink.borrow_mut().push(js_format!(
                        "complete:{}:{}",
                        success.to_string(),
                        message.unwrap_or_default()
                    ));
                }),
                props["providerNameOverride"].as_str().map(JsString::from).as_ref(),
                props["titleOverride"].as_str().map(JsString::from).as_ref(),
            );
            for action in case["actions"].as_array().cloned().unwrap_or_default() {
                match action["kind"].as_str().unwrap() {
                    "show_device_code" => {
                        let info = pi_ai::OAuthDeviceCodeInfo {
                            user_code: JsString::from(action["info"]["userCode"].as_str().unwrap().to_owned()),
                            verification_uri: JsString::from(
                                action["info"]["verificationUri"].as_str().unwrap().to_owned(),
                            ),
                            interval_seconds: action["info"]["intervalSeconds"].as_f64(),
                            expires_in_seconds: action["info"]["expiresInSeconds"].as_f64(),
                        };
                        dialog.show_device_code(&info);
                    }
                    "show_info" => {
                        let lines = action["lines"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|line| JsString::from(line.as_str().unwrap()))
                            .collect::<Vec<_>>();
                        dialog.show_info(&lines);
                    }
                    "show_waiting" => {
                        dialog.show_waiting(&JsString::from(action["message"].as_str().unwrap_or_default()))
                    }
                    "show_progress" => {
                        dialog.show_progress(&JsString::from(action["message"].as_str().unwrap_or_default()))
                    }
                    "show_prompt" => {
                        let _receiver = dialog.show_prompt(
                            &JsString::from(action["message"].as_str().unwrap_or_default()),
                            action["placeholder"].as_str().map(JsString::from).as_ref(),
                        );
                    }
                    other => panic!("unknown action {other}"),
                }
            }
            renders.push(dialog.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut dialog, None, step, &mut current_width);
            }
        }
        "agent_switcher" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::multi_agent_store::AgentSnapshot;
            use pi_coding_agent::modes::interactive::components::agent_switcher::AgentSwitcherComponent;

            let agents: Vec<AgentSnapshot> = props["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| serde_json::from_value(agent.clone()).expect("valid agent snapshot"))
                .collect();
            let event_sink = Rc::clone(&events);
            let inactive_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = AgentSwitcherComponent::new(
                agents,
                props["selectedAgentId"].as_str().map(str::to_owned).map(JsString::from),
                Rc::new(move |agent_id| event_sink.borrow_mut().push(js_format!("select:{}", agent_id))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
                Rc::new(move |agent: AgentSnapshot| {
                    inactive_sink.borrow_mut().push(js_format!("inactive:{}", agent.id));
                }),
            );
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "user_message" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::user_message_selector::{
                UserMessageItem, UserMessageSelectorComponent,
            };

            let messages: Vec<UserMessageItem> = props["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|message| UserMessageItem {
                    id: JsString::from(message["id"].as_str().unwrap().to_owned()),
                    text: JsString::from(message["text"].as_str().unwrap().to_owned()),
                })
                .collect();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = UserMessageSelectorComponent::new(
                messages,
                Rc::new(move |id| event_sink.borrow_mut().push(js_format!("select:{}", id))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned().into())),
                props["initialSelectedId"].as_str().map(JsString::from).as_ref(),
            );
            let list = selector.get_message_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        other => panic!("unknown component {other}"),
    }

    let events = events.borrow().clone();
    json!({"name": case["name"], "renders": renders, "events": events})
}

fn apply_step(
    renders: &mut Vec<Vec<JsString>>,
    component: &mut dyn pi_tui::tui::Component,
    input_target: Option<&pi_tui::tui::ComponentRef>,
    step: &Value,
    width: &mut usize,
) {
    if let Some(step_width) = step["width"].as_u64() {
        *width = step_width as usize;
    }
    if let Some(key) = step["key"].as_str() {
        match input_target {
            Some(target) => target.borrow_mut().handle_input(&JsString::from(key)),
            None => component.handle_input(&JsString::from(key)),
        }
    }
    renders.push(component.render(*width));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
