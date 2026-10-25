use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value},
};
use pi_orchestrator::rpc_process::RpcProcessInstance;
#[tokio::test]
async fn child_response_correlation_events_exit_and_disposal() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"));
    let rpc = RpcProcessInstance::spawn_command(
        &mut command,
        &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
    )
    .unwrap();
    let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let collected = events.clone();
    rpc.on_event(std::sync::Arc::new(move |v| collected.lock().unwrap().push(v)));
    let (first, second) = tokio::join!(
        rpc.send(json!({"type":"get_state","id":"a"})),
        rpc.send(json!({"type":"get_state","id":"b"}))
    );
    assert_eq!(first.unwrap()["id"], json!("a"));
    assert_eq!(second.unwrap()["id"], json!("b"));
    assert_eq!(events.lock().unwrap().len(), 2);
    let error = rpc.send(json!({"type":"exit","id":"e"})).await.unwrap_err();
    assert!(error.contains("code=7"), "{error:?}");
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
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(fake);
    let rpc = RpcProcessInstance::spawn_command(
        &mut command,
        &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
    )
    .unwrap();
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

#[tokio::test]
async fn duplicate_pending_id_orphans_original_even_after_disposal() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"));
    let rpc = RpcProcessInstance::spawn_command(
        &mut command,
        &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
    )
    .unwrap();
    let first = rpc.clone();
    let mut pending = tokio::spawn(async move { first.send(json!({"type":"hang","id":"duplicate"})).await });
    tokio::task::yield_now().await;
    assert_eq!(
        rpc.send(json!({"type":"get_state","id":"duplicate"})).await.unwrap()["command"],
        json!("get_state")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
            .await
            .is_err(),
        "overwritten original resolver must remain pending"
    );
    rpc.dispose().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
            .await
            .is_err()
    );
    pending.abort();
    let _ = pending.await;
}

#[tokio::test]
async fn duplicate_id_frozen_discrepancy_is_explicitly_preserved() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let result = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-rpc-oracle.mjs"))
        .arg(source)
        .arg(root.join("oracle/orchestrator-rpc-fake.mjs"))
        .arg("duplicate")
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!({"before":true,"after":true,"command":"get_state"})
    );
    // Same observable before/after predicate is asserted against Rust in the real-child test.
}

#[tokio::test]
async fn child_signal_error_uses_node_signal_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"));
    let rpc = RpcProcessInstance::spawn_command(
        &mut command,
        &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
    )
    .unwrap();
    assert_eq!(
        rpc.send(json!({"type":"sigkill","id":"kill"})).await.unwrap_err(),
        "RPC process exited (code=null signal=SIGKILL). Stderr: "
    );
    rpc.dispose().await.unwrap();
}

#[tokio::test]
async fn native_sibling_pi_receives_rpc_mode_and_working_directory() {
    use std::os::unix::fs::PermissionsExt;
    if let Some(cwd) = std::env::var_os("ORCHESTRATOR_SIBLING_PROBE") {
        let rpc = RpcProcessInstance::spawn(&JsString::from(
            (std::path::Path::new(&cwd)).to_str().expect("utf-8 test path"),
        ))
        .unwrap();
        let response = rpc.send(json!({"type":"get_state","id":"probe"})).await.unwrap();
        println!(
            "SIBLING_PROBE={}",
            pi_js_compat::js_json::stringify_lossless(&response["data"])
        );
        rpc.dispose().await.unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let runner = dir.path().join("embedding");
    std::fs::copy(std::env::current_exe().unwrap(), &runner).unwrap();
    let pi = dir.path().join("pi");
    std::fs::write(&pi,"#!/usr/bin/python3\nimport sys,json,os\nfor line in sys.stdin:\n request=json.loads(line)\n print(json.dumps({'type':'response','success':True,'id':request['id'],'command':request['type'],'data':{'argv':sys.argv[1:],'cwd':os.getcwd()}}),flush=True)\n").unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cwd = dir.path().join("working");
    std::fs::create_dir(&cwd).unwrap();
    let output = tokio::process::Command::new(runner)
        .args([
            "--exact",
            "native_sibling_pi_receives_rpc_mode_and_working_directory",
            "--nocapture",
        ])
        .env("ORCHESTRATOR_SIBLING_PROBE", &cwd)
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let result = text
        .lines()
        .find_map(|line| line.strip_prefix("SIBLING_PROBE="))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(result).unwrap(),
        json!({"argv":["--mode","rpc"],"cwd":cwd})
    );
}

