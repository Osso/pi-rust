//! Port of `packages/ai/test/github-copilot-oauth.test.ts`.

use pi_js_compat::js_json::JsString;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_ai::auth::oauth::github_copilot::{
    GitHubCopilotOAuth, get_base_url_from_token, normalize_domain, parse_available_copilot_model_ids,
};
use pi_ai::auth::{AuthEvent, OAuthAuth, OAuthCredential};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

use crate::shared_oauth as oauth_support;

use oauth_support::{PromptAnswer, TestInteraction, runtime};

use crate::shared_oracle as oracle;

#[test]
fn legacy_modify_models_filters_present_ids_and_preserves_absent_id_catalog() {
    use pi_js_compat::js_json::{self, JsValue};
    let provider = pi_ai::oauth::get_oauth_provider("github-copilot").expect("built-in legacy provider");
    let mut models = vec![
        pi_ai::compat::get_model("github-copilot", "gpt-5.4").unwrap(),
        pi_ai::compat::get_model("github-copilot", "claude-opus-4.7").unwrap(),
        pi_ai::compat::get_model("anthropic", "claude-sonnet-4-5").unwrap(),
    ];
    models[0].name = JsString::from_utf16(vec![71, 0xd800]);
    let mut source = oracle::TsOracle::spawn(
        "copilot-modify-models-oracle.ts",
        "packages/ai/src/utils/oauth/github-copilot.ts",
    );
    for ids in [None, Some(vec!["gpt-5.4"]), Some(vec![]), Some(vec!["claude-opus-4.7"])] {
        let mut credentials = OAuthCredential::new(
            "refresh",
            "tid=fixture;proxy-ep=proxy.individual.githubcopilot.com;",
            1e15,
        );
        if let Some(ids) = ids {
            credentials
                .extra
                .insert("availableModelIds".into(), js_json::to_value(&ids).unwrap());
        }
        let expected = source.request(
            &pi_js_compat::js_json!({"id":1,"models":js_json::to_value(&models).unwrap(),
            "credentials":js_json::to_value(&pi_ai::auth::Credential::OAuth(credentials.clone())).unwrap()}),
        );
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let actual = provider.modify_models(models.clone(), &credentials).unwrap();
        assert_eq!(
            js_json::to_value(&actual).unwrap(),
            expected["output"],
            "extra={:?}",
            credentials.extra
        );
        assert_eq!(
            actual.last().unwrap(),
            &models[2],
            "unrelated provider remains byte-for-byte unchanged"
        );
    }
}

struct BytesBody {
    bytes: Option<Vec<u8>>,
}

impl HttpBody for BytesBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let bytes = self.bytes.take();
        Box::pin(async move { bytes.map(Ok) })
    }
}

