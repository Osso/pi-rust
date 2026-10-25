use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use pi_orchestrator::{
    handler::handle_ipc_request,
    radius::{RadiusOptions, RadiusPresence},
    rpc_process::RpcProcessInstance,
    storage::Storage,
    supervisor::*,
};
use std::sync::Arc;
fn create(storage: Storage) -> Arc<OrchestratorSupervisor> {
    let radius = RadiusPresence::new(
        storage.clone(),
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    OrchestratorSupervisor::new(
        storage,
        radius,
        SupervisorOptions {
            now: Arc::new(|| "2026-10-21T00:00:00.000Z".into()),
            id: Arc::new(|| Ok("instance".into())),
            spawn: Arc::new(|cwd| {
                let mut command = tokio::process::Command::new("node");
                command.arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"),
                );
                RpcProcessInstance::spawn_command(&mut command, cwd)
            }),
        },
    )
}
#[tokio::test]
async fn restart_recovery_and_handler_unknown_instance_contract() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage(JsString::from(dir.path().to_str().expect("utf-8 test path")));
    storage
        .save_instances(&[
            json!({"id":"a","status":"online"}),
            json!({"id":"b","status":"starting"}),
            json!({"id":"c","status":"error"}),
        ])
        .unwrap();
    let supervisor = create(storage);
    supervisor.recover_after_restart().await.unwrap();
    let records = supervisor.list_instances().unwrap();
    assert_eq!(records[0]["status"], json!("stopped"));
    assert_eq!(records[1]["status"], json!("stopped"));
    assert_eq!(records[2]["status"], json!("error"));
    assert_eq!(records[0]["lastSeenAt"], json!("2026-10-21T00:00:00.000Z"));
    for kind in ["status", "stop", "rpc", "rpc_stream"] {
        assert_eq!(
            handle_ipc_request(
                &supervisor,
                json!({"type":kind,"instanceId":"missing","command":{"type":"get_state"}})
            )
            .await
            .unwrap(),
            json!({"type":"error","ok":false,"error":"Unknown instance: missing"})
        );
    }
}
#[tokio::test]
async fn spawn_rpc_and_stop_preserve_record_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = create(Storage(JsString::from(
        (dir.path().join("state")).to_str().expect("utf-8 test path"),
    )));
    let instance = supervisor
        .spawn_instance(dir.path().to_str().unwrap().into(), Some("work".into()))
        .await
        .unwrap();
    assert_eq!(instance["status"], json!("online"));
    assert_eq!(instance["sessionId"], json!("session"));
    assert_eq!(instance["label"], json!("work"));
    let response = handle_ipc_request(
        &supervisor,
        json!({"type":"rpc","instanceId":"instance","command":{"type":"get_state","id":"rpc"}}),
    )
    .await
    .unwrap();
    assert_eq!(response["response"]["id"], json!("rpc"));
    assert_eq!(
        handle_ipc_request(&supervisor, json!({"type":"list"})).await.unwrap()["instances"][0]["sessionId"],
        json!("session")
    );
    assert_eq!(
        supervisor
            .stop_instance(&JsString::from("instance"))
            .await
            .unwrap()
            .unwrap()["status"],
        json!("stopped")
    );
    assert!(supervisor.list_instances().unwrap().is_empty());
    assert!(
        supervisor
            .stop_instance(&JsString::from("instance"))
            .await
            .unwrap()
            .is_none()
    );
    supervisor.shutdown().await.unwrap();
}

#[tokio::test]
async fn supervisor_handler_stream_lifecycle_matches_frozen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-supervisor-oracle.mjs"))
        .arg(source)
        .arg(root.join("oracle/orchestrator-rpc-fake.mjs"))
        .arg(dir.path())
        .env("PI_ORCHESTRATOR_DIR", state.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let ruststate = tempfile::tempdir().unwrap();
    let supervisor = create(Storage(JsString::from(
        ruststate.path().to_str().expect("utf-8 test path"),
    )));
    let spawn_result = handle_ipc_request(
        &supervisor,
        json!({
            "type":"spawn", "cwd":dir.path(), "label":"work", "provider":"ignored", "model":"ignored",
        }),
    )
    .await
    .unwrap();
    assert_eq!(spawn_result["type"], json!("spawn_result"));
    let instance = supervisor.get_instance(&"instance".into()).unwrap().unwrap();
    let ready = handle_ipc_request(&supervisor, json!({"type":"rpc_stream","instanceId":"instance"}))
        .await
        .unwrap();
    assert_eq!(ready["type"], json!("rpc_ready"));
    let list = handle_ipc_request(&supervisor, json!({"type":"list"})).await.unwrap();
    let status = handle_ipc_request(&supervisor, json!({"type":"status","instanceId":"instance"}))
        .await
        .unwrap();
    let response = handle_ipc_request(
        &supervisor,
        json!({"type":"rpc","instanceId":"instance","command":{"type":"get_state","id":"rpc"}}),
    )
    .await
    .unwrap();
    let messages = Arc::new(std::sync::Mutex::new(vec![]));
    let m = messages.clone();
    use pi_orchestrator::ipc::server::IpcRequestHandler;
    let handler = pi_orchestrator::handler::OrchestratorHandler(supervisor.clone());
    let stream = handler
        .open_rpc_stream(
            &JsString::from("instance"),
            Arc::new(move |v| m.lock().unwrap().push(v)),
        )
        .unwrap();
    stream
        .handle_request(json!({"type":"abort","id":"stream"}))
        .await
        .unwrap();
    stream
        .handle_request(json!({"type":"extension_ui_response","id":"ui","confirmed":true}))
        .await
        .unwrap();
    stream.close();
    let stop = handle_ipc_request(&supervisor, json!({"type":"stop","instanceId":"instance"}))
        .await
        .unwrap();
    let unknown = handle_ipc_request(&supervisor, json!({"type":"status","instanceId":"missing"}))
        .await
        .unwrap();
    let final_records = supervisor.list_instances().unwrap();
    assert_eq!(
        json!({"spawnResult":spawn_result,"ready":ready,"instance":instance,"list":list,"status":status,"response":response,"messages":*messages.lock().unwrap(),"stop":stop,"unknown":unknown,"final":final_records}),
        expected
    );
}

