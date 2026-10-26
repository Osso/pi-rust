use super::*;
use pi_ai::request::{HttpBody, HttpResponse};
use pi_js_compat::callback_error::CallbackError;
use std::collections::VecDeque;
use std::sync::Mutex;

struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
#[derive(Default)]
struct FixtureFetch {
    responses: Mutex<VecDeque<(u16, Option<String>, &'static str)>>,
    requests: Mutex<Vec<HttpRequest>>,
}
impl Fetch for FixtureFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let response = self.responses.lock().unwrap().pop_front();
        Box::pin(async move {
            let (status, retry, body) = response.expect("no unexpected HTTP request");
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: retry
                    .map(|value| vec![("retry-after".into(), value)])
                    .unwrap_or_default(),
                body: Box::new(Body(Some(body.as_bytes().to_vec()))),
            })
        })
    }
}
const WINDOWS: &str = r#"{"five_hour":{"utilization":12.5,"resets_at":"2026-10-25T12:00:00+02:00"},"seven_day":{"utilization":40,"resets_at":null}}"#;

fn runtime(fetch: Arc<FixtureFetch>, sleeps: Arc<Mutex<Vec<f64>>>) -> UsageRuntime {
    UsageRuntime {
        fetch,
        now: Arc::new(|| 0.0),
        random: Arc::new(|| Ok(0.5)),
        sleep: Arc::new(move |delay| {
            sleeps.lock().unwrap().push(delay);
            Box::pin(async {})
        }),
    }
}
#[tokio::test]
async fn claude_bridge_usage_retries_transient_responses_with_jitter_and_retry_after() {
    let fetch = Arc::new(FixtureFetch::default());
    fetch
        .responses
        .lock()
        .unwrap()
        .extend([(503, None, ""), (429, Some("1".into()), ""), (200, None, WINDOWS)]);
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let response = fetch_subscription(&"fixture-token".into(), &runtime(fetch.clone(), sleeps.clone()))
        .await
        .unwrap();
    assert_eq!(response, js_json::parse_lossless(WINDOWS).unwrap());
    assert_eq!(*sleeps.lock().unwrap(), vec![375.0, 1000.0]);
    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|r| r.method == "GET" && r.url == USAGE_URL));
    assert_eq!(
        requests[0].headers,
        vec![
            ("Authorization".into(), "Bearer fixture-token".into()),
            ("anthropic-beta".into(), "oauth-2025-04-20".into())
        ]
    );
}
#[tokio::test]
async fn claude_bridge_usage_refreshes_expired_grants_even_when_inference_exits_nonzero() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(
        home.path().join(".claude/a.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"expired","expiresAt":-1}}"#,
    )
    .unwrap();
    std::fs::write(
        home.path().join(".claude/b.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"b-token"}}"#,
    )
    .unwrap();
    let executable = home.path().join("claude");
    std::fs::write(&executable, include_str!("../fixtures/claude.py")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bridge = BridgeProvider::new(
        js_json::parse_lossless(r#"{"accountProfiles":["a","b"]}"#).unwrap(),
        Ok(executable),
        "AskClaude".into(),
    );
    *bridge.accounts.lock().unwrap() = Ok(Some(
        super::super::account_state::AccountManager::new(home.path().into(), vec!["a".into(), "b".into()]).unwrap(),
    ));
    let fetch = Arc::new(FixtureFetch::default());
    fetch
        .responses
        .lock()
        .unwrap()
        .extend([(200, None, WINDOWS), (403, None, "never expose me")]);
    let service = UsageService {
        bridge,
        home: home.path().into(),
        config_dir: None,
        runtime: runtime(fetch.clone(), Arc::new(Mutex::new(Vec::new()))),
    };
    let result = service.all().await.unwrap();
    let expected = js_json::parse_lossless(r#"[{"account":"a","selected":true,"fiveHour":{"percent":12.5,"resetsAt":"2026-10-25T10:00:00.000Z"},"weekly":{"percent":40,"resetsAt":null}},{"account":"b","selected":false,"error":"Claude usage HTTP 403: token not authorized to read usage (inference-only setup-token? full `claude auth login` grants user:profile)"}]"#).unwrap();
    assert_eq!(result, expected);
    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests[0].headers[0].1, "Bearer refreshed-for-usage");
    assert_eq!(requests[1].headers[0].1, "Bearer b-token");
}

#[tokio::test]
async fn claude_bridge_usage_rechecks_exhausted_accounts_before_provider_launch() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(
        home.path().join(".claude/a.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"a-token"}}"#,
    )
    .unwrap();
    let executable = home.path().join("claude");
    std::fs::write(&executable, include_str!("../fixtures/claude.py")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut bridge = BridgeProvider::new(
        js_json::parse_lossless(r#"{"accountProfiles":["a"]}"#).unwrap(),
        Ok(executable),
        "AskClaude".into(),
    );
    let mut manager = super::super::account_state::AccountManager::new(home.path().into(), vec!["a".into()]).unwrap();
    manager.reject("a", None, 0.0).unwrap();
    *bridge.accounts.lock().unwrap() = Ok(Some(manager));
    let fetch = Arc::new(FixtureFetch::default());
    fetch.responses.lock().unwrap().push_back((200, None, WINDOWS));
    let mut test_runtime = runtime(fetch.clone(), Arc::new(Mutex::new(Vec::new())));
    test_runtime.now = Arc::new(|| 1000.0);
    bridge.usage_runtime = test_runtime;
    let model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
    let context: pi_ai::types::Context = js_json::from_value(
        &js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"hello","timestamp":1}]}"#).unwrap(),
    )
    .unwrap();
    let mut options = pi_ai::request::SimpleStreamOptions::default();
    options.stream.cwd = Some(home.path().to_str().unwrap().into());
    let output = bridge
        .stream(model, context, Some(options))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(
        output.stop_reason,
        pi_ai::types::StopReason::Stop,
        "{:?}",
        output.error_message
    );
    assert_eq!(fetch.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn claude_bridge_usage_throttling_permits_one_request_without_false_recovery() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    for name in ["a", "b"] {
        std::fs::write(
            home.path().join(format!(".claude/{name}.credentials.json")),
            r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
        )
        .unwrap();
    }
    let executable = home.path().join("claude");
    std::fs::write(&executable, include_str!("../fixtures/claude.py")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut bridge = BridgeProvider::new(
        js_json::parse_lossless(r#"{"accountProfiles":["a","b"]}"#).unwrap(),
        Ok(executable),
        "AskClaude".into(),
    );
    let mut manager =
        super::super::account_state::AccountManager::new(home.path().into(), vec!["a".into(), "b".into()]).unwrap();
    manager.reject("a", None, 0.0).unwrap();
    manager.reject("b", None, 0.0).unwrap();
    *bridge.accounts.lock().unwrap() = Ok(Some(manager));
    let fetch = Arc::new(FixtureFetch::default());
    fetch
        .responses
        .lock()
        .unwrap()
        .extend([(429, Some("9".into()), ""), (429, Some("9".into()), "")]);
    let mut test_runtime = runtime(fetch.clone(), Arc::new(Mutex::new(Vec::new())));
    test_runtime.now = Arc::new(|| 1000.0);
    bridge.usage_runtime = test_runtime;
    let model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
    let context: pi_ai::types::Context = js_json::from_value(
        &js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"account-quota","timestamp":1}]}"#).unwrap(),
    )
    .unwrap();
    let mut options = pi_ai::request::SimpleStreamOptions::default();
    options.stream.cwd = Some(home.path().to_str().unwrap().into());
    options.stream.session_id = Some("quota-retry".into());
    let first = bridge
        .stream(model.clone(), context.clone(), Some(options.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, pi_ai::types::StopReason::Error);
    assert_ne!(
        first.error_message.as_ref().unwrap(),
        super::super::account_state::ACCOUNTS_EXHAUSTED
    );
    let second = bridge
        .stream(model, context, Some(options))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(
        second.stop_reason,
        pi_ai::types::StopReason::Stop,
        "{:?}",
        second.error_message
    );
    assert_eq!(fetch.requests.lock().unwrap().len(), 2);
    let mut managers = bridge.accounts.lock().unwrap();
    assert_eq!(
        managers
            .as_mut()
            .unwrap()
            .as_mut()
            .unwrap()
            .select(2.0)
            .unwrap()
            .as_deref(),
        Some("b")
    );
}

#[test]
fn claude_bridge_usage_report_has_exact_windows_and_utc_resets() {
    let response = js_json::parse_lossless(WINDOWS).unwrap();
    assert_eq!(
        render_subscription("work", &response).unwrap(),
        "Claude account: work\nSubscription quota (not session token usage)\n5-hour: 12.5% — resets 2026-10-25T10:00:00.000Z (UTC)\n7-day: 40% — reset unavailable\n7-day Opus: unavailable\n7-day Sonnet: unavailable"
    );
    let exhausted = js_json::parse_lossless(
        r#"{"five_hour":{"utilization":1},"seven_day":{"utilization":2},"seven_day_opus":{"utilization":100}}"#,
    )
    .unwrap();
    assert!(!has_capacity(&exhausted, "claude-opus-5-5").unwrap());
    assert!(has_capacity(&exhausted, "claude-haiku-5-5").unwrap());
}

#[tokio::test]
async fn claude_bridge_usage_does_not_retry_permanent_errors_or_exceed_wait_budget() {
    let fetch = Arc::new(FixtureFetch::default());
    fetch
        .responses
        .lock()
        .unwrap()
        .extend([(401, None, "secret"), (429, Some("9".into()), "secret")]);
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let runtime = runtime(fetch.clone(), sleeps.clone());
    let error = fetch_subscription(&"token".into(), &runtime).await.unwrap_err();
    assert_eq!(
        error.message,
        "Claude usage HTTP 401: current account OAuth token expired or rejected"
    );
    let error = fetch_subscription(&"token".into(), &runtime).await.unwrap_err();
    assert_eq!(error.status, Some(429));
    assert_eq!(
        error.message,
        "Claude usage HTTP 429; retry after 9 seconds (1970-01-01T00:00:09.000Z)"
    );
    assert!(sleeps.lock().unwrap().is_empty());
    assert_eq!(fetch.requests.lock().unwrap().len(), 2);
}
