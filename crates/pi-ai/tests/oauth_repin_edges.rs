//! Native regressions found by the whole-module common-pin OAuth audit.
use crate::shared_oauth as support;
use pi_ai::{
    auth::oauth::{
        anthropic::AnthropicOAuth,
        github_copilot::{GitHubCopilotOAuth, get_base_url_from_token},
        openai_codex::OpenAICodexOAuth,
        support::parse_authorization_input,
    },
    auth::{AuthError, OAuthAuth, OAuthCredential},
    request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse},
};
use pi_js_compat::{
    abort::{AbortReason, AbortSignal},
    js_json::{JsString, JsValue},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use support::recording_fetch::{RecordingFetch, ScriptedResponse};
use support::{PromptAnswer, TestInteraction, lock_callback_port, runtime};

#[test]
fn authorization_parser_uses_first_query_values_and_only_two_hash_segments() {
    for input in [
        "http://localhost/callback?code=first&code=last&state=one&state=two",
        "code=first&code=last&state=one&state=two",
        "first#one#ignored",
    ] {
        let parsed = parse_authorization_input(&input.into());
        assert_eq!(parsed.code, Some("first".into()), "{input}");
        assert_eq!(parsed.state, Some("one".into()), "{input}");
    }
}

#[test]
fn copilot_proxy_pattern_skips_empty_matches() {
    assert_eq!(get_base_url_from_token(&"proxy-ep=;other=x".into()), None);
    assert_eq!(
        get_base_url_from_token(&"proxy-ep=;proxy-ep=proxy.example.com".into()),
        Some("https://api.example.com".into())
    );
}

#[tokio::test]
async fn anthropic_empty_manual_input_uses_the_text_prompt() {
    let _port = lock_callback_port().await;
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"access_token":"a","refresh_token":"r","expires_in":60}"#,
    )]);
    let interaction = TestInteraction::new(vec![
        PromptAnswer::Text(String::new()),
        PromptAnswer::Text("code".into()),
    ]);
    let credential = AnthropicOAuth::with_runtime(runtime(fetch, 1000.0))
        .login(interaction.clone())
        .await
        .unwrap();
    assert_eq!(credential.access, "a");
    assert_eq!(interaction.prompts().len(), 2);
    assert!(
        matches!(&interaction.prompts()[1].kind, pi_ai::auth::AuthPromptKind::Text { message, placeholder } if message == "Paste the authorization code or full redirect URL:" && placeholder.as_ref() == Some(&JsString::from("http://localhost:53692/callback")))
    );
}

#[tokio::test]
async fn codex_empty_manual_input_uses_the_text_prompt() {
    let _port = lock_callback_port().await;
    let interaction = TestInteraction::new(vec![
        PromptAnswer::Text("browser".into()),
        PromptAnswer::Text(String::new()),
        PromptAnswer::Error("text prompt rejected".into()),
    ]);
    let error = OpenAICodexOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), 1000.0))
        .login(interaction.clone())
        .await
        .unwrap_err();
    assert_eq!(error.detail(), Some(&JsString::from("text prompt rejected")));
    assert_eq!(interaction.prompts().len(), 3);
}

struct RejectPoll {
    first: Arc<RecordingFetch>,
    calls: AtomicUsize,
    rejection: CallbackError,
}
impl Fetch for RejectPoll {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.fetch(request, signal)
        } else {
            let error = self.rejection.clone();
            Box::pin(async move { Err(error) })
        }
    }
}
#[tokio::test(start_paused = true)]
async fn device_poll_rejection_keeps_its_exact_value_for_both_providers() {
    for codex in [false, true] {
        let rejection = CallbackError::from_rejection(AbortReason::Value(JsValue::String(JsString::from_utf16(vec![
            88, 0xd800,
        ]))));
        let first = if codex {
            r#"{"device_auth_id":"id","user_code":"CODE","interval":1}"#
        } else {
            r#"{"device_code":"id","user_code":"CODE","verification_uri":"https://github.com/login/device","interval":1,"expires_in":60}"#
        };
        let fetch = Arc::new(RejectPoll {
            first: RecordingFetch::new(vec![ScriptedResponse::json(200, first)]),
            calls: AtomicUsize::new(0),
            rejection: rejection.clone(),
        });
        let provider: Box<dyn OAuthAuth> = if codex {
            Box::new(OpenAICodexOAuth::with_runtime(runtime(fetch, 0.0)))
        } else {
            Box::new(GitHubCopilotOAuth::with_runtime(runtime(fetch, 0.0)))
        };
        let interaction = TestInteraction::new(vec![PromptAnswer::Text(if codex {
            "device_code".into()
        } else {
            String::new()
        })]);
        assert_eq!(
            provider.login(interaction).await.unwrap_err(),
            AuthError::from(rejection)
        );
    }
}

