//! Frozen supervisor.ts:270-304, storage.ts:35-67 and ipc/protocol.ts:129-140.
//! Native JS strings must survive lifecycle storage and actual JSONL socket traffic.
use pi_js_compat::{
    js_json,
    js_json::{JsString, encode_node_utf8, parse_lossless},
};
use pi_orchestrator::{
    handler::OrchestratorHandler,
    ipc::{client::send_ipc_request, server::start_ipc_server},
    radius::{RadiusOptions, RadiusPresence},
    rpc_process::RpcProcessInstance,
    storage::Storage,
    supervisor::{OrchestratorSupervisor, SupervisorOptions},
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn lone_surrogate_cwd_label_and_id_survive_spawn_persist_list_over_real_ipc() {
    let root = tempfile::tempdir().expect("fixture directory");
    let native_root = JsString::from(root.path().to_str().expect("utf-8 test path"));
    let cwd = pi_coding_agent::utils::paths::node_join(&native_root, JsString::from_utf16(vec![0xd800]));
    // Node's OS boundary encodes an unpaired unit as U+FFFD, without changing the JS record.
    std::fs::create_dir(encode_node_utf8(&cwd)).expect("encoded child cwd");
    let label = JsString::from_utf16(vec![b'L' as u16, 0xdc00]);
    let id = JsString::from_utf16(vec![b'I' as u16, 0xd800]);
    let storage = Storage(pi_coding_agent::utils::paths::node_join(&native_root, "state"));
    let radius = RadiusPresence::new(
        storage.clone(),
        pi_ai::http::default_fetch(),
        RadiusOptions {
            env: Arc::new(Default::default),
            credential: Arc::new(|| None),
            now: Arc::new(|| "2026-10-22T00:00:00.000Z".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let observed_cwd = Arc::new(Mutex::new(None));
    let spawn_cwd = observed_cwd.clone();
    let instance_id = id.clone();
    let supervisor = OrchestratorSupervisor::new(
        storage.clone(),
        radius,
        SupervisorOptions {
            now: Arc::new(|| "2026-10-22T00:00:00.000Z".into()),
            id: Arc::new(move || Ok(instance_id.clone())),
            spawn: Arc::new(move |cwd| {
                *spawn_cwd.lock().expect("observed cwd lock") = Some(cwd.clone());
                let mut command = tokio::process::Command::new("node");
                command.arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"),
                );
                RpcProcessInstance::spawn_command(&mut command, cwd)
            }),
        },
    );
    let socket = pi_coding_agent::utils::paths::node_join(&native_root, "ipc.sock");
    let server = start_ipc_server(&socket, Arc::new(OrchestratorHandler(supervisor.clone())))
        .await
        .expect("start real IPC server");
    let spawned = send_ipc_request(&socket, &js_json!({"type":"spawn", "cwd":cwd, "label":label}))
        .await
        .expect("spawn over IPC");
    assert_eq!(spawned["ok"], true);
    assert_eq!(spawned["instance"]["id"], js_json!(id));
    assert_eq!(spawned["instance"]["cwd"], js_json!(cwd));
    assert_eq!(spawned["instance"]["label"], js_json!(label));
    assert_eq!(observed_cwd.lock().expect("observed cwd lock").as_ref(), Some(&cwd));

    let persisted_bytes = std::fs::read(encode_node_utf8(&pi_coding_agent::utils::paths::node_join(
        &storage.0,
        "instances.json",
    )))
    .expect("persisted instance bytes");
    let persisted =
        parse_lossless(String::from_utf8(persisted_bytes).expect("JSON UTF-8 wire")).expect("lossless persisted JSON");
    let listed = send_ipc_request(&socket, &js_json!({"type":"list"}))
        .await
        .expect("list over IPC");
    assert_eq!(listed["ok"], true);
    for field in ["id", "cwd", "label"] {
        assert_eq!(persisted[0][field], spawned["instance"][field], "persisted {field}");
        assert_eq!(
            listed["instances"][0][field], spawned["instance"][field],
            "listed {field}"
        );
    }
    let stopped = send_ipc_request(&socket, &js_json!({"type":"stop", "instanceId":id}))
        .await
        .expect("stop by lossless id over IPC");
    assert_eq!(stopped["ok"], true);
    assert!(storage.load_instances().expect("read final persistence").is_empty());
    server.close().await;
    supervisor.shutdown().await.expect("shutdown child");
}
