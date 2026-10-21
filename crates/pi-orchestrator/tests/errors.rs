use pi_orchestrator::{RpcProcessInstance, Storage};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
async fn oracle(mode: &str, input: Value) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = Path::new(lock["worktree"].as_str().unwrap());
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-errors-oracle.mjs"))
        .arg(source)
        .arg(mode)
        .arg(input.to_string())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn filesystem_errors_match_frozen_node_messages() {
    let dir = tempfile::tempdir().unwrap();
    let expected = oracle("storage", json!({"dir":dir.path()})).await;
    let actual = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let storage = Storage(case["dir"].as_str().unwrap().into());
            let error = match case["operation"].as_str().unwrap() {
                "loadMachine" => storage.load_machine().unwrap_err(),
                "loadInstances" => storage.load_instances().unwrap_err(),
                "saveMachine" => storage.save_machine(&json!({"id":"m","createdAt":"time"})).unwrap_err(),
                "deleteMachine" => storage.delete_machine().unwrap_err(),
                _ => unreachable!(),
            };
            json!({"operation":case["operation"],"dir":case["dir"],"message":error})
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), expected);
}
#[tokio::test]
async fn spawn_failures_match_frozen_node_messages() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-executable");
    std::fs::write(&file, "text").unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    let node = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let cases = vec![
        json!({"program":dir.path().join("missing"),"cwd":dir.path()}),
        json!({"program":file,"cwd":dir.path()}),
        json!({"program":dir.path(),"cwd":dir.path()}),
        json!({"program":node,"cwd":dir.path().join("missing-cwd")}),
        json!({"program":node,"cwd":file}),
        json!({"program":node,"cwd":"bad\u{0}cwd"}),
    ];
    let mut expected = vec![];
    let mut actual = vec![];
    for case in cases {
        expected.push(oracle("spawn", case.clone()).await);
        let mut command = tokio::process::Command::new(case["program"].as_str().unwrap());
        let error = match RpcProcessInstance::spawn_command(&mut command, Path::new(case["cwd"].as_str().unwrap())) {
            Ok(process) => {
                process.dispose().await.unwrap();
                panic!("spawn unexpectedly succeeded")
            }
            Err(error) => error,
        };
        actual.push(json!({"message":error}));
    }
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn closed_child_stdin_error_matches_frozen_write_callback() {
    let dir = tempfile::tempdir().unwrap();
    let script =
        "require('node:fs').closeSync(0);process.stdout.write('{\"type\":\"ready\"}\\n');setInterval(()=>{},1000)";
    let expected = oracle("stdin", json!({"cwd":dir.path(),"script":script})).await;
    let mut command = tokio::process::Command::new("node");
    command.args(["-e", script]);
    let process = RpcProcessInstance::spawn_command(&mut command, dir.path()).unwrap();
    let ready = Arc::new(tokio::sync::Notify::new());
    let notify = ready.clone();
    process.on_event(Arc::new(move |_| notify.notify_one()));
    ready.notified().await;
    let error = process
        .send(json!({"type":"get_state","id":"probe"}))
        .await
        .unwrap_err();
    process.dispose().await.unwrap();
    assert_eq!(json!({"message":error}), expected);
}
