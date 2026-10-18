//! Ports `packages/coding-agent/test/permission-rule-store.test.ts`.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::rule_store::{
    PermissionRuleBehavior, PermissionRuleDestination, PermissionRuleStore, PermissionRuleUpdate,
    WritePermissionRulesOptions, build_permission_rule_content, write_permission_rules,
};
use serde_json::json;

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-rules-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn allow_rules_match_by_tool_name_and_exact_content() {
    let mut store = PermissionRuleStore::new();
    store.add_allow_rules("bash", &["git status".to_owned()]);
    assert!(store.has_allow_rule("bash", "git status"));
    assert!(!store.has_allow_rule("bash", "git diff"));
    assert!(!store.has_allow_rule("read", "git status"));
}

#[test]
fn persisted_allow_rules_load_from_settings() {
    let settings = json!({"permissionRules": {"allow": {"bash": ["git status"]}}});
    let store = PermissionRuleStore::from_settings(Some(&settings));
    assert!(store.has_allow_rule("bash", "git status"));

    let invalid = json!({"permissionRules": {"allow": {"bash": ["ok", 3], "read": "nope"}}});
    let store = PermissionRuleStore::from_settings(Some(&invalid));
    assert!(store.has_allow_rule("bash", "ok"));
    assert!(!store.has_allow_rule("bash", "3"));
    assert!(!store.has_allow_rule("read", "nope"));
}

#[test]
fn rule_content_uses_bash_commands_and_compact_json_otherwise() {
    let bash = json!({"command": "git status", "timeout": 5});
    assert_eq!(
        build_permission_rule_content("bash", bash.as_object().unwrap()),
        "git status"
    );

    let read = json!({"path": "/tmp/file.txt", "limit": 10});
    assert_eq!(
        build_permission_rule_content("read", read.as_object().unwrap()),
        "{\"path\":\"/tmp/file.txt\",\"limit\":10}"
    );

    let bash_without_command = json!({"timeout": 5});
    assert_eq!(
        build_permission_rule_content("bash", bash_without_command.as_object().unwrap()),
        "{\"timeout\":5}"
    );
}

fn setup() -> (PathBuf, PathBuf) {
    let cwd = temp_dir("write");
    let agent_dir = cwd.join("agent");
    fs::create_dir_all(cwd.join(".pi")).unwrap();
    fs::create_dir_all(&agent_dir).unwrap();
    fs::write(
        agent_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({"theme": "dark"})).unwrap(),
    )
    .unwrap();
    fs::write(
        cwd.join(".pi/settings.json"),
        serde_json::to_string_pretty(&json!({"extensions": ["./ext.ts"]})).unwrap(),
    )
    .unwrap();
    (cwd, agent_dir)
}

#[test]
fn writes_allow_rules_without_dropping_existing_settings() {
    for (destination, relative) in [
        (PermissionRuleDestination::UserSettings, "agent/settings.json"),
        (PermissionRuleDestination::ProjectSettings, ".pi/settings.json"),
        (PermissionRuleDestination::LocalSettings, ".pi/settings.local.json"),
    ] {
        let (cwd, agent_dir) = setup();
        write_permission_rules(&WritePermissionRulesOptions {
            cwd: cwd.display().to_string(),
            agent_dir: agent_dir.display().to_string(),
            destination,
            behavior: PermissionRuleBehavior::Allow,
            tool_name: "bash".to_owned(),
            rules: vec!["git status".to_owned(), "git status".to_owned()],
        })
        .unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(cwd.join(relative)).unwrap()).unwrap();
        assert_eq!(settings["permissionRules"]["allow"]["bash"], json!(["git status"]));
        if destination == PermissionRuleDestination::UserSettings {
            assert_eq!(settings["theme"], "dark");
        }
        if destination == PermissionRuleDestination::ProjectSettings {
            assert_eq!(settings["extensions"], json!(["./ext.ts"]));
        }
    }
}

#[test]
fn adding_rules_does_not_reformat_existing_settings() {
    let (cwd, agent_dir) = setup();
    let settings_path = agent_dir.join("settings.json");
    fs::write(&settings_path, "{\"theme\":\"dark\",\"extensions\":[\"./ext.ts\"]}\n").unwrap();

    write_permission_rules(&WritePermissionRulesOptions {
        cwd: cwd.display().to_string(),
        agent_dir: agent_dir.display().to_string(),
        destination: PermissionRuleDestination::UserSettings,
        behavior: PermissionRuleBehavior::Allow,
        tool_name: "bash".to_owned(),
        rules: vec!["git status".to_owned()],
    })
    .unwrap();

    let content = fs::read_to_string(&settings_path).unwrap();
    assert!(content.contains("\"theme\":\"dark\""), "content: {content}");
    assert!(content.contains("\"extensions\":[\"./ext.ts\"]"), "content: {content}");
    let settings: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(settings["permissionRules"]["allow"]["bash"], json!(["git status"]));
}