fn response(status: u16, body: String) -> HttpResponse {
    HttpResponse {
        status,
        status_text: String::new(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: Box::new(BytesBody {
            bytes: Some(body.into_bytes()),
        }),
    }
}

struct CopilotFetch {
    device_code: Value,
    access_tokens: Mutex<VecDeque<Value>>,
    models: Value,
    policy_delay: Option<Duration>,
    requests: Mutex<Vec<HttpRequest>>,
    active_policies: AtomicUsize,
    max_active_policies: AtomicUsize,
    policy_count: AtomicUsize,
}

impl CopilotFetch {
    fn new(device_code: Value, access_tokens: Vec<Value>, models: Value) -> Arc<Self> {
        Arc::new(Self {
            device_code,
            access_tokens: Mutex::new(access_tokens.into()),
            models,
            policy_delay: None,
            requests: Mutex::new(Vec::new()),
            active_policies: AtomicUsize::new(0),
            max_active_policies: AtomicUsize::new(0),
            policy_count: AtomicUsize::new(0),
        })
    }

    fn with_policy_delay(mut self: Arc<Self>, delay: Duration) -> Arc<Self> {
        Arc::get_mut(&mut self).expect("unique fetch").policy_delay = Some(delay);
        self
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("requests").clone()
    }
}

impl Fetch for CopilotFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().expect("requests").push(request.clone());
        let url = request.url.clone();
        let fetch = self;
        Box::pin(async move {
            if url.ends_with("/login/device/code") {
                return Ok(response(200, fetch.device_code.to_string()));
            }
            if url.ends_with("/login/oauth/access_token") {
                let next = fetch.access_tokens.lock().expect("tokens").pop_front();
                let body = next.unwrap_or_else(|| json!({"error": "authorization_pending"}));
                return Ok(response(200, body.to_string()));
            }
            if url.contains("/copilot_internal/v2/token") {
                return Ok(response(
                    200,
                    json!({
                        "token": "tid=test;exp=9999999999i64;proxy-ep=proxy.individual.githubcopilot.com;",
                        "expires_at": 9999999999i64
                    })
                    .to_string(),
                ));
            }
            if url.ends_with("/models") {
                return Ok(response(200, fetch.models.to_string()));
            }
            if url.contains("/models/") && url.ends_with("/policy") {
                fetch.policy_count.fetch_add(1, Ordering::SeqCst);
                let active = fetch.active_policies.fetch_add(1, Ordering::SeqCst) + 1;
                fetch.max_active_policies.fetch_max(active, Ordering::SeqCst);
                if let Some(delay) = fetch.policy_delay {
                    tokio::time::sleep(delay).await;
                }
                fetch.active_policies.fetch_sub(1, Ordering::SeqCst);
                return Ok(response(200, String::new()));
            }
            Err(CallbackError::new(format!("Unexpected fetch URL: {url}")))
        })
    }
}

fn device_code(verification_uri: &str, interval: f64, expires_in: f64) -> Value {
    json!({
        "device_code": "device-code",
        "user_code": "ABCD-EFGH",
        "verification_uri": verification_uri,
        "interval": interval,
        "expires_in": expires_in
    })
}

fn oauth(fetch: Arc<CopilotFetch>) -> GitHubCopilotOAuth {
    GitHubCopilotOAuth::with_runtime(runtime(fetch as Arc<dyn Fetch>, 1_772_000_000_000.0))
}

fn refresh_models(fetch: Arc<CopilotFetch>) -> OAuthCredential {
    tokio::runtime::Runtime::new().expect("runtime").block_on(async {
        oauth(fetch)
            .refresh(OAuthCredential::new("ghu_refresh_token", "old-access-token", 0.0))
            .await
            .expect("refresh")
    })
}

fn available_ids(credential: &OAuthCredential) -> Vec<String> {
    credential
        .extra
        .get(&pi_js_compat::js_json::JsString::from("availableModelIds"))
        .and_then(pi_js_compat::js_json::JsValue::as_array)
        .expect("availableModelIds")
        .iter()
        .map(|value| {
            value
                .as_string()
                .expect("string id")
                .to_utf8()
                .expect("scalar fixture id")
        })
        .collect()
}

#[test]
fn refresh_filters_models_to_the_account_picker_catalog() {
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": [
            {"id": "gpt-5.4", "model_picker_enabled": true, "capabilities": {"supports": {"tool_calls": true}}},
            {"id": "claude-opus-4.7", "model_picker_enabled": true, "policy": {"state": "disabled"},
             "capabilities": {"supports": {"tool_calls": true}}},
            {"id": "gpt-5.4-nano", "model_picker_enabled": false, "policy": {"state": "enabled"},
             "capabilities": {"supports": {"tool_calls": true}}}
        ]}),
    );
    let credential = refresh_models(Arc::clone(&fetch));
    assert_eq!(available_ids(&credential), vec!["gpt-5.4".to_owned()]);
    assert_eq!(
        credential.access.to_utf8().unwrap(),
        "tid=test;exp=9999999999i64;proxy-ep=proxy.individual.githubcopilot.com;"
    );
    assert_eq!(credential.expires, 9_999_999_999_000.0 - 300_000.0);

    let requests = fetch.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, "https://api.github.com/copilot_internal/v2/token");
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "Bearer ghu_refresh_token")
    );
    assert_eq!(requests[1].url, "https://api.individual.githubcopilot.com/models");
    assert!(
        requests[1]
            .headers
            .iter()
            .any(|(name, value)| name == "X-GitHub-Api-Version" && value == "2026-06-01")
    );
}