#[tokio::test]
async fn codex_refresh_wraps_non_error_rejections_with_string_conversion() {
    struct Reject;
    impl Fetch for Reject {
        fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            Box::pin(async { Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Bool(false)))) })
        }
    }
    let error = OpenAICodexOAuth::with_runtime(runtime(Arc::new(Reject), 0.0))
        .refresh(OAuthCredential::new("r", "a", 0.0))
        .await
        .unwrap_err();
    assert_eq!(
        error.detail(),
        Some(&JsString::from("OpenAI Codex token refresh error: false"))
    );
}

#[tokio::test]
async fn codex_expiry_reads_the_clock_after_the_response() {
    use std::sync::atomic::AtomicU64;
    struct AdvanceClock {
        inner: Arc<RecordingFetch>,
        clock: Arc<AtomicU64>,
    }
    impl Fetch for AdvanceClock {
        fn fetch(
            &self,
            request: HttpRequest,
            signal: Option<AbortSignal>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            self.clock.store(5000, Ordering::SeqCst);
            self.inner.fetch(request, signal)
        }
    }
    let clock = Arc::new(AtomicU64::new(1000));
    let payload = pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"id"}}"#);
    let body = format!(r#"{{"access_token":"e30.{payload}.sig","refresh_token":"r","expires_in":60}}"#);
    let fetch = Arc::new(AdvanceClock {
        inner: RecordingFetch::new(vec![ScriptedResponse::json(200, &body)]),
        clock: clock.clone(),
    });
    let runtime = pi_ai::auth::oauth::support::OAuthRuntime::new(
        fetch,
        Arc::new(move || clock.load(Ordering::SeqCst) as f64),
        support::TestRandom::new(),
    );
    let credential = OpenAICodexOAuth::with_runtime(runtime)
        .refresh(OAuthCredential::new("r", "a", 0.0))
        .await
        .unwrap();
    assert_eq!(credential.expires, 65_000.0);
}

#[tokio::test(start_paused = true)]
async fn copilot_only_models_fetch_has_a_request_signal() {
    struct CheckSignals(Arc<RecordingFetch>);
    impl Fetch for CheckSignals {
        fn fetch(
            &self,
            request: HttpRequest,
            signal: Option<AbortSignal>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            assert_eq!(signal.is_some(), request.url.ends_with("/models"), "{}", request.url);
            self.0.fetch(request, signal)
        }
    }
    let fetch = Arc::new(CheckSignals(RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            r#"{"device_code":"d","user_code":"U","verification_uri":"https://github.com/login/device","interval":1,"expires_in":60}"#,
        ),
        ScriptedResponse::json(200, r#"{"access_token":"ghu"}"#),
        ScriptedResponse::json(200, r#"{"token":"token","expires_at":1000}"#),
    ])));
    // The policy requests and model response need URL dispatch, not a fixed response count.
    struct Dispatch(Arc<CheckSignals>);
    impl Fetch for Dispatch {
        fn fetch(
            &self,
            request: HttpRequest,
            signal: Option<AbortSignal>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            if request.url.ends_with("/policy") || request.url.ends_with("/models") {
                assert_eq!(signal.is_some(), request.url.ends_with("/models"));
                let scripted = RecordingFetch::new(vec![ScriptedResponse::json(200, r#"{"data":[]}"#)]);
                Box::pin(async move { scripted.fetch(request, signal).await })
            } else {
                self.0.fetch(request, signal)
            }
        }
    }
    GitHubCopilotOAuth::with_runtime(runtime(Arc::new(Dispatch(fetch)), 0.0))
        .login(TestInteraction::new(vec![PromptAnswer::Text(String::new())]))
        .await
        .unwrap();
}

#[tokio::test]
async fn codex_body_failures_use_the_response_side_catch_policy() {
    struct Body;
    impl pi_ai::request::HttpBody for Body {
        fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
            Box::pin(async {
                Some(Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Bool(
                    false,
                )))))
            })
        }
    }
    struct BrokenBody(u16);
    impl Fetch for BrokenBody {
        fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            let status = self.0;
            Box::pin(async move {
                Ok(HttpResponse {
                    status,
                    status_text: "Server Error".into(),
                    headers: vec![],
                    body: Box::new(Body),
                })
            })
        }
    }
    for status in [200, 500] {
        let error = OpenAICodexOAuth::with_runtime(runtime(Arc::new(BrokenBody(status)), 0.0))
            .refresh(OAuthCredential::new("r", "a", 0.0))
            .await
            .unwrap_err()
            .into_callback();
        if status == 200 {
            assert_eq!(error.thrown_value(), AbortReason::Value(JsValue::Bool(false)));
        } else {
            assert_eq!(error.message, "OpenAI Codex token refresh failed (500): Server Error");
        }
    }
}