#[test]
fn adding_rules_replaces_an_existing_permission_rules_value() {
    let (cwd, agent_dir) = setup();
    let settings_path = agent_dir.join("settings.json");
    fs::write(
        &settings_path,
        "{\n  \"theme\": \"dark\",\n  \"permissionRules\": {\"allow\": {\"bash\": [\"old\"]}},\n  \"after\": true\n}\n",
    )
    .unwrap();

    write_permission_rules(&WritePermissionRulesOptions {
        cwd: cwd.display().to_string(),
        agent_dir: agent_dir.display().to_string(),
        destination: PermissionRuleDestination::UserSettings,
        behavior: PermissionRuleBehavior::Allow,
        tool_name: "bash".to_owned(),
        rules: vec!["new".to_owned()],
    })
    .unwrap();

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(settings["permissionRules"]["allow"]["bash"], json!(["old", "new"]));
    assert_eq!(settings["after"], true);
    assert!(content.contains("\"theme\": \"dark\""), "content: {content}");
}

#[test]
fn created_settings_files_are_pretty_json_with_a_trailing_newline() {
    let (cwd, agent_dir) = setup();
    write_permission_rules(&WritePermissionRulesOptions {
        cwd: cwd.display().to_string(),
        agent_dir: agent_dir.display().to_string(),
        destination: PermissionRuleDestination::LocalSettings,
        behavior: PermissionRuleBehavior::Allow,
        tool_name: "read".to_owned(),
        rules: vec!["/tmp/a".to_owned()],
    })
    .unwrap();
    let content = fs::read_to_string(cwd.join(".pi/settings.local.json")).unwrap();
    assert_eq!(
        content,
        "{\n  \"permissionRules\": {\n    \"allow\": {\n      \"read\": [\n        \"/tmp/a\"\n      ]\n    }\n  }\n}\n"
    );
}

#[test]
fn apply_updated_permissions_keeps_session_rules_and_persists_the_rest() {
    let recorded: Arc<Mutex<Vec<WritePermissionRulesOptions>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    let mut store = PermissionRuleStore::with_writer(Box::new(move |options| {
        sink.lock().unwrap().push(options.clone());
    }));
    store.set_locations(Some("/project".to_owned()), Some("/agent".to_owned()));

    store.apply_updated_permissions(
        "bash",
        Some(&[
            PermissionRuleUpdate::new(PermissionRuleDestination::Session, vec!["session-rule".to_owned()]),
            PermissionRuleUpdate::new(
                PermissionRuleDestination::ProjectSettings,
                vec!["project-rule".to_owned()],
            ),
        ]),
    );

    assert!(store.has_allow_rule("bash", "session-rule"));
    assert!(!store.has_allow_rule("bash", "project-rule"));
    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].destination, PermissionRuleDestination::ProjectSettings);
    assert_eq!(recorded[0].cwd, "/project");
    assert_eq!(recorded[0].agent_dir, "/agent");
    assert_eq!(recorded[0].tool_name, "bash");
    assert_eq!(recorded[0].rules, ["project-rule"]);
}

#[test]
fn non_session_updates_are_skipped_without_locations() {
    let recorded: Arc<Mutex<Vec<WritePermissionRulesOptions>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    let mut store = PermissionRuleStore::with_writer(Box::new(move |options| {
        sink.lock().unwrap().push(options.clone());
    }));
    store.apply_updated_permissions(
        "bash",
        Some(&[
            PermissionRuleUpdate::new(PermissionRuleDestination::Session, vec!["session-only".to_owned()]),
            PermissionRuleUpdate::new(PermissionRuleDestination::UserSettings, vec!["user-rule".to_owned()]),
        ]),
    );
    assert!(store.has_allow_rule("bash", "session-only"));
    assert!(recorded.lock().unwrap().is_empty());
    assert_eq!(
        store.allow_rules(),
        std::collections::BTreeMap::from([("bash".to_owned(), vec!["session-only".to_owned()])])
    );
}