#[test]
fn policy_enabled_models_without_picker_are_unavailable_for_all_accounts() {
    let individual = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": [
            {"id": "gpt-5.4", "model_picker_enabled": false, "policy": {"state": "enabled"},
             "capabilities": {"supports": {"tool_calls": true}}},
            {"id": "claude-opus-4.7", "model_picker_enabled": false, "policy": {"state": "disabled"},
             "capabilities": {"supports": {"tool_calls": true}}},
            {"id": "gpt-5.4-nano", "model_picker_enabled": false,
             "capabilities": {"supports": {"tool_calls": true}}},
            {"id": "gpt-4o", "model_picker_enabled": false, "policy": {"state": "enabled"},
             "capabilities": {"supports": {"tool_calls": false}}}
        ]}),
    );
    assert!(available_ids(&refresh_models(individual)).is_empty());

    // Business and individual accounts use the same picker rule.
    let business = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": [{"id": "gpt-5.4", "model_picker_enabled": false, "policy": {"state": "enabled"}}]}),
    );
    // The token's proxy host decides the endpoint, so override it via a custom token response.
    let business = Arc::new(BusinessFetch {
        inner: business,
        token: "tid=test;exp=9999999999i64;proxy-ep=proxy.business.githubcopilot.com;".to_owned(),
    });
    let credential = tokio::runtime::Runtime::new().expect("runtime").block_on(async {
        oauth_arc(Arc::clone(&business))
            .refresh(OAuthCredential::new("ghu_refresh_token", "old-access-token", 0.0))
            .await
            .expect("refresh")
    });
    assert!(available_ids(&credential).is_empty());
}

/// Wraps [`CopilotFetch`] to serve a business proxy token.
struct BusinessFetch {
    inner: Arc<CopilotFetch>,
    token: String,
}

impl Fetch for BusinessFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        if request.url.contains("/copilot_internal/v2/token") {
            self.inner.requests.lock().expect("requests").push(request);
            let body = json!({"token": self.token, "expires_at": 9999999999i64}).to_string();
            return Box::pin(async move { Ok(response(200, body)) });
        }
        self.inner.fetch(request, signal)
    }
}

fn oauth_arc(fetch: Arc<BusinessFetch>) -> GitHubCopilotOAuth {
    GitHubCopilotOAuth::with_runtime(runtime(fetch as Arc<dyn Fetch>, 1_772_000_000_000.0))
}

#[test]
fn device_code_login_reports_details_and_normalizes_the_verification_uri() {
    let raw_uri = "https://github.com/login/\u{1b}]8;;evil";
    let normalized = url::Url::parse(raw_uri).expect("url").to_string();
    assert_ne!(normalized, raw_uri);

    let fetch = CopilotFetch::new(
        device_code(raw_uri, 1.0, 900.0),
        vec![json!({"access_token": "ghu_refresh_token"})],
        json!({"data": []}),
    );
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(String::new())]);
    let runtime_handle = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .expect("runtime");
    let oauth = oauth(Arc::clone(&fetch));
    runtime_handle.block_on(async {
        let login_interaction = Arc::clone(&interaction);
        let login = tokio::spawn(async move { oauth.login(login_interaction).await });
        tokio::time::advance(Duration::from_millis(0)).await;
        let events = interaction.events();
        assert_eq!(
            events[0],
            AuthEvent::DeviceCode {
                user_code: ("ABCD-EFGH".to_owned()).into(),
                verification_uri: (normalized).into(),
                interval_seconds: Some(1.0),
                expires_in_seconds: Some(900.0),
            }
        );
        tokio::time::advance(Duration::from_millis(1000)).await;
        let credential = login.await.expect("join").expect("login");
        assert_eq!(credential.refresh.to_utf8().unwrap(), "ghu_refresh_token");
        assert_eq!(
            credential
                .extra
                .get(&pi_js_compat::js_json::JsString::from("enterpriseUrl")),
            None
        );
        assert_eq!(
            interaction.events()[1],
            AuthEvent::Progress {
                message: ("Enabling models...".to_owned()).into()
            }
        );
    });
}