#[tokio::test]
async fn supervisor_stream_does_not_send_responses_to_session_event_subscribers() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = create(Storage(JsString::from(
        (dir.path().join("state")).to_str().expect("utf-8 test path"),
    )));
    supervisor
        .spawn_instance(dir.path().to_str().unwrap().into(), None)
        .await
        .unwrap();
    let events = Arc::new(std::sync::Mutex::new(vec![]));
    let sink = events.clone();
    let stream = supervisor
        .open_rpc_stream(
            &JsString::from("instance"),
            Arc::new(move |v| sink.lock().unwrap().push(v)),
            Arc::new(|_| {}),
        )
        .unwrap();
    stream.handle_rpc(json!({"type":"abort","id":"a"})).await.unwrap();
    assert_eq!(events.lock().unwrap().len(), 1);
    stream.close();
    supervisor.shutdown().await.unwrap();
}

#[tokio::test]
async fn serve_startup_cleanup_and_idempotent_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let config = pi_orchestrator::config::Config {
        dir: JsString::from((dir.path().join("runtime")).to_str().expect("utf-8 test path")),
    };
    let storage = Storage(config.dir.clone());
    let supervisor = create(storage.clone());
    let radius = RadiusPresence::new(
        storage.clone(),
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let runtime = pi_orchestrator::serve::RunningOrchestrator::start(config.clone(), supervisor, radius)
        .await
        .unwrap();
    assert!(std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
    runtime.shutdown().await.unwrap();
    runtime.shutdown().await.unwrap();
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
    std::fs::write(encode_node_utf8(&config.instances_path()), "{").unwrap();
    let supervisor = create(storage.clone());
    let radius = RadiusPresence::new(
        storage,
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    assert!(
        pi_orchestrator::serve::RunningOrchestrator::start(config.clone(), supervisor, radius)
            .await
            .is_err()
    );
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
}

#[tokio::test]
async fn serve_startup_logs_and_signal_cleanup_match_frozen() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let config = pi_orchestrator::config::Config {
        dir: JsString::from((dir.path().join("runtime")).to_str().expect("utf-8 test path")),
    };
    let mut child = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(source.join("packages/orchestrator/src/cli.ts"))
        .arg("serve")
        .env("PI_ORCHESTRATOR_DIR", encode_node_utf8(&config.dir))
        .env_remove("PI_RADIUS_API_KEY")
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut logs = vec![];
    for _ in 0..2 {
        let mut line = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(30), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        logs.push(line.trim_end().to_string());
    }
    // Frozen serve.ts logs before installing signal handlers; IPC dispatch proves setup returned.
    let ready = pi_orchestrator::ipc::client::send_ipc_request(&config.socket_path(), &json!({"type":"list"}))
        .await
        .expect("frozen serve must process IPC before receiving SIGTERM");
    assert_eq!(ready["type"], json!("list_result"));
    let pid = rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "{status}");
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
    let storage = Storage(config.dir.clone());
    let supervisor = create(storage.clone());
    let radius = RadiusPresence::new(
        storage,
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let runtime = pi_orchestrator::serve::RunningOrchestrator::start(config.clone(), supervisor, radius)
        .await
        .unwrap();
    assert_eq!(runtime.logs, logs);
    runtime.shutdown().await.unwrap();
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
}

#[tokio::test]
async fn failed_spawn_stays_stopped_but_unexpected_child_exit_stays_error() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = create(Storage(JsString::from(
        (dir.path().join("state")).to_str().expect("utf-8 test path"),
    )));
    assert!(
        supervisor
            .spawn_instance(
                dir.path()
                    .join("does-not-exist")
                    .to_str()
                    .expect("utf-8 test path")
                    .into(),
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        supervisor.get_instance(&JsString::from("instance")).unwrap().unwrap()["status"],
        json!("stopped")
    );
    assert!(supervisor.get_live_instance(&JsString::from("instance")).is_none());
    supervisor
        .spawn_instance(dir.path().to_str().unwrap().into(), None)
        .await
        .unwrap();
    assert!(
        supervisor
            .handle_rpc(&JsString::from("instance"), json!({"type":"exit","id":"exit"}))
            .await
            .is_err()
    );
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        supervisor.get_instance(&JsString::from("instance")).unwrap().unwrap()["status"],
        json!("error")
    );
    assert!(supervisor.get_live_instance(&JsString::from("instance")).is_none());
    assert!(
        supervisor
            .stop_instance(&JsString::from("instance"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn malformed_child_json_ends_serve_with_error_and_normal_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let config = pi_orchestrator::config::Config {
        dir: JsString::from((dir.path().join("runtime")).to_str().expect("utf-8 test path")),
    };
    let storage = Storage(config.dir.clone());
    let supervisor = create(storage.clone());
    let radius = RadiusPresence::new(
        storage.clone(),
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let runtime = pi_orchestrator::serve::RunningOrchestrator::start(config.clone(), supervisor.clone(), radius)
        .await
        .unwrap();
    supervisor
        .spawn_instance(dir.path().to_str().unwrap().into(), None)
        .await
        .unwrap();
    let command = supervisor.clone();
    let pending = tokio::spawn(async move {
        command
            .handle_rpc(&JsString::from("instance"), json!({"type":"malformed","id":"bad"}))
            .await
    });
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), runtime.run_until_signal()).await;
    runtime.shutdown().await.unwrap();
    assert!(pending.await.unwrap().unwrap_err().contains("disposed"));
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
    assert!(storage.load_instances().unwrap().is_empty());
    assert!(result.expect("malformed child JSON must terminate serve").is_err());
}

#[tokio::test]
async fn frozen_serve_malformed_child_json_exits_one_and_cleans_up() {
    use pi_orchestrator::ipc::client::send_ipc_request;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let config = pi_orchestrator::config::Config {
        dir: JsString::from((dir.path().join("state")).to_str().expect("utf-8 test path")),
    };
    let mut child = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-fatal-oracle.mjs"))
        .arg(source)
        .arg(root.join("oracle/orchestrator-rpc-fake.mjs"))
        .env("PI_ORCHESTRATOR_DIR", encode_node_utf8(&config.dir))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    for _ in 0..2 {
        let mut line = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(30), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
    }
    let spawn = send_ipc_request(&config.socket_path(), &json!({"type":"spawn","cwd":dir.path()}))
        .await
        .unwrap();
    let id = spawn["instance"]["id"].as_string().unwrap();
    let _outcome = send_ipc_request(
        &config.socket_path(),
        &json!({"type":"rpc","instanceId":id,"command":{"type":"malformed","id":"bad"}}),
    )
    .await;
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(!std::path::Path::new(&encode_node_utf8(&config.socket_path())).exists());
    assert!(Storage(config.dir).load_instances().unwrap().is_empty());
}

#[tokio::test]
async fn shared_stream_callback_identity_and_close_match_frozen_sets() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-supervisor-oracle.mjs"))
        .arg(source)
        .arg(root.join("oracle/orchestrator-rpc-fake.mjs"))
        .arg(dir.path())
        .arg("listener_identity")
        .env("PI_ORCHESTRATOR_DIR", state.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let supervisor = create(Storage(JsString::from(
        (dir.path().join("state")).to_str().expect("utf-8 test path"),
    )));
    supervisor
        .spawn_instance(dir.path().to_str().unwrap().into(), None)
        .await
        .unwrap();
    let events = Arc::new(AtomicUsize::new(0));
    let ui = Arc::new(AtomicUsize::new(0));
    let e = events.clone();
    let callback: pi_orchestrator::rpc_process::Listener = Arc::new(move |_| {
        e.fetch_add(1, Ordering::SeqCst);
    });
    let u = ui.clone();
    let on_ui: pi_orchestrator::rpc_process::Listener = Arc::new(move |_| {
        u.fetch_add(1, Ordering::SeqCst);
    });
    let first = supervisor
        .open_rpc_stream(&JsString::from("instance"), callback.clone(), on_ui.clone())
        .unwrap();
    let second = supervisor
        .open_rpc_stream(&JsString::from("instance"), callback, on_ui)
        .unwrap();
    first.handle_rpc(json!({"type":"get_state","id":"one"})).await.unwrap();
    let before = json!({"events":events.load(Ordering::SeqCst),"ui":ui.load(Ordering::SeqCst)});
    first.close();
    second.handle_rpc(json!({"type":"get_state","id":"two"})).await.unwrap();
    second.close();
    supervisor.shutdown().await.unwrap();
    assert_eq!(
        json!({"before":before,"after":{"events":events.load(Ordering::SeqCst),"ui":ui.load(Ordering::SeqCst)}}),
        expected
    );
}
