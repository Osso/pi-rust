use pi_orchestrator::{
    config::orchestrator_dir,
    ipc::protocol::{encode_message, parse_request_line},
    storage::Storage,
};
use serde_json::json;
#[test]
fn config_precedence_and_empty_overrides() {
    let mut env = std::collections::BTreeMap::new();
    assert_eq!(
        orchestrator_dir(&env, "/home/test"),
        std::path::Path::new("/home/test/.pi/orchestrator")
    );
    env.insert("PI_CONFIG_DIR".into(), "/private/pi".into());
    assert_eq!(
        orchestrator_dir(&env, "/home/test"),
        std::path::Path::new("/private/pi/orchestrator")
    );
    env.insert("PI_ORCHESTRATOR_DIR".into(), "/private/orch".into());
    assert_eq!(
        orchestrator_dir(&env, "/home/test"),
        std::path::Path::new("/private/orch")
    );
    env.insert("PI_ORCHESTRATOR_DIR".into(), String::new());
    assert_eq!(
        orchestrator_dir(&env, "/home/test"),
        std::path::Path::new("/private/pi/orchestrator")
    );
}
#[test]
fn protocol_uses_js_numbers_key_order_and_newline() {
    let value = json!({"type":"rpc","instanceId":"中","command":{"type":"get_state","id":"x"},"n":1.0});
    assert_eq!(
        encode_message(&value),
        concat!(
            r#"{"type":"rpc","instanceId":"中","command":{"type":"get_state","id":"x"},"n":1}"#,
            "\n"
        )
    );
    assert_eq!(
        parse_request_line(r#"{"type":"list"}"#).unwrap(),
        json!({"type":"list"})
    );
    assert!(parse_request_line("{").is_err());
}
#[test]
fn storage_upsert_keeps_order_and_remove_preserves_other_records() {
    let d = tempfile::tempdir().unwrap();
    let s = Storage(d.path().join("data"));
    assert!(s.load_instances().unwrap().is_empty());
    s.upsert_instance(json!({"id":"a","status":"online"})).unwrap();
    s.upsert_instance(json!({"id":"b","status":"starting"})).unwrap();
    s.upsert_instance(json!({"id":"a","status":"error"})).unwrap();
    assert_eq!(
        s.load_instances().unwrap(),
        vec![
            json!({"id":"a","status":"error"}),
            json!({"id":"b","status":"starting"})
        ]
    );
    s.remove_instance("a").unwrap();
    assert_eq!(s.load_instances().unwrap(), vec![json!({"id":"b","status":"starting"})]);
}

#[test]
fn config_paths_normalize_joined_segments_but_not_explicit_directory() {
    let env = [("PI_CONFIG_DIR".into(), "/private/pi/../config//".into())].into();
    assert_eq!(
        orchestrator_dir(&env, "/home/test"),
        std::path::Path::new("/private/config/orchestrator")
    );
    let config = pi_orchestrator::config::Config {
        dir: "relative/../state//".into(),
    };
    assert_eq!(config.socket_path(), std::path::Path::new("state/orchestrator.sock"));
}

#[test]
fn storage_absent_delete_corrupt_json_unknown_fields_and_enum_wire_values() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage(dir.path().into());
    assert!(storage.load_machine().unwrap().is_none());
    storage.delete_machine().unwrap();
    assert!(storage.get_instance("missing").unwrap().is_none());
    storage.remove_instance("missing").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("instances.json")).unwrap(),
        "[]"
    );
    std::fs::write(dir.path().join("machine.json"), "{").unwrap();
    assert!(storage.load_machine().is_err());
    std::fs::write(dir.path().join("instances.json"), "{").unwrap();
    assert!(storage.load_instances().is_err());
    use pi_orchestrator::types::{InstanceRecord, InstanceStatus, MachineRecord, RadiusRegistration};
    for (status, text) in [
        (InstanceStatus::Starting, "starting"),
        (InstanceStatus::Online, "online"),
        (InstanceStatus::Stopping, "stopping"),
        (InstanceStatus::Stopped, "stopped"),
        (InstanceStatus::Error, "error"),
    ] {
        assert_eq!(serde_json::to_value(status).unwrap(), text);
        assert_eq!(serde_json::from_value::<InstanceStatus>(json!(text)).unwrap(), status);
    }
    let machine: MachineRecord = serde_json::from_value(json!({"id":"m","createdAt":"t"})).unwrap();
    assert_eq!(
        serde_json::to_value(machine).unwrap(),
        json!({"id":"m","createdAt":"t"})
    );
    let instance:InstanceRecord=serde_json::from_value(json!({"id":"i","status":"online","cwd":"/tmp","createdAt":"t","label":"test","sessionId":"s","sessionFile":"f","radiusPiId":"r","lastSeenAt":"seen"})).unwrap();
    assert_eq!(serde_json::to_value(instance).unwrap()["radiusPiId"], "r");
    let registration: RadiusRegistration =
        serde_json::from_value(json!({"heartbeatIntervalMs":1000,"expiresInMs":2000})).unwrap();
    assert_eq!(registration.heartbeat_interval_ms, 1000);
}

#[test]
fn config_no_home_matches_frozen_os_homedir() {
    if std::env::var_os("ORCHESTRATOR_HOME_PROBE").is_some() {
        println!(
            "HOME_PROBE={}",
            pi_orchestrator::config::Config::from_environment().dir.display()
        );
        return;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "config_no_home_matches_frozen_os_homedir", "--nocapture"])
        .env("ORCHESTRATOR_HOME_PROBE", "1")
        .env_remove("HOME")
        .env_remove("PI_CONFIG_DIR")
        .env_remove("PI_ORCHESTRATOR_DIR")
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    let actual = text.lines().find_map(|line| line.strip_prefix("HOME_PROBE=")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let output = std::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-home-oracle.mjs"))
        .arg(source)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(actual, String::from_utf8(output.stdout).unwrap().trim());
}
