use std::sync::{Arc, Mutex};

use pi_ai::{
    OAuthAuthInfo, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectPrompt,
    auth::types::AuthError,
    request::{BoxFuture, CallbackError},
};
use pi_js_compat::js_json::JsString;

struct RejectingCallbacks {
    seen: Mutex<Vec<&'static str>>,
}

fn rejection() -> CallbackError {
    let mut error = CallbackError::new(JsString::from("rejected ") + JsString::from_utf16(vec![0xd800]));
    error.name = "FixtureFailure".into();
    error.code = Some("FIXTURE_REJECT".into());
    error
}

impl OAuthLoginCallbacks for RejectingCallbacks {
    fn on_auth(&self, _: OAuthAuthInfo) -> Result<(), CallbackError> {
        Ok(())
    }
    fn on_device_code(&self, _: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
        Ok(())
    }
    fn on_prompt(&self, _: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        self.seen.lock().unwrap().push("prompt");
        Box::pin(async { Err(rejection()) })
    }
    fn on_manual_code_input(&self) -> Option<BoxFuture<'_, Result<JsString, CallbackError>>> {
        self.seen.lock().unwrap().push("manual");
        Some(Box::pin(async { Err(rejection()) }))
    }
    fn on_select(&self, _: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        self.seen.lock().unwrap().push("select");
        Box::pin(async { Err(rejection()) })
    }
}

async fn assert_rejected(provider_id: &str, step: &'static str) {
    let callbacks = Arc::new(RejectingCallbacks {
        seen: Mutex::new(vec![]),
    });
    let provider = pi_ai::oauth::get_oauth_provider(provider_id).unwrap();
    let error = provider.login(callbacks.clone()).await.unwrap_err();
    assert_eq!(error, AuthError::from(rejection()));
    assert!(callbacks.seen.lock().unwrap().contains(&step));
}

#[tokio::test]
async fn rejecting_prompt_rejects_github_login_without_replacement() {
    assert_rejected("github-copilot", "prompt").await;
}

#[tokio::test]
async fn rejecting_manual_code_rejects_anthropic_login_without_replacement() {
    assert_rejected("anthropic", "manual").await;
}

#[tokio::test]
async fn rejecting_selection_rejects_codex_login_without_replacement() {
    assert_rejected("openai-codex", "select").await;
}
