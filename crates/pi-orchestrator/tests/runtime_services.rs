//! Isolated enabled-Radius service lifecycle; no external network or providers.
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, encode_node_utf8},
};
use pi_orchestrator::{
    config::Config,
    ipc::client::send_ipc_request,
    radius::{RadiusOptions, RadiusPresence},
    rpc_process::RpcProcessInstance,
    serve::RunningOrchestrator,
    storage::Storage,
    supervisor::{OrchestratorSupervisor, SupervisorOptions},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct ResponseBody(Option<Vec<u8>>);
impl HttpBody for ResponseBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move { self.0.take().map(Ok) })
    }
}

#[derive(Default)]
struct RadiusFixture {
    requests: Mutex<Vec<HttpRequest>>,
    reject_pi_disconnect: AtomicBool,
}
impl Fetch for RadiusFixture {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let disconnect_fails =
                request.url.ends_with("pis/pi-id/disconnect") && self.reject_pi_disconnect.load(Ordering::SeqCst);
            let id = if request.url.ends_with("pis/register") {
                "pi-id"
            } else {
                "machine-id"
            };
            self.requests.lock().unwrap().push(request);
            let body = if disconnect_fails {
                "offline".to_owned()
            } else {
                json::stringify_lossless(&json!({"id":id,"heartbeatIntervalMs":600_000}))
            };
            Ok(HttpResponse {
                status: if disconnect_fails { 503 } else { 200 },
                status_text: String::new(),
                headers: vec![],
                body: Box::new(ResponseBody(Some(body.into_bytes()))),
            })
        })
    }
}

struct ServiceFixture {
    directory: tempfile::TempDir,
    config: Config,
    radius: Arc<RadiusPresence>,
    fetch: Arc<RadiusFixture>,
    supervisor: Arc<OrchestratorSupervisor>,
    process: Arc<Mutex<Option<RpcProcessInstance>>>,
}
impl ServiceFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config = Config {
            dir: directory.path().join("state").to_str().unwrap().into(),
        };
        let storage = Storage(config.dir.clone());
        let fetch = Arc::new(RadiusFixture::default());
        let radius = RadiusPresence::new(
            storage.clone(),
            fetch.clone(),
            RadiusOptions {
                env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "test-key".into())].into()),
                credential: Arc::new(|| None),
                now: Arc::new(|| "2026-10-25T00:00:00.000Z".into()),
                random: Arc::new(|| 0.5),
                hostname: "fixture".into(),
                platform: "linux".into(),
                arch: "x64".into(),
                pid: 1,
            },
        );
        let process = Arc::new(Mutex::new(None));
        let capture = process.clone();
        let supervisor = OrchestratorSupervisor::new(
            storage,
            radius.clone(),
            SupervisorOptions {
                now: Arc::new(|| "2026-10-25T00:00:00.000Z".into()),
                id: Arc::new(|| Ok("instance".into())),
                spawn: Arc::new(move |cwd| {
                    let mut command = tokio::process::Command::new("node");
                    command.arg(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/orchestrator-rpc-fake.mjs"),
                    );
                    let process = RpcProcessInstance::spawn_command(&mut command, cwd)?;
                    *capture.lock().unwrap() = Some(process.clone());
                    Ok(process)
                }),
            },
        );
        Self {
            directory,
            config,
            radius,
            fetch,
            supervisor,
            process,
        }
    }

    async fn start(&self) -> RunningOrchestrator {
        RunningOrchestrator::start(self.config.clone(), self.supervisor.clone(), self.radius.clone())
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn enabled_radius_serve_registers_serves_ipc_and_disconnects_in_order() {
    let fixture = ServiceFixture::new();
    let runtime = fixture.start().await;
    assert_eq!(
        runtime.logs,
        vec![
            pi_js_compat::js_format!(
                "radius integration enabled: {} -> https://radius.pi.dev/v1/",
                fixture.config.socket_path()
            ),
            JsString::from("radius machine id: machine-id"),
            pi_js_compat::js_format!("orchestrator listening on {}", fixture.config.socket_path()),
        ]
    );
    let storage = Storage(fixture.config.dir.clone());
    assert_eq!(storage.load_machine().unwrap().unwrap()["id"], json!("machine-id"));
    let created = send_ipc_request(
        &fixture.config.socket_path(),
        &json!({"type":"spawn","cwd":fixture.directory.path(),"label":"work"}),
    )
    .await
    .unwrap();
    assert_eq!(created["instance"]["radiusPiId"], json!("pi-id"));
    assert_eq!(created["instance"]["status"], json!("online"));
    assert_eq!(created["instance"]["sessionId"], json!("session"));
    let listed = send_ipc_request(&fixture.config.socket_path(), &json!({"type":"list"}))
        .await
        .unwrap();
    assert_eq!(listed["instances"], json!([created["instance"].clone()]));
    runtime.shutdown().await.unwrap();
    runtime.shutdown().await.unwrap();
    assert!(storage.load_instances().unwrap().is_empty());
    assert!(!std::path::Path::new(&encode_node_utf8(&fixture.config.socket_path())).exists());
    let requests = fixture.fetch.requests.lock().unwrap();
    assert_eq!(
        requests.iter().map(|request| request.url.as_str()).collect::<Vec<_>>(),
        vec![
            "https://radius.pi.dev/v1/machines/register",
            "https://radius.pi.dev/v1/pis/register",
            "https://radius.pi.dev/v1/pis/pi-id/disconnect",
            "https://radius.pi.dev/v1/machines/machine-id/disconnect",
        ]
    );
    let registration: JsValue =
        json::parse_lossless(std::str::from_utf8(requests[1].body.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(registration["machineId"], json!("machine-id"));
    assert_eq!(registration["sessionId"], json!("session"));
    assert_eq!(registration["label"], json!("work"));
}

#[tokio::test]
async fn radius_disconnect_failure_preserves_frozen_stop_finally_and_skipped_rpc_disposal() {
    let fixture = ServiceFixture::new();
    let runtime = fixture.start().await;
    fixture
        .supervisor
        .spawn_instance(fixture.directory.path().to_str().unwrap().into(), None)
        .await
        .unwrap();
    let process = fixture.process.lock().unwrap().clone().unwrap();
    fixture.fetch.reject_pi_disconnect.store(true, Ordering::SeqCst);
    let error = fixture.supervisor.stop_instance(&"instance".into()).await.unwrap_err();
    // Frozen cleanup throws before dispose, but stopInstance's finally removes the live/stored record.
    assert_eq!(error, "Radius request failed: 503 offline");
    assert!(fixture.supervisor.get_live_instance(&"instance".into()).is_none());
    assert!(Storage(fixture.config.dir.clone()).load_instances().unwrap().is_empty());
    let still_running = process
        .send(json!({"type":"get_state","id":"after-stop"}))
        .await
        .unwrap();
    assert_eq!(still_running["data"]["sessionId"], json!("session"));
    // Test-owned cleanup, not product recovery: retain the source bug without leaving an orphan.
    process.dispose().await.unwrap();
    fixture.fetch.reject_pi_disconnect.store(false, Ordering::SeqCst);
    runtime.shutdown().await.unwrap();
}
