use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use pi_orchestrator::radius::*;
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
        _signal: Option<pi_js_compat::abort::AbortSignal>,
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
            Ok(HttpResponse{status,status_text:"OK".into(),headers:vec![],body:Box::new(Body(Some(pi_js_compat::js_json::stringify_lossless(&json!({"id":if pi{"pi-id"}else{"machine-id"},"heartbeatIntervalMs":self.interval.unwrap_or(1000000),"expiresInMs":2000000})).into_bytes())))})
        })
    }
}
#[tokio::test]
async fn fake_http_registration_persistence_and_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let storage = pi_orchestrator::storage::Storage(JsString::from(dir.path().to_str().expect("utf-8 test path")));
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
    assert_eq!(machine["id"], json!("machine-id"));
    assert_eq!(storage.load_machine().unwrap().unwrap(), machine);
    let instance = presence
        .register_pi(json!({"id":"local","cwd":"/tmp","sessionId":"session"}))
        .await
        .unwrap();
    assert_eq!(instance["radiusPiId"], json!("pi-id"));
    presence.disconnect_pi(&instance).await.unwrap();
    presence.stop().await.unwrap();
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].url.ends_with("/v1/machines/register"));
    assert!(requests[1].url.ends_with("/v1/pis/register"));
    assert_eq!(requests[0].headers[0], ("Authorization".into(), "Bearer key".into()));
    let body: Value = serde_json::from_slice(requests[1].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["machineId"], json!("machine-id"));
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
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rustdir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let presence = RadiusPresence::new(
        pi_orchestrator::storage::Storage(JsString::from(rustdir.path().to_str().expect("utf-8 test path"))),
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
        .register_pi(json!({"id":"local","cwd":"/tmp","label":"work","sessionId":"session"}))
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
            let mut body: Value = serde_json::from_slice(r.body.as_ref().unwrap()).unwrap();
            for field in ["hostname", "platform", "arch", "pid"] {
                body.as_object_mut().unwrap().shift_remove(&JsString::from(field));
            }
            json!({"url":r.url,"body":body,"headers":{"Authorization":"Bearer key","Content-Type":"application/json"}})
        })
        .collect::<Vec<_>>();
    let actual = json!({"machine":machine,"instance":instance,"requests":requests});
    assert_eq!(actual, expected);
    assert_eq!(
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

#[tokio::test]
async fn invalid_radius_urls_keep_frozen_node_error_message() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    for (mode, key) in [("base", "PI_RADIUS_URL"), ("explicit", "PI_RADIUS_ORCHESTRATOR_URL")] {
        let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(root.join("oracle/orchestrator-radius-url-error.mjs"))
            .arg(source)
            .arg(mode)
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        let expected: String = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(expected, "Invalid URL");
        let dir = tempfile::tempdir().unwrap();
        let presence = RadiusPresence::new(
            pi_orchestrator::storage::Storage(dir.path().to_str().unwrap().into()),
            Arc::new(Fake::default()),
            RadiusOptions {
                env: Arc::new(move || {
                    [
                        ("PI_RADIUS_API_KEY".into(), "key".into()),
                        (key.into(), "not a URL".into()),
                    ]
                    .into()
                }),
                credential: Arc::new(|| None),
                now: Arc::new(|| "time".into()),
                random: Arc::new(|| 0.5),
                hostname: "test".into(),
                platform: "linux".into(),
                arch: "x64".into(),
                pid: 1,
            },
        );
        assert_eq!(presence.start(None).await.unwrap_err(), JsString::from(expected));
    }
}

