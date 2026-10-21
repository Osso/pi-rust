use pi_orchestrator::rpc_process::RpcProcessInstance;
use serde_json::json;
#[tokio::test]
async fn child_response_correlation_events_exit_and_disposal() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"));
    let rpc = RpcProcessInstance::spawn_command(&mut command, dir.path()).unwrap();
    let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let collected = events.clone();
    rpc.on_event(std::sync::Arc::new(move |v| collected.lock().unwrap().push(v)));
    let (first, second) = tokio::join!(
        rpc.send(json!({"type":"get_state","id":"a"})),
        rpc.send(json!({"type":"get_state","id":"b"}))
    );
    assert_eq!(first.unwrap()["id"], "a");
    assert_eq!(second.unwrap()["id"], "b");
    assert_eq!(events.lock().unwrap().len(), 2);
    let error = rpc.send(json!({"type":"exit","id":"e"})).await.unwrap_err();
    assert!(error.contains("code=7"), "{error}");
    assert!(
        rpc.send(json!({"type":"get_state"}))
            .await
            .unwrap_err()
            .contains("not running")
    );
    rpc.dispose().await.unwrap();
}

#[tokio::test]
async fn routing_matches_frozen_process_and_disposal_rejects_pending() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let fake = root.join("oracle/orchestrator-rpc-fake.mjs");
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-rpc-oracle.mjs"))
        .arg(source)
        .arg(&fake)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(fake);
    let rpc = RpcProcessInstance::spawn_command(&mut command, dir.path()).unwrap();
    let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let e = events.clone();
    rpc.on_event(std::sync::Arc::new(move |v| e.lock().unwrap().push(v)));
    let ui = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let u = ui.clone();
    rpc.set_ui_request_handler(Some(std::sync::Arc::new(move |v| u.lock().unwrap().push(v))));
    let (a, b) = tokio::join!(
        rpc.send(json!({"type":"get_state","id":"a"})),
        rpc.send(json!({"type":"get_state","id":"b"}))
    );
    assert_eq!(
        json!({"responses":[a.unwrap(),b.unwrap()],"events":*events.lock().unwrap(),"ui":*ui.lock().unwrap()}),
        expected
    );
    let pending = rpc.clone();
    let task = tokio::spawn(async move { pending.send(json!({"type":"hang","id":"pending"})).await });
    tokio::task::yield_now().await;
    rpc.dispose().await.unwrap();
    assert!(task.await.unwrap().unwrap_err().contains("disposed"));
}
