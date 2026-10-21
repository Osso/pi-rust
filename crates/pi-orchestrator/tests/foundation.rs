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