#[tokio::test]
async fn anthropic_json_error_details_use_the_syntax_error_brand() {
    let error = AnthropicOAuth::with_runtime(runtime(
        RecordingFetch::new(vec![ScriptedResponse::json(200, "bad")]),
        0.0,
    ))
    .refresh(OAuthCredential::new("r", "a", 0.0))
    .await
    .unwrap_err();
    assert!(error.detail().unwrap().contains("; details=SyntaxError: "));
}

#[tokio::test]
async fn manual_non_error_rejection_is_wrapped_as_an_error() {
    use pi_ai::auth::{AuthEvent, AuthInteraction, AuthPrompt};
    struct Interaction;
    impl AuthInteraction for Interaction {
        fn signal(&self) -> AbortSignal {
            AbortSignal::default()
        }
        fn prompt(&self, _: AuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
            Box::pin(async { Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Bool(false)))) })
        }
        fn notify(&self, _: AuthEvent) -> Result<(), CallbackError> {
            Ok(())
        }
    }
    let _port = lock_callback_port().await;
    let error = AnthropicOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), 0.0))
        .login(Arc::new(Interaction))
        .await
        .unwrap_err()
        .into_callback();
    assert_eq!(error, CallbackError::new("false"));
}

#[tokio::test]
async fn anthropic_callback_bind_error_matches_node_listen_fields() {
    let _port = lock_callback_port().await;
    let _listener = tokio::net::TcpListener::bind(("127.0.0.1", 53692)).await.unwrap();
    let error = AnthropicOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), 0.0))
        .login(TestInteraction::new(vec![]))
        .await
        .unwrap_err()
        .into_callback();
    assert_eq!(
        error.message,
        "listen EADDRINUSE: address already in use 127.0.0.1:53692"
    );
    let AbortReason::Error(error) = error.thrown_value() else {
        panic!("expected Node Error")
    };
    let value = error.value();
    assert_eq!(value.get("code"), Some(&JsValue::String("EADDRINUSE".into())));
    assert_eq!(value.get("errno"), Some(&JsValue::Number(-98.0)));
    assert_eq!(value.get("syscall"), Some(&JsValue::String("listen".into())));
    assert_eq!(value.get("address"), Some(&JsValue::String("127.0.0.1".into())));
    assert_eq!(value.get("port"), Some(&JsValue::Number(53692.0)));
}

#[tokio::test]
async fn closing_callback_server_releases_its_port_without_a_scheduler_turn() {
    use pi_ai::auth::oauth::support::{CallbackReply, start_callback_server};

    for accept_started in [false, true] {
        let server = start_callback_server(
            &"127.0.0.1".into(),
            0,
            Arc::new(|_| Box::pin(async { CallbackReply::html(200, "ok".to_owned()) })),
        )
        .await
        .unwrap();
        let port = server.port();
        if accept_started {
            tokio::task::yield_now().await;
        }
        server.close();
        let _rebound = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("close must release the listening port before returning, like Node server.close()");
    }
}

