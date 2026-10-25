use super::oracle;
use pi_coding_agent::core::resident_console_transport::ResidentConsoleManagedBy;
use pi_coding_agent::supervisor::main::create_supervisor_console_snapshot;
use pi_js_compat::{js_json as json, js_json::JsString};

#[test]
fn console_snapshot_metadata_matches_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
    );
    let mut previous_instance = None;
    for (managed, tag) in [
        (ResidentConsoleManagedBy::External, "external"),
        (ResidentConsoleManagedBy::Pi, "pi"),
    ] {
        let expected=source.request(&json!({"id":1,"operation":"consoleSnapshot","cwd":"/fixture/kb","generation":7,"managedBy":tag,"sessionId":"resident"}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let snapshot =
            create_supervisor_console_snapshot(&"/fixture/kb".into(), 7, managed, Some(&"resident".into()), Vec::new());
        let identity = snapshot
            .identity
            .as_ref()
            .expect("frozen snapshot has runtime identity");
        assert_eq!(identity.pid, std::process::id() as f64);
        let executable = pi_js_compat::node_fs::decode_node_path(std::env::current_exe().unwrap().as_os_str());
        assert_eq!(identity.executable, executable);
        assert_eq!(identity.entrypoint, Some(executable));
        let uuid = identity.instance_id.to_utf8().unwrap();
        assert_eq!(uuid.len(), 36);
        assert!(
            uuid.bytes()
                .enumerate()
                .all(|(index, byte)| if [8, 13, 18, 23].contains(&index) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                })
        );
        assert_eq!(&uuid[14..15], "4");
        assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
        if let Some(previous) = &previous_instance {
            assert_eq!(&identity.instance_id, previous);
        }
        previous_instance = Some(identity.instance_id.clone());
        let mut actual = json!(snapshot);
        let identity = actual["identity"].as_object_mut().unwrap();
        identity.insert("pid".into(), json!(0));
        identity.insert("executable".into(), json!("<executable>"));
        identity.insert("entrypoint".into(), json!("<entrypoint>"));
        identity.insert("instanceId".into(), json!("<uuid>"));
        assert_eq!(actual, expected["output"]);
    }
}

#[test]
fn flags_in_argv_do_not_become_console_entrypoints() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "differential_supervisor_identity::console_identity_argv_child",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {} {}",
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap()
    );
}

#[test]
#[ignore = "invoked by flags_in_argv_do_not_become_console_entrypoints"]
fn console_identity_argv_child() {
    assert_eq!(std::env::args().nth(1).as_deref(), Some("--exact"));
    let snapshot = create_supervisor_console_snapshot(
        &"/fixture/kb".into(),
        1,
        ResidentConsoleManagedBy::External,
        None,
        Vec::new(),
    );
    let entrypoint = snapshot.identity.unwrap().entrypoint.unwrap();
    let executable = pi_js_compat::node_fs::decode_node_path(std::env::current_exe().unwrap().as_os_str());
    assert_eq!(entrypoint, executable);
    assert_ne!(entrypoint, JsString::from("--exact"));
}
