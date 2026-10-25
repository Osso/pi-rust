//! Console/error and timer proofs run in owned processes, without live providers.
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value},
};
use pi_orchestrator::{
    radius::{RadiusOptions, RadiusPresence, RadiusPresenceCoordinator},
    storage::Storage,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move { self.0.take().map(Ok) })
    }
}
struct ScriptedFetch {
    responses: Mutex<VecDeque<(u16, Value)>>,
    requests: Mutex<Vec<Value>>,
}
impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let mut body =
                pi_js_compat::js_json::parse_lossless(std::str::from_utf8(request.body.as_ref().unwrap()).unwrap())
                    .unwrap();
            for key in ["hostname", "platform", "arch", "pid"] {
                body.as_object_mut().unwrap().shift_remove(&JsString::from(key));
            }
            if request.url.contains("/machines/") && request.url.ends_with("/heartbeat") {
                body.as_object_mut().unwrap().shift_remove(&JsString::from("cwd"));
                body.as_object_mut()
                    .unwrap()
                    .shift_remove(&JsString::from("socketPath"));
            }
            self.requests
                .lock()
                .unwrap()
                .push(json!({"url":request.url,"body":body}));
            let (status, data) = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected HTTP request");
            let text = data
                .as_string()
                .map(pi_js_compat::js_json::encode_node_utf8)
                .unwrap_or_else(|| pi_js_compat::js_json::stringify_lossless(&data));
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body(Some(text.into_bytes()))),
            })
        })
    }
}
struct Coordinator {
    instance: Mutex<Option<Value>>,
    updates: Mutex<Vec<Value>>,
}
impl RadiusPresenceCoordinator for Coordinator {
    fn get_live_instance(&self, _: &JsString) -> Option<Value> {
        self.instance.lock().unwrap().clone()
    }
    fn list_live_instances(&self) -> Vec<Value> {
        self.instance.lock().unwrap().clone().into_iter().collect()
    }
    fn update_instance(&self, instance: Value) -> Result<(), JsString> {
        *self.instance.lock().unwrap() = Some(instance.clone());
        self.updates.lock().unwrap().push(instance);
        Ok(())
    }
}
fn registration(id: &str, interval: u64) -> Value {
    json!({"id":id,"heartbeatIntervalMs":interval,"expiresInMs":interval*2})
}
fn responses(scenario: &str) -> VecDeque<(u16, Value)> {
    let machine = scenario.starts_with("machine");
    let mut responses = vec![(200, registration("m1", if machine { 10 } else { 1000000 }))];
    if !machine {
        responses.push((200, registration("p1", 10)));
    }
    if scenario == "random_consumption" {
        responses.extend([(404, json!("gone")), (500, json!("transient"))]);
    } else {
        responses.extend([(404, json!("gone")), (404, json!("gone")), (404, json!("gone"))]);
    }
    match scenario {
        "machine_failure" | "pi_failure" => responses.extend([(503, json!("recover")), (200, json!({}))]),
        "machine_pi_failure" => responses.extend([(200, registration("m2", 10)), (503, json!("recover"))]),
        "pi_disappeared" | "random_consumption" => {}
        _ => panic!("unknown fixture scenario"),
    }
    if !machine {
        responses.push((404, json!("gone")));
    }
    responses.push((404, json!("gone")));
    responses.into()
}
async fn settle_timers() {
    for _ in 0..30 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
#[ignore = "child helper invoked by frozen_radius_failure_matrix"]
async fn radius_failure_child() {
    let scenario = std::env::var("PI_RADIUS_TEST_SCENARIO").unwrap();
    let directory = std::env::var("PI_RADIUS_TEST_DIR").unwrap();
    let fetch = Arc::new(ScriptedFetch {
        responses: Mutex::new(responses(&scenario)),
        requests: Mutex::new(Vec::new()),
    });
    let random_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let random_scenario = scenario == "random_consumption";
    let presence = RadiusPresence::new(
        Storage(directory.into()),
        fetch.clone(),
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "2026-10-21T00:00:00.000Z".into()),
            random: Arc::new(move || {
                if random_scenario {
                    if random_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        0.1
                    } else {
                        0.9
                    }
                } else {
                    0.5
                }
            }),
            hostname: "test".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    let coordinator = Arc::new(Coordinator {
        instance: Mutex::new(Some(json!({"id":"local","cwd":"/tmp","sessionId":"session"}))),
        updates: Mutex::new(Vec::new()),
    });
    let shared: Arc<dyn RadiusPresenceCoordinator> = coordinator.clone();
    presence.set_coordinator(Arc::downgrade(&shared));
    presence.start(Some("desk".into())).await.unwrap();
    let machine = scenario.starts_with("machine");
    let instance = if machine {
        None
    } else {
        let instance = coordinator.instance.lock().unwrap().clone().unwrap();
        let registered = presence.register_pi(instance).await.unwrap();
        *coordinator.instance.lock().unwrap() = Some(registered.clone());
        Some(registered)
    };
    let delays = if scenario == "machine_failure" || scenario == "pi_failure" {
        vec![10, 10, 10, 1125]
    } else if scenario == "random_consumption" {
        vec![10, 10]
    } else {
        vec![10, 10, 10]
    };
    for (index, delay) in delays.into_iter().enumerate() {
        if scenario == "pi_disappeared" && index == 2 {
            *coordinator.instance.lock().unwrap() = None;
        }
        settle_timers().await;
        let before = fetch.requests.lock().unwrap().len();
        tokio::time::advance(std::time::Duration::from_millis(delay - 1)).await;
        settle_timers().await;
        assert_eq!(
            fetch.requests.lock().unwrap().len(),
            before,
            "heartbeat before source deadline"
        );
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        settle_timers().await;
    }
    if scenario == "pi_disappeared" {
        let before = fetch.requests.lock().unwrap().len();
        tokio::time::advance(std::time::Duration::from_millis(1125)).await;
        settle_timers().await;
        assert_eq!(
            fetch.requests.lock().unwrap().len(),
            before,
            "removed Pi must not retry"
        );
    }
    if let Some(instance) = instance {
        presence.disconnect_pi(&instance).await.unwrap();
    }
    presence.stop().await.unwrap();
    assert!(fetch.responses.lock().unwrap().is_empty());
    println!(
        "RESULT {}",
        pi_js_compat::js_json::stringify_lossless(
            &json!({"requests":*fetch.requests.lock().unwrap(),"updates":*coordinator.updates.lock().unwrap()})
        )
    );
}