#[test]
fn rejects_a_non_http_verification_uri_before_notifying() {
    let fetch = CopilotFetch::new(
        device_code("$(id>/tmp/pwned)", 1.0, 900.0),
        Vec::new(),
        json!({"data": []}),
    );
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(String::new())]);
    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(oauth(fetch).login(interaction.clone()))
        .expect_err("untrusted uri");
    assert!(
        error
            .detail()
            .is_some_and(|message| message.contains("Untrusted verification_uri")),
        "{error}"
    );
    assert!(interaction.events().is_empty());
}

#[test]
fn waits_before_polling_and_honors_slow_down_intervals() {
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 5.0, 900.0),
        vec![
            json!({"error": "authorization_pending", "error_description": "pending"}),
            json!({"error": "slow_down", "error_description": "slow down", "interval": 7}),
            json!({"access_token": "ghu_refresh_token"}),
        ],
        json!({"data": []}),
    );
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(String::new())]);
    let runtime_handle = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .expect("runtime");
    let oauth = oauth(Arc::clone(&fetch));
    runtime_handle.block_on(async {
        let login = tokio::spawn(async move { oauth.login(interaction).await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(0)).await;
        assert_eq!(access_token_polls(&fetch), 0);
        tokio::time::advance(Duration::from_millis(4999)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 0);
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 1);
        tokio::time::advance(Duration::from_millis(4999)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 1);
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 2);
        tokio::time::advance(Duration::from_millis(6999)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 2);
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        login.await.expect("join").expect("login");
        assert_eq!(access_token_polls(&fetch), 3);
    });
}

fn access_token_polls(fetch: &Arc<CopilotFetch>) -> usize {
    fetch
        .requests()
        .iter()
        .filter(|request| request.url.ends_with("/login/oauth/access_token"))
        .count()
}

#[test]
fn times_out_after_repeated_slow_down_responses() {
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 5.0, 25.0),
        vec![
            json!({"error": "slow_down", "error_description": "slow down"}),
            json!({"error": "slow_down", "error_description": "still too fast"}),
            json!({"error": "authorization_pending", "error_description": "pending"}),
        ],
        json!({"data": []}),
    );
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(String::new())]);
    let runtime_handle = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .expect("runtime");
    let oauth = oauth(Arc::clone(&fetch));
    runtime_handle.block_on(async {
        let login = tokio::spawn(async move { oauth.login(interaction).await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(0)).await;
        tokio::time::advance(Duration::from_millis(5000)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 1);
        tokio::time::advance(Duration::from_millis(10000)).await;
        tokio::task::yield_now().await;
        assert_eq!(access_token_polls(&fetch), 2);
        tokio::time::advance(Duration::from_millis(10000)).await;
        tokio::task::yield_now().await;
        let error = login.await.expect("join").expect_err("timeout");
        assert!(
            error
                .detail()
                .is_some_and(|message| message.contains("Device flow timed out after one or more slow_down")),
            "{error}"
        );
        assert_eq!(access_token_polls(&fetch), 2);
    });
}

