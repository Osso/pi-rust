//! Native regressions found by the whole-module common-pin OAuth audit.
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
#[path = "support/oauth.rs"]
mod support;
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
