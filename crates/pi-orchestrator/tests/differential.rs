use pi_orchestrator::{ipc::protocol::encode_message, storage::Storage};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn oracle(input: Value, dir: &Path) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
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
        .write_all(input.to_string().as_bytes())
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
    let store = Storage(dir.path().to_owned());
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
        std::fs::read_to_string(dir.path().join("machine.json")).unwrap(),
        expected["machineBytes"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("instances.json")).unwrap(),
        expected["instanceBytes"]
    );
    let read = oracle(json!({"op":"read_storage"}), dir.path());
    assert_eq!(read["machine"], expected["machine"]);
    assert_eq!(read["instances"], expected["instances"]);
    store.delete_machine().unwrap();
    store.delete_machine().unwrap();
    assert!(store.load_machine().unwrap().is_none());
}