#[tokio::test]
async fn browser_callbacks_reject_empty_codes_without_settling_login() {
    let _port = lock_callback_port().await;
    for codex in [false, true] {
        let payload = pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"id"}}"#);
        let body = format!(r#"{{"access_token":"e30.{payload}.sig","refresh_token":"r","expires_in":60}}"#);
        let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &body)]);
        let provider: Box<dyn OAuthAuth> = if codex {
            Box::new(OpenAICodexOAuth::with_runtime(runtime(fetch, 0.0)))
        } else {
            Box::new(AnthropicOAuth::with_runtime(runtime(fetch, 0.0)))
        };
        let answers = if codex {
            vec![PromptAnswer::Text("browser".into()), PromptAnswer::Pending]
        } else {
            vec![PromptAnswer::Pending]
        };
        let interaction = TestInteraction::new(answers);
        let login = tokio::spawn({
            let interaction = interaction.clone();
            async move { provider.login(interaction).await }
        });
        let url = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(pi_ai::auth::AuthEvent::AuthUrl { url, .. }) = interaction
                    .events()
                    .into_iter()
                    .find(|event| matches!(event, pi_ai::auth::AuthEvent::AuthUrl { .. }))
                {
                    break url;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let url = url::Url::parse(&url.to_utf8().unwrap()).unwrap();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let callback = if codex {
            "http://127.0.0.1:1455/auth/callback"
        } else {
            "http://127.0.0.1:53692/callback"
        };
        let (status, _) = support::http_get(&format!("{callback}?code=&state={state}")).await;
        assert_eq!(status, 400);
        let (status, _) = support::http_get(&format!("{callback}?code=valid&state={state}&error=")).await;
        assert_eq!(status, 200);
        login.await.unwrap().unwrap();
    }
}

#[test]
fn empty_copilot_enterprise_domain_keeps_the_default_endpoint() {
    assert_eq!(
        pi_ai::oauth::get_github_copilot_base_url(None, Some("")),
        "https://api.individual.githubcopilot.com"
    );
}

#[tokio::test(start_paused = true)]
async fn copilot_policy_enable_does_not_read_response_bodies() {
    struct CountBody(Arc<AtomicUsize>);
    impl pi_ai::request::HttpBody for CountBody {
        fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { None })
        }
    }
    struct Policies {
        scripted: Arc<RecordingFetch>,
        reads: Arc<AtomicUsize>,
    }
    impl Fetch for Policies {
        fn fetch(
            &self,
            request: HttpRequest,
            signal: Option<AbortSignal>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            if request.url.ends_with("/policy") {
                let reads = self.reads.clone();
                Box::pin(async move {
                    Ok(HttpResponse {
                        status: 200,
                        status_text: String::new(),
                        headers: vec![],
                        body: Box::new(CountBody(reads)),
                    })
                })
            } else {
                self.scripted.fetch(request, signal)
            }
        }
    }
    let reads = Arc::new(AtomicUsize::new(0));
    let scripted = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            r#"{"device_code":"d","user_code":"U","verification_uri":"https://github.com/login/device","interval":1,"expires_in":60}"#,
        ),
        ScriptedResponse::json(200, r#"{"access_token":"ghu"}"#),
        ScriptedResponse::json(200, r#"{"token":"token","expires_at":1000}"#),
        ScriptedResponse::json(200, r#"{"data":[]}"#),
    ]);
    let fetch = Arc::new(Policies {
        scripted,
        reads: reads.clone(),
    });
    GitHubCopilotOAuth::with_runtime(runtime(fetch, 0.0))
        .login(TestInteraction::new(vec![PromptAnswer::Text(String::new())]))
        .await
        .unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn copilot_device_json_errors_keep_the_syntax_error_brand() {
    let provider = GitHubCopilotOAuth::with_runtime(runtime(
        RecordingFetch::new(vec![ScriptedResponse::json(200, "bad")]),
        0.0,
    ));
    let error = provider
        .login(TestInteraction::new(vec![PromptAnswer::Text(String::new())]))
        .await
        .unwrap_err()
        .into_callback();
    assert_eq!(error.name, "SyntaxError");
    assert_eq!(
        error.message,
        pi_js_compat::js_json::parse_lossless("bad").unwrap_err().into_message()
    );
}

#[tokio::test]
async fn codex_without_manual_callback_prompts_when_the_listener_cannot_bind() {
    use pi_ai::oauth::*;
    struct Callbacks;
    impl OAuthLoginCallbacks for Callbacks {
        fn on_auth(&self, _: OAuthAuthInfo) -> Result<(), CallbackError> {
            Ok(())
        }
        fn on_device_code(&self, _: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
            Ok(())
        }
        fn on_prompt(&self, _: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
            Box::pin(async { Err(CallbackError::new("fallback prompt used")) })
        }
        fn on_select(&self, _: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
            Box::pin(async { Err(CallbackError::new("direct browser flow must not select")) })
        }
    }
    let _port = lock_callback_port().await;
    let _listener = tokio::net::TcpListener::bind(("127.0.0.1", 1455)).await.unwrap();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        login_openai_codex(Arc::new(Callbacks), None),
    )
    .await
    .expect("must use the text prompt, not wait for absent manual input")
    .unwrap_err();
    assert_eq!(error.detail(), Some(&JsString::from("fallback prompt used")));
}