#[tokio::test]
async fn child_jsonl_eof_and_utf8_decoding_match_frozen_node_streams() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let fake = root.join("oracle/orchestrator-rpc-fake.mjs");
    let mut expected = vec![];
    let mut actual = vec![];
    for case in ["unterminated", "invalid_utf8", "stderr_utf8"] {
        let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(root.join("oracle/orchestrator-rpc-oracle.mjs"))
            .arg(source)
            .arg(&fake)
            .arg(case)
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        expected.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
        let mut command = tokio::process::Command::new("node");
        command.arg(&fake);
        let rpc = RpcProcessInstance::spawn_command(
            &mut command,
            &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
        )
        .unwrap();
        actual.push(match rpc.send(json!({"type":case,"id":"frame"})).await {
            Ok(response) => json!({"response":response}),
            Err(error) => json!({"error":error}),
        });
        rpc.dispose().await.unwrap();
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn callback_mutation_matches_frozen_live_set_iteration() {
    use pi_orchestrator::rpc_process::{ExitListener, Listener};
    use std::sync::{Arc, Mutex};
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
        .arg("listener_mutation")
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected, json!({"events":["first","late"],"exits":["first","late"]}));
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(fake);
    let rpc = RpcProcessInstance::spawn_command(&mut command, &dir.path().to_str().unwrap().into()).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let exits = Arc::new(Mutex::new(Vec::new()));
    let event_remove = Arc::new(Mutex::new(None));
    let exit_remove = Arc::new(Mutex::new(None));
    let trace = events.clone();
    let late_event: Listener = Arc::new(move |_| trace.lock().unwrap().push("late"));
    let trace = exits.clone();
    let late_exit: ExitListener = Arc::new(move |_| trace.lock().unwrap().push("late"));
    let trace = events.clone();
    let remove = event_remove.clone();
    let process = rpc.clone();
    let first_event = rpc.on_event(Arc::new(move |_| {
        trace.lock().unwrap().push("first");
        process.remove_event_listener(remove.lock().unwrap().unwrap());
        process.on_event(late_event.clone());
    }));
    let trace = events.clone();
    *event_remove.lock().unwrap() = Some(rpc.on_event(Arc::new(move |_| trace.lock().unwrap().push("removed"))));
    let trace = exits.clone();
    let remove = exit_remove.clone();
    let process = rpc.clone();
    let first_exit = rpc.on_exit(Arc::new(move |_| {
        trace.lock().unwrap().push("first");
        process.remove_exit_listener(remove.lock().unwrap().unwrap());
        process.on_exit(late_exit.clone());
    }));
    let trace = exits.clone();
    *exit_remove.lock().unwrap() = Some(rpc.on_exit(Arc::new(move |_| trace.lock().unwrap().push("removed"))));
    rpc.send(json!({"type":"get_state","id":"one"})).await.unwrap();
    rpc.remove_event_listener(first_event);
    rpc.dispose().await.unwrap();
    rpc.remove_exit_listener(first_exit);
    assert_eq!(
        json!({"events":*events.lock().unwrap(),"exits":*exits.lock().unwrap()}),
        expected
    );
}

#[tokio::test]
async fn repeated_callback_identity_matches_frozen_set_subscriptions() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
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
        .arg("listener_identity")
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(fake);
    let rpc = RpcProcessInstance::spawn_command(
        &mut command,
        &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
    )
    .unwrap();
    let events = Arc::new(AtomicUsize::new(0));
    let exits = Arc::new(AtomicUsize::new(0));
    let e = events.clone();
    let callback: pi_orchestrator::rpc_process::Listener = Arc::new(move |_| {
        e.fetch_add(1, Ordering::SeqCst);
    });
    let x = exits.clone();
    let exit: pi_orchestrator::rpc_process::ExitListener = Arc::new(move |_| {
        x.fetch_add(1, Ordering::SeqCst);
    });
    let token = rpc.on_event(callback.clone());
    rpc.on_event(callback);
    rpc.on_exit(exit.clone());
    rpc.on_exit(exit);
    rpc.send(json!({"type":"get_state","id":"one"})).await.unwrap();
    let first = events.load(Ordering::SeqCst);
    rpc.remove_event_listener(token);
    rpc.send(json!({"type":"get_state","id":"two"})).await.unwrap();
    rpc.dispose().await.unwrap();
    assert_eq!(
        json!({"first":first,"events":events.load(Ordering::SeqCst),"exits":exits.load(Ordering::SeqCst)}),
        expected
    );
}
