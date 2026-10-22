use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value},
};
use pi_orchestrator::{ipc::protocol::encode_message, storage::Storage};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn oracle(input: Value, dir: &Path) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = Path::new(lock["worktree"].as_str().unwrap());
    let mut child = Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-oracle.mjs"))
        .arg(source)
        .env("PI_ORCHESTRATOR_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(pi_js_compat::js_json::stringify_lossless(&input).as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    serde_json::from_slice(&result.stdout).unwrap()
}
#[test]
fn frozen_protocol_storage_bidirectional_file_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let messages = json!([{"type":"spawn","cwd":"/tmp/中","label":"a\nb"},{"type":"rpc","instanceId":"i","command":{"type":"get_state","id":"r"}},{"type":"rpc_ready","ok":true},{"type":"response","success":false,"command":"prompt","error":"denied"},{"2":2,"1":1,"n":1.0}]);
    let machine = json!({"id":"machine","createdAt":"2026-10-21T00:00:00.000Z","label":"é"});
    let replacement = json!({"id":"a","status":"error","extension":{"7":7,"2":2}});
    let expected = oracle(
        json!({"op":"foundation","messages":messages,"machine":machine,"instances":[{"id":"a","status":"online"},{"id":"b","status":"stopped"}],"replacement":replacement,"removeId":"b"}),
        dir.path(),
    );
    let store = Storage(JsString::from(
        (dir.path().to_owned()).to_str().expect("utf-8 test path"),
    ));
    assert_eq!(store.load_machine().unwrap().unwrap(), machine);
    assert_eq!(store.load_instances().unwrap(), vec![replacement.clone()]);
    assert_eq!(
        expected["encoded"],
        Value::Array(
            messages
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Value::String(encode_message(v)))
                .collect()
        )
    );
    store.save_machine(&machine).unwrap();
    store.save_instances(&[replacement]).unwrap();
    assert_eq!(
        json!(std::fs::read_to_string(dir.path().join("machine.json")).unwrap()),
        expected["machineBytes"]
    );
    assert_eq!(
        json!(std::fs::read_to_string(dir.path().join("instances.json")).unwrap()),
        expected["instanceBytes"]
    );
    let read = oracle(json!({"op":"read_storage"}), dir.path());
    assert_eq!(read["machine"], expected["machine"]);
    assert_eq!(read["instances"], expected["instances"]);
    store.delete_machine().unwrap();
    store.delete_machine().unwrap();
    assert!(store.load_machine().unwrap().is_none());
}

#[test]
fn frozen_config_path_precedence_and_normalization_matrix() {
    use pi_orchestrator::config::{Config, IS_BUN_BINARY, VERSION, orchestrator_dir};
    let dir = tempfile::tempdir().unwrap();
    for env in [
        json!({}),
        json!({"PI_CONFIG_DIR":"/config//a/../b"}),
        json!({"PI_CONFIG_DIR":"../../relative/."}),
        json!({"PI_ORCHESTRATOR_DIR":"explicit//../state/"}),
        json!({"PI_ORCHESTRATOR_DIR":"","PI_CONFIG_DIR":""}),
        json!({"PI_ORCHESTRATOR_DIR":"/explicit","PI_CONFIG_DIR":"/ignored"}),
    ] {
        let map: std::collections::BTreeMap<String, JsString> =
            pi_js_compat::js_json::from_value(&env.clone()).unwrap();
        let config = Config {
            dir: orchestrator_dir(&map, &JsString::from("/home/test")),
        };
        let expected = oracle(json!({"op":"config","env":env,"home":"/home/test"}), dir.path());
        assert_eq!(
            json!({"dir":config.dir,"auth":config.auth_path(),"machine":config.machine_path(),"instances":config.instances_path(),"socket":config.socket_path(),"version":VERSION,"bun":IS_BUN_BINARY}),
            expected
        );
    }
}