struct Coordinator(Mutex<Value>);
impl RadiusPresenceCoordinator for Coordinator {
    fn get_live_instance(&self, _id: &JsString) -> Option<Value> {
        Some(self.0.lock().unwrap().clone())
    }
    fn list_live_instances(&self) -> Vec<Value> {
        vec![self.0.lock().unwrap().clone()]
    }
    fn update_instance(&self, instance: Value) -> Result<(), JsString> {
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
        pi_orchestrator::storage::Storage(JsString::from(dir.path().to_str().expect("utf-8 test path"))),
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

struct ScriptedFetch {
    sequence: Mutex<std::collections::VecDeque<(u16, Value)>>,
    requests: Mutex<Vec<Value>>,
}
impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            let mut body: Value = serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
            for key in ["hostname", "platform", "arch", "pid"] {
                body.as_object_mut().unwrap().shift_remove(&JsString::from(key));
            }
            self.requests
                .lock()
                .unwrap()
                .push(json!({"url":request.url,"body":body}));
            let (status, data) = self
                .sequence
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra HTTP request");
            let bytes = if let Some(text) = data.as_string() {
                encode_node_utf8(text).into_bytes()
            } else {
                pi_js_compat::js_json::stringify_lossless(&data).into_bytes()
            };
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body(Some(bytes))),
            })
        })
    }
}
#[tokio::test(start_paused = true)]
async fn frozen_radius_machine_and_pi_retry_recovery_matrices() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    for scenario in ["machine", "pi"] {
        let dir = tempfile::tempdir().unwrap();
        let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(root.join("oracle/orchestrator-radius-retry-oracle.mjs"))
            .arg(source)
            .arg(scenario)
            .env("PI_ORCHESTRATOR_DIR", dir.path())
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let mut expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let registration =
            |id: &str, interval: u64| json!({"id":id,"heartbeatIntervalMs":interval,"expiresInMs":interval*2});
        let sequence = if scenario == "machine" {
            vec![
                (200, registration("m1", 10)),
                (500, json!("oops")),
                (500, json!("oops")),
                (200, json!({})),
                (404, json!("gone")),
                (404, json!("gone")),
                (404, json!("gone")),
                (200, registration("m2", 10)),
                (200, registration("p1", 10)),
                (200, json!({})),
            ]
        } else {
            vec![
                (200, registration("m1", 1000000)),
                (200, registration("p1", 10)),
                (500, json!("oops")),
                (200, json!({})),
                (404, json!("gone")),
                (404, json!("gone")),
                (404, json!("gone")),
                (200, registration("p2", 10)),
                (200, json!({})),
                (200, json!({})),
            ]
        };
        let fake = Arc::new(ScriptedFetch {
            sequence: Mutex::new(sequence.into()),
            requests: Mutex::new(vec![]),
        });
        let storage = pi_orchestrator::storage::Storage(JsString::from(
            (dir.path().join("rust")).to_str().expect("utf-8 test path"),
        ));
        let presence = RadiusPresence::new(
            storage,
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
        let coordinator = Arc::new(Coordinator(Mutex::new(
            json!({"id":"local","cwd":"/tmp","sessionId":"session"}),
        )));
        let dyn_coordinator: Arc<dyn RadiusPresenceCoordinator> = coordinator.clone();
        presence.set_coordinator(Arc::downgrade(&dyn_coordinator));
        presence.start(Some("desk".into())).await.unwrap();
        if scenario == "pi" {
            let record = coordinator.0.lock().unwrap().clone();
            let instance = presence.register_pi(record).await.unwrap();
            *coordinator.0.lock().unwrap() = instance;
        }
        let delays = if scenario == "machine" {
            vec![10, 1125, 2250, 10, 10, 10]
        } else {
            vec![10, 1125, 10, 10, 10]
        };
        for delay in delays {
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            let before = fake.requests.lock().unwrap().len();
            tokio::time::advance(std::time::Duration::from_millis(delay - 1)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(
                fake.requests.lock().unwrap().len(),
                before,
                "{scenario}: fired before frozen deadline"
            );
            tokio::time::advance(std::time::Duration::from_millis(1)).await;
            for _ in 0..30 {
                tokio::task::yield_now().await;
            }
        }
        if scenario == "pi" {
            let instance = coordinator.0.lock().unwrap().clone();
            presence.disconnect_pi(&instance).await.unwrap();
        }
        presence.stop().await.unwrap();
        let mut actual_requests = fake.requests.lock().unwrap().clone();
        for requests in [&mut actual_requests, expected["requests"].as_array_mut().unwrap()] {
            for request in requests {
                if request["url"].as_string().unwrap().contains("/machines/")
                    && request["url"].as_string().unwrap().ends_with("/heartbeat")
                {
                    let body = request["body"].as_object_mut().unwrap();
                    body.shift_remove(&JsString::from("cwd"));
                    body.shift_remove(&JsString::from("socketPath"));
                }
            }
        }
        assert_eq!(json!(actual_requests), expected["requests"], "{scenario}");
        assert!(fake.sequence.lock().unwrap().is_empty());
        assert_eq!(
            coordinator.0.lock().unwrap()["radiusPiId"],
            expected["updates"][0]["radiusPiId"]
        );
    }
}

#[tokio::test]
async fn cached_machine_identity_does_not_read_replaced_disk_record() {
    let dir = tempfile::tempdir().unwrap();
    let storage = pi_orchestrator::storage::Storage(JsString::from(dir.path().to_str().expect("utf-8 test path")));
    let fake = Arc::new(Fake::default());
    let presence = RadiusPresence::new(
        storage,
        fake,
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    presence.start(None).await.unwrap();
    std::fs::write(dir.path().join("machine.json"), "{").unwrap();
    let result = presence.register_pi(json!({"id":"i","cwd":"/tmp"})).await;
    presence.stop().await.unwrap();
    assert_eq!(result.unwrap()["radiusPiId"], json!("pi-id"));
}