#[tokio::test]
async fn starts_all_model_policy_updates_during_login_like_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "copilot-policy-oracle.mts",
        "packages/ai/src/utils/oauth/github-copilot.ts",
    );
    let expected = source.request(&json!({"id": 1}));
    assert_eq!(expected["ok"], true, "{expected}");
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        vec![json!({"access_token": "ghu_refresh_token"})],
        json!({"data": []}),
    )
    .with_policy_delay(Duration::from_millis(10));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(String::new())]);
    oauth(Arc::clone(&fetch)).login(interaction).await.expect("login");

    assert_eq!(
        json!({
            "count": fetch.policy_count.load(Ordering::SeqCst),
            "maximum": fetch.max_active_policies.load(Ordering::SeqCst),
        }),
        expected["output"],
    );
}

#[test]
fn rejects_invalid_enterprise_domains() {
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": []}),
    );
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("not a url".to_owned())]);
    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(oauth(fetch).login(interaction))
        .expect_err("invalid domain");
    assert_eq!(error.detail(), Some(&"Invalid GitHub Enterprise URL/domain".into()));
}

#[test]
fn normalizes_domains_and_derives_base_urls() {
    assert_eq!(
        normalize_domain(&JsString::from("  github.com ")),
        Some("github.com".to_owned())
    );
    assert_eq!(
        normalize_domain(&JsString::from("https://Company.GHE.com/path")),
        Some("company.ghe.com".to_owned())
    );
    assert_eq!(normalize_domain(&JsString::from("")), None);
    assert_eq!(normalize_domain(&JsString::from("not a url")), None);

    assert_eq!(
        get_base_url_from_token(&JsString::from(
            "tid=1;exp=2;proxy-ep=proxy.individual.githubcopilot.com;"
        )),
        Some(JsString::from("https://api.individual.githubcopilot.com".to_owned()))
    );
    assert_eq!(get_base_url_from_token(&JsString::from("no proxy here")), None);
}

#[tokio::test]
async fn to_auth_derives_the_proxy_endpoint_from_the_access_token() {
    let fetch = CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": []}),
    );
    let mut credential = OAuthCredential::new(
        "refresh",
        "tid=test;exp=9999999999i64;proxy-ep=proxy.business.githubcopilot.com;",
        0.0,
    );
    credential.extra.insert(
        "enterpriseUrl".into(),
        Value::String("company.ghe.com".to_owned()).into(),
    );
    let auth = oauth(fetch).to_auth(credential).await.expect("to auth");
    assert_eq!(
        auth.api_key.as_ref(),
        Some(&JsString::from(
            "tid=test;exp=9999999999i64;proxy-ep=proxy.business.githubcopilot.com;"
        ))
    );
    assert_eq!(
        auth.base_url.as_ref(),
        Some(&JsString::from("https://api.business.githubcopilot.com"))
    );

    let fallback = oauth(CopilotFetch::new(
        device_code("https://github.com/login/device", 1.0, 900.0),
        Vec::new(),
        json!({"data": []}),
    ))
    .to_auth(OAuthCredential::new("refresh", "plain-token", 0.0))
    .await
    .expect("to auth");
    assert_eq!(
        fallback.base_url.as_ref(),
        Some(&JsString::from("https://api.individual.githubcopilot.com"))
    );
}

#[test]
fn parses_picker_ids_with_the_source_rules() {
    let picker = parse_available_copilot_model_ids(&json!({"data": [
        {"id": "a", "model_picker_enabled": true},
        {"id": "b", "model_picker_enabled": true, "policy": {"state": "disabled"}},
        {"id": "c", "model_picker_enabled": false, "policy": {"state": "enabled"}},
        {"id": "d", "model_picker_enabled": true, "capabilities": {"supports": {"tool_calls": false}}},
        {"model_picker_enabled": true},
        "not-an-object"
    ]}))
    .expect("parse");
    assert_eq!(picker, vec!["a".to_owned()]);

    let policy = parse_available_copilot_model_ids(
        &json!({"data": [{"id": "c", "model_picker_enabled": false, "policy": {"state": "enabled"}}]}),
    )
    .expect("parse");
    assert!(policy.is_empty());

    assert_eq!(
        parse_available_copilot_model_ids(&json!({})).expect_err("invalid"),
        "Invalid Copilot models response"
    );
}