#[tokio::test]
async fn browser_listener_closes_while_token_exchange_is_pending() {
    struct DelayedFetch {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        scripted: Arc<RecordingFetch>,
    }
    impl Fetch for DelayedFetch {
        fn fetch(
            &self,
            request: HttpRequest,
            signal: Option<AbortSignal>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            Box::pin(async move {
                self.entered.notify_one();
                self.release.notified().await;
                self.scripted.fetch(request, signal).await
            })
        }
    }
    let _port = lock_callback_port().await;
    for codex in [false, true] {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let payload = pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"id"}}"#);
        let body = format!(r#"{{"access_token":"e30.{payload}.sig","refresh_token":"r","expires_in":60}}"#);
        let fetch = Arc::new(DelayedFetch {
            entered: entered.clone(),
            release: release.clone(),
            scripted: RecordingFetch::new(vec![ScriptedResponse::json(200, &body)]),
        });
        let provider: Box<dyn OAuthAuth> = if codex {
            Box::new(OpenAICodexOAuth::with_runtime(runtime(fetch, 0.0)))
        } else {
            Box::new(AnthropicOAuth::with_runtime(runtime(fetch, 0.0)))
        };
        let answers = if codex {
            vec![PromptAnswer::Text("browser".into()), PromptAnswer::Text("code".into())]
        } else {
            vec![PromptAnswer::Text("code".into())]
        };
        let login = tokio::spawn(async move { provider.login(TestInteraction::new(answers)).await });
        entered.notified().await;
        let port = if codex { 1455 } else { 53692 };
        let rebound = tokio::net::TcpListener::bind(("127.0.0.1", port)).await;
        release.notify_one();
        login.await.unwrap().unwrap();
        assert!(
            rebound.is_ok(),
            "listener must close before awaiting the exchange result"
        );
    }
}

#[tokio::test]
async fn anthropic_request_details_format_branded_causes_recursively() {
    use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
    struct Reject;
    impl Fetch for Reject {
        fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            Box::pin(async {
                let cause = NativeNodeError::new("Error", "connect failed")
                    .with_property("code", JsValue::String("ECONNREFUSED".into()))
                    .with_property("errno", JsValue::Number(-111.0));
                Err(
                    NativeNodeError::from_constructor(NativeErrorClass::TypeError, "fetch failed")
                        .with_cause(AbortReason::Error(cause))
                        .into(),
                )
            })
        }
    }
    let error = AnthropicOAuth::with_runtime(runtime(Arc::new(Reject), 0.0))
        .refresh(OAuthCredential::new("r", "a", 0.0))
        .await
        .unwrap_err();
    assert_eq!(
        error.detail(),
        Some(&JsString::from(
            "Anthropic token refresh request failed. url=https://platform.claude.com/v1/oauth/token; details=TypeError: fetch failed; cause=Error: connect failed; code=ECONNREFUSED; errno=-111"
        ))
    );
}

#[tokio::test]
async fn invalid_token_json_keeps_syntax_error_identity() {
    let provider = OpenAICodexOAuth::with_runtime(runtime(
        RecordingFetch::new(vec![ScriptedResponse::json(200, "not json")]),
        0.0,
    ));
    let error = provider
        .refresh(OAuthCredential::new("r", "a", 0.0))
        .await
        .unwrap_err()
        .into_callback();
    assert_eq!(error.name, "SyntaxError");
    assert_eq!(
        error.message,
        pi_js_compat::js_json::parse_lossless("not json")
            .unwrap_err()
            .into_message()
    );
}