#[tokio::test]
async fn frozen_radius_failure_matrix() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    for scenario in [
        "machine_failure",
        "machine_pi_failure",
        "pi_failure",
        "pi_disappeared",
        "random_consumption",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let expected = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(root.join("oracle/orchestrator-radius-failures.mjs"))
            .arg(source)
            .arg(scenario)
            .env("PI_ORCHESTRATOR_DIR", directory.path().join("source"))
            .output()
            .await
            .unwrap();
        assert!(
            expected.status.success(),
            "{scenario}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let mut expected =
            pi_js_compat::js_json::parse_lossless(std::str::from_utf8(&expected.stdout).unwrap()).unwrap();
        let errors = expected
            .as_object_mut()
            .unwrap()
            .shift_remove(&JsString::from("errors"))
            .unwrap();
        let schedules = expected
            .as_object_mut()
            .unwrap()
            .shift_remove(&JsString::from("schedules"))
            .unwrap();
        assert!(!schedules.as_array().unwrap().is_empty());
        let actual = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "radius_failure_child", "--nocapture"])
            .env("PI_RADIUS_TEST_SCENARIO", scenario)
            .env("PI_RADIUS_TEST_DIR", directory.path().join("native"))
            .output()
            .await
            .unwrap();
        assert!(
            actual.status.success(),
            "{scenario}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        let actual_stdout = String::from_utf8(actual.stdout).unwrap();
        let result = actual_stdout
            .lines()
            .find_map(|line| line.strip_prefix("RESULT "))
            .expect("child result");
        assert_eq!(
            result,
            pi_js_compat::js_json::stringify_lossless(&expected),
            "{scenario}"
        );
        let expected_errors = errors
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                format!(
                    "{}\n",
                    pi_js_compat::js_json::encode_node_utf8(value.as_string().unwrap())
                )
            })
            .collect::<String>();
        assert_eq!(actual.stderr, expected_errors.into_bytes(), "{scenario}");
    }
}
