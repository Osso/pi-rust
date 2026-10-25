//! Differential proof for `auth-guidance.ts`, `trust-manager.ts`, and `permissions/rule-store.ts`.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::fs;
use std::path::PathBuf;

use pi_coding_agent::core::rule_store::{
    PermissionRuleBehavior, PermissionRuleDestination, PermissionRuleStore, PermissionRuleUpdate,
    WritePermissionRulesOptions, build_permission_rule_content, write_permission_rules,
};
use pi_coding_agent::core::trust_manager::{
    ProjectTrustDecision, ProjectTrustStore, ProjectTrustUpdate, get_project_trust_options,
    get_project_trust_parent_path,
};
use serde_json::{Value, json};

#[path = "support/config_probe.rs"]
mod config_probe;
use crate::shared_oracle as oracle;
#[path = "support/trust_probe.rs"]
mod trust_probe;

use oracle::TsOracle;

#[test]
#[ignore = "child probe entry point"]
fn config_probe_child() {
    config_probe::run_child_probe();
}

#[test]
#[ignore = "child probe entry point"]
fn trust_resources_probe_child() {
    trust_probe::run_child();
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-state-diff-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn decision_value(decision: ProjectTrustDecision) -> Value {
    match decision {
        ProjectTrustDecision::Unknown => Value::Null,
        ProjectTrustDecision::Trusted => Value::Bool(true),
        ProjectTrustDecision::Untrusted => Value::Bool(false),
    }
}

fn options_value(cwd: &str, include_session_only: bool) -> Value {
    Value::Array(
        get_project_trust_options(&JsString::from(cwd), include_session_only)
            .unwrap()
            .into_iter()
            .map(|option| {
                let mut value = json!({
                    "label": option.label,
                    "trusted": option.trusted,
                    "updates": option.updates.iter().map(|update| json!({
                        "path": update.path,
                        "decision": decision_value(update.decision),
                    })).collect::<Vec<_>>(),
                });
                if let Some(saved_path) = option.saved_path {
                    value["savedPath"] = json!(saved_path);
                }
                value
            })
            .collect(),
    )
}

#[test]
fn auth_guidance_matches_the_pinned_typescript_module() {
    let package = temp_dir("guidance-package");
    let mut oracle = TsOracle::spawn_with_environment(
        "auth-guidance-oracle.ts",
        "packages/coding-agent/src/core/auth-guidance.ts",
        &[("PI_PACKAGE_DIR", package.display().to_string().into())],
    );
    let expected = oracle.request(&json!({"id": 1, "providers": ["anthropic", "unknown", ""]}));
    assert_eq!(expected["ok"], true, "oracle failed: {expected}");
    // The Rust payload runs in a child process so `PI_PACKAGE_DIR` cannot leak into other tests.
    let actual =
        config_probe::spawn_child_probe("guidance", &[("PI_PACKAGE_DIR", package.display().to_string().into())]);
    assert_eq!(actual, expected["output"]);
}

#[test]
fn trust_store_matches_the_pinned_typescript_module() {
    let root = temp_dir("trust");
    let agent_dir = root.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let parent = root.join("trusted-parent");
    let child = parent.join("project");
    fs::create_dir_all(&child).unwrap();
    let zulu = root.join("zulu");
    let alpha = root.join("alpha");
    fs::create_dir_all(&zulu).unwrap();
    fs::create_dir_all(&alpha).unwrap();

    let steps = json!([
        {"kind": "get", "cwd": child},
        {"kind": "set", "cwd": parent, "decision": true},
        {"kind": "get", "cwd": child},
        {"kind": "set", "cwd": child, "decision": false},
        {"kind": "get", "cwd": child},
        {"kind": "set", "cwd": child, "decision": null},
        {"kind": "get", "cwd": child},
        {"kind": "options", "cwd": child, "includeSessionOnly": false},
        {"kind": "options", "cwd": child, "includeSessionOnly": true},
        {"kind": "parent", "cwd": child},
        {"kind": "setMany", "updates": [
            {"path": zulu, "decision": true},
            {"path": alpha, "decision": false}
        ]},
        {"kind": "resources", "cwd": child},
    ]);
    let mut oracle = TsOracle::spawn(
        "trust-manager-oracle.ts",
        "packages/coding-agent/src/core/trust-manager.ts",
    );
    let expected = oracle.request(&json!({
        "id": 1,
        "agentDir": agent_dir,
        "steps": steps,
        "env": {"HOME": root},
    }));
    assert_eq!(expected["ok"], true, "oracle failed: {expected}");

    // Reset the trust store so the Rust run starts from the same state.
    let _ = fs::remove_file(agent_dir.join("trust.json"));
    let _ = fs::remove_dir_all(agent_dir.join("trust.json.lock"));

    let store = ProjectTrustStore::new(&JsString::from(&agent_dir.display().to_string())).unwrap();
    let mut results: Vec<Value> = Vec::new();
    results.push(decision_value(
        store.get(&JsString::from(&child.display().to_string())).unwrap(),
    ));
    store
        .set(
            &JsString::from(&parent.display().to_string()),
            ProjectTrustDecision::Trusted,
        )
        .unwrap();
    results.push(Value::Null);
    results.push(decision_value(
        store.get(&JsString::from(&child.display().to_string())).unwrap(),
    ));
    store
        .set(
            &JsString::from(&child.display().to_string()),
            ProjectTrustDecision::Untrusted,
        )
        .unwrap();
    results.push(Value::Null);
    results.push(decision_value(
        store.get(&JsString::from(&child.display().to_string())).unwrap(),
    ));
    store
        .set(
            &JsString::from(&child.display().to_string()),
            ProjectTrustDecision::Unknown,
        )
        .unwrap();
    results.push(Value::Null);
    results.push(decision_value(
        store.get(&JsString::from(&child.display().to_string())).unwrap(),
    ));
    results.push(options_value(&child.display().to_string(), false));
    results.push(options_value(&child.display().to_string(), true));
    results.push(json!(
        get_project_trust_parent_path(&JsString::from(&child.display().to_string())).unwrap()
    ));
    store
        .set_many(&[
            ProjectTrustUpdate {
                path: zulu.display().to_string().into(),
                decision: ProjectTrustDecision::Trusted,
            },
            ProjectTrustUpdate {
                path: alpha.display().to_string().into(),
                decision: ProjectTrustDecision::Untrusted,
            },
        ])
        .unwrap();
    results.push(Value::Null);
    // `HOME` selects the always-trusted user skill directories, so the probe runs in a child.
    results.push(json!(trust_probe::spawn(
        &child.display().to_string(),
        &root.display().to_string()
    )));

    assert_eq!(Value::Array(results), expected["output"]["results"]);
    let actual_file = fs::read_to_string(agent_dir.join("trust.json")).unwrap();
    assert_eq!(json!(actual_file), expected["output"]["trustFile"]);
}

#[test]
fn permission_rule_store_matches_the_pinned_typescript_module() {
    let root = temp_dir("rules");
    let cwd = root.join("project");
    let agent_dir = cwd.join("agent");
    fs::create_dir_all(cwd.join(".pi")).unwrap();
    fs::create_dir_all(&agent_dir).unwrap();
    let mut oracle = TsOracle::spawn(
        "permission-rule-store-oracle.ts",
        "packages/coding-agent/src/core/permissions/rule-store.ts",
    );

    let inputs = [
        json!({"command": "git status", "timeout": 5}),
        json!({"timeout": 5}),
        json!({"path": "/tmp/é.txt", "nested": {"z": 1, "a": [1, 2, {"b": null}]}}),
        json!({"b": 1, "a": 2, "1": 3, "0": 4}),
        json!({}),
    ];
    for (index, input) in inputs.iter().enumerate() {
        let response = oracle.request(&json!({
            "id": index,
            "op": "content",
            "toolName": if index == 0 { "bash" } else { "read" },
            "input": input,
        }));
        assert_eq!(response["ok"], true, "content oracle failed: {response}");
        let actual = build_permission_rule_content(
            if index == 0 { "bash" } else { "read" },
            &(input.as_object().unwrap())
                .iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect(),
        );
        assert_eq!(json!({"content": actual}), response["output"], "content case {index}");
    }

    let fixtures = [
        ("no-file", None),
        ("compact", Some("{\"theme\":\"dark\",\"extensions\":[\"./ext.ts\"]}\n")),
        (
            "existing-rules",
            Some(
                "{\n  \"theme\": \"dark\",\n  \"permissionRules\": {\"allow\": {\"bash\": [\"old\"]}},\n  \"after\": true\n}\n",
            ),
        ),
        ("empty-object", Some("{}\n")),
        ("spaced-object", Some("{ }\n")),
        ("null-rules", Some("{\"permissionRules\": null, \"theme\": \"dark\"}\n")),
        (
            "nested-string",
            Some("{\"note\": \"permissionRules\", \"theme\": \"dark\"}\n"),
        ),
        (
            "nbsp-before-value",
            Some("{\n  \"a\": 1,\n  \"permissionRules\":\u{a0}{\"allow\": {}},\n  \"b\": 2\n}\n"),
        ),
        ("no-closing-brace", Some("{\"theme\":\"dark\"")),
        ("unicode-keys", Some("{\"thème\":\"sombre\"}\n")),
    ];
    let destinations = ["userSettings", "projectSettings", "localSettings"];
    for (fixture_name, content) in fixtures {
        for destination in destinations {
            let path = match destination {
                "userSettings" => agent_dir.join("settings.json"),
                "projectSettings" => cwd.join(".pi/settings.json"),
                _ => cwd.join(".pi/settings.local.json"),
            };
            if let Some(content) = content {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, content).unwrap();
            } else {
                let _ = fs::remove_file(&path);
            }
            let response = oracle.request(&json!({
                "id": fixture_name,
                "op": "write",
                "cwd": cwd,
                "agentDir": agent_dir,
                "destination": destination,
                "toolName": "bash",
                "rules": ["git status", "git status"],
                "behavior": "allow",
            }));
            let expected_content = response["output"]["content"].clone();

            // Reset and run the Rust writer on the same fixture.
            if let Some(content) = content {
                fs::write(&path, content).unwrap();
            } else {
                let _ = fs::remove_file(&path);
            }
            let destination_kind = match destination {
                "userSettings" => PermissionRuleDestination::UserSettings,
                "projectSettings" => PermissionRuleDestination::ProjectSettings,
                _ => PermissionRuleDestination::LocalSettings,
            };
            let actual = write_permission_rules(&WritePermissionRulesOptions {
                cwd: cwd.display().to_string().into(),
                agent_dir: agent_dir.display().to_string().into(),
                destination: destination_kind,
                behavior: PermissionRuleBehavior::Allow,
                tool_name: "bash".to_owned(),
                rules: vec!["git status".to_owned().into(), "git status".to_owned().into()],
            });
            match actual {
                Ok(()) => {
                    assert_eq!(response["ok"], true, "{fixture_name}/{destination}: {response}");
                    let actual_content = fs::read_to_string(&path).ok();
                    assert_eq!(
                        json!(actual_content),
                        expected_content,
                        "write case {fixture_name}/{destination:?}"
                    );
                }
                Err(error) => {
                    assert_eq!(
                        response["ok"], false,
                        "{fixture_name}/{destination} must fail: {response}"
                    );
                    assert_eq!(
                        error.to_string(),
                        response["error"]["message"],
                        "write error case {fixture_name}/{destination:?}"
                    );
                }
            }
        }
    }

    // In-memory store behavior: session rules stay local, other destinations call the writer.
    let steps = json!([
        {"kind": "add", "toolName": "bash", "rules": ["git status"]},
        {"kind": "has", "toolName": "bash", "rule": "git status"},
        {"kind": "has", "toolName": "bash", "rule": "git diff"},
        {"kind": "has", "toolName": "read", "rule": "git status"},
        {"kind": "settings", "settings": {"permissionRules": {"allow": {"read": ["/tmp/a", 3], "grep": "nope"}}}},
        {"kind": "has", "toolName": "read", "rule": "/tmp/a"},
        {"kind": "apply", "toolName": "bash", "updates": [
            {"destination": "session", "behavior": "allow", "rules": ["session-rule"]},
            {"destination": "projectSettings", "behavior": "allow", "rules": ["project-rule"]}
        ]},
        {"kind": "apply", "toolName": "bash", "cwd": cwd, "agentDir": agent_dir, "updates": [
            {"destination": "userSettings", "behavior": "allow", "rules": ["user-rule"]}
        ]}
    ]);
    let expected = oracle.request(&json!({"id": 99, "op": "store", "steps": steps}));
    assert_eq!(expected["ok"], true, "store oracle failed: {expected}");

    let mut writes: Vec<Value> = Vec::new();
    let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&recorded);
    let mut store = PermissionRuleStore::with_writer(Box::new(move |options: &WritePermissionRulesOptions| {
        sink.lock().unwrap().push(json!({
            "agentDir": options.agent_dir,
            "behavior": "allow",
            "cwd": options.cwd,
            "destination": match options.destination {
                PermissionRuleDestination::Session => "session",
                PermissionRuleDestination::UserSettings => "userSettings",
                PermissionRuleDestination::ProjectSettings => "projectSettings",
                PermissionRuleDestination::LocalSettings => "localSettings",
            },
            "rules": options.rules,
            "toolName": options.tool_name,
        }));
    }));
    let mut results: Vec<Value> = Vec::new();
    store.add_allow_rules("bash", &["git status".to_owned().into()]);
    results.push(Value::Null);
    results.push(json!(store.has_allow_rule("bash", &JsString::from("git status"))));
    results.push(json!(store.has_allow_rule("bash", &JsString::from("git diff"))));
    results.push(json!(store.has_allow_rule("read", &JsString::from("git status"))));
    store.add_settings_rules(Some(&JsValue::from(
        &json!({"permissionRules": {"allow": {"read": ["/tmp/a", 3], "grep": "nope"}}}),
    )));
    results.push(Value::Null);
    results.push(json!(store.has_allow_rule("read", &JsString::from("/tmp/a"))));
    store.apply_updated_permissions(
        "bash",
        Some(&[
            PermissionRuleUpdate::new(
                PermissionRuleDestination::Session,
                vec!["session-rule".to_owned().into()],
            ),
            PermissionRuleUpdate::new(
                PermissionRuleDestination::ProjectSettings,
                vec!["project-rule".to_owned().into()],
            ),
        ]),
    );
    results.push(Value::Null);
    store.set_locations(
        Some(cwd.display().to_string().into()),
        Some(agent_dir.display().to_string().into()),
    );
    store.apply_updated_permissions(
        "bash",
        Some(&[PermissionRuleUpdate::new(
            PermissionRuleDestination::UserSettings,
            vec!["user-rule".to_owned().into()],
        )]),
    );
    results.push(Value::Null);
    writes.extend(recorded.lock().unwrap().iter().cloned());

    assert_eq!(Value::Array(results), expected["output"]["results"]);
    assert_eq!(Value::Array(writes), expected["output"]["writes"]);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
