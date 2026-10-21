use pi_orchestrator::radius::*;
use serde_json::json;
#[test]
fn credentials_urls_and_bounded_jitter_match_frozen_semantics() {
    let mut env = std::collections::BTreeMap::new();
    assert_eq!(radius_url(&env), "https://radius.pi.dev/");
    assert_eq!(radius_orchestrator_base_url(&env).unwrap(), "https://radius.pi.dev/v1/");
    env.insert("PI_RADIUS_URL".into(), "http://localhost:8888/base/path".into());
    assert_eq!(radius_orchestrator_base_url(&env).unwrap(), "http://localhost:8888/v1/");
    env.insert(
        "PI_RADIUS_ORCHESTRATOR_URL".into(),
        "http://localhost:7777/custom/".into(),
    );
    assert_eq!(
        radius_orchestrator_base_url(&env).unwrap(),
        "http://localhost:7777/custom/"
    );
    assert!(!radius_enabled(None, &env));
    assert!(radius_access_token(None, &env).is_err());
    env.insert("PI_RADIUS_API_KEY".into(), "env-key".into());
    assert_eq!(radius_access_token(None, &env).unwrap(), "env-key");
    assert_eq!(
        radius_access_token(Some(&json!({"type":"oauth","access":"stored"})), &env).unwrap(),
        "stored"
    );
    assert_eq!(
        radius_access_token(Some(&json!({"type":"api_key","access":"wrong"})), &env).unwrap(),
        "env-key"
    );
    assert_eq!(
        (1..=8).map(|n| compute_backoff_delay_ms(n, 0.5)).collect::<Vec<_>>(),
        vec![1125, 2250, 4500, 9000, 18000, 30000, 30000, 30000]
    );
}
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use std::sync::{Arc, Mutex};
struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move { self.0.take().map(Ok) })
    }
}
#[derive(Default)]
struct Fake {
    requests: Mutex<Vec<HttpRequest>>,
    failures: Mutex<std::collections::VecDeque<u16>>,
    interval: Option<u64>,
}
impl Fetch for Fake {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            let pi = request.url.ends_with("pis/register");
            let status = if request.url.ends_with("machines/machine-id/heartbeat") {
                self.failures.lock().unwrap().pop_front().unwrap_or(200)
            } else {
                200
            };
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse{status,status_text:"OK".into(),headers:vec![],body:Box::new(Body(Some(json!({"id":if pi{"pi-id"}else{"machine-id"},"heartbeatIntervalMs":self.interval.unwrap_or(1000000),"expiresInMs":2000000}).to_string().into_bytes())))})
        })
    }
}
#[tokio::test]
async fn fake_http_registration_persistence_and_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let storage = pi_orchestrator::storage::Storage(dir.path().into());
    let fake = Arc::new(Fake::default());
    let presence = RadiusPresence::new(
        storage.clone(),
        fake.clone(),
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "2026-10-21T00:00:00.000Z".into()),
            random: Arc::new(|| 0.5),
            hostname: "test-host".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 7,
        },
    );
    let machine = presence.start(Some("desk".into())).await.unwrap().unwrap();
    assert_eq!(machine["id"], "machine-id");
    assert_eq!(storage.load_machine().unwrap().unwrap(), machine);
    let instance = presence
        .register_pi(json!({"id":"local","cwd":"/tmp","sessionId":"session"}))
        .await
        .unwrap();
    assert_eq!(instance["radiusPiId"], "pi-id");
    presence.disconnect_pi(&instance).await.unwrap();
    presence.stop().await.unwrap();
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].url.ends_with("/v1/machines/register"));
    assert!(requests[1].url.ends_with("/v1/pis/register"));
    assert_eq!(requests[0].headers[0], ("Authorization".into(), "Bearer key".into()));
    let body: serde_json::Value = serde_json::from_slice(requests[1].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["machineId"], "machine-id");
    assert_eq!(body["capabilities"], json!({"rpc":true,"relay":false,"iroh":false}));
}

#[tokio::test]
async fn radius_registration_matches_frozen_fake_fetch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-radius-oracle.mjs"))
        .arg(source)
        .env("PI_ORCHESTRATOR_DIR", dir.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rustdir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let presence = RadiusPresence::new(
        pi_orchestrator::storage::Storage(rustdir.path().into()),
        fake.clone(),
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "2026-10-21T00:00:00.000Z".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let machine = presence.start(Some("desk".into())).await.unwrap();
    let instance = presence
        .register_pi(json!({"id":"local","cwd":"/tmp","sessionId":"session"}))
        .await
        .unwrap();
    presence.disconnect_pi(&instance).await.unwrap();
    presence.stop().await.unwrap();
    let requests = fake
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| {
            let mut body: serde_json::Value = serde_json::from_slice(r.body.as_ref().unwrap()).unwrap();
            for field in ["hostname", "platform", "arch", "pid"] {
                body.as_object_mut().unwrap().remove(field);
            }
            json!({"url":r.url,"body":body,"headers":{"Authorization":"Bearer key","Content-Type":"application/json"}})
        })
        .collect::<Vec<_>>();
    assert_eq!(
        json!({"machine":machine,"instance":instance,"requests":requests}),
        expected
    );
}

struct Coordinator(Mutex<serde_json::Value>);
impl RadiusPresenceCoordinator for Coordinator {
    fn get_live_instance(&self, _id: &str) -> Option<serde_json::Value> {
        Some(self.0.lock().unwrap().clone())
    }
    fn list_live_instances(&self) -> Vec<serde_json::Value> {
        vec![self.0.lock().unwrap().clone()]
    }
    fn update_instance(&self, instance: serde_json::Value) -> Result<(), String> {
        *self.0.lock().unwrap() = instance;
        Ok(())
    }
}
#[tokio::test(start_paused = true)]
async fn three_machine_not_found_heartbeats_re_register_machine_and_live_pis() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake {
        requests: Mutex::new(vec![]),
        failures: Mutex::new([404, 404, 404].into()),
        interval: Some(10),
    });
    let presence = RadiusPresence::new(
        pi_orchestrator::storage::Storage(dir.path().into()),
        fake.clone(),
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "2026-10-21T00:00:00.000Z".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let coordinator: Arc<dyn RadiusPresenceCoordinator> =
        Arc::new(Coordinator(Mutex::new(json!({"id":"live","cwd":"/tmp"}))));
    presence.set_coordinator(Arc::downgrade(&coordinator));
    presence.start(None).await.unwrap();
    for _ in 0..3 {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(std::time::Duration::from_millis(10)).await;
    }
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let urls = fake
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.url.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        urls.iter().filter(|u| u.ends_with("machines/register")).count(),
        2,
        "{urls:?}"
    );
    assert_eq!(
        urls.iter().filter(|u| u.ends_with("pis/register")).count(),
        1,
        "{urls:?}"
    );
    presence.stop().await.unwrap();
}
