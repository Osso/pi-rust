use crate::shared_oauth as oauth_support;
use crate::shared_oracle as oracle;

use pi_ai::auth::{
    OAuthAuth,
    oauth::{
        anthropic::AnthropicOAuth,
        openai_codex::OpenAICodexOAuth,
        support::{OAuthRuntime, RandomSource},
    },
};
use pi_ai::request::CallbackError;
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::NativeNodeError,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct RejectEntropy {
    call: AtomicUsize,
    fail_at: usize,
    error: CallbackError,
}
impl RandomSource for RejectEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CallbackError> {
        if self.call.fetch_add(1, Ordering::SeqCst) == self.fail_at {
            return Err(self.error.clone());
        }
        bytes.fill(0);
        Ok(())
    }
    fn uuid(&self) -> Result<String, CallbackError> {
        pi_ai::auth::oauth::support::uuid_v4(self)
    }
}

#[tokio::test]
async fn login_entropy_rejections_match_frozen_before_network_or_auth_notifications() {
    let mut source = oracle::TsOracle::spawn("pkce-repin-oracle.mts", "packages/ai/src/utils/oauth/pkce.ts");
    for (flow, stage, fail_at) in [
        ("anthropic", "random", 0),
        ("openai-codex", "random", 0),
        ("openai-codex", "state", 1),
    ] {
        for raw in [false, true] {
            let expected = source.request(&js_json!({"bytes":vec![0;32],"flow":flow,"reject":stage,"raw":raw}));
            assert_eq!(expected["ok"], JsValue::Bool(false), "{expected:?}");
            let reason = if raw {
                AbortReason::Value(JsValue::String(
                    JsString::from("pkce-") + JsString::from_utf16(vec![0xd800]),
                ))
            } else {
                NativeNodeError::new(
                    "TypeError",
                    JsString::from("crypto-") + JsString::from_utf16(vec![0xdfff]),
                )
                .with_property("code", JsValue::String("E_CRYPTO".into()))
                .into()
            };
            let random = Arc::new(RejectEntropy {
                call: AtomicUsize::new(0),
                fail_at,
                error: CallbackError::from_rejection(reason),
            });
            let fetch = oauth_support::recording_fetch::RecordingFetch::new(vec![]);
            let runtime = OAuthRuntime::new(fetch.clone(), Arc::new(|| 1_750_000_000_000.0), random.clone());
            let interaction =
                oauth_support::TestInteraction::new(vec![oauth_support::PromptAnswer::Text("browser".into())]);
            let error = if flow == "anthropic" {
                AnthropicOAuth::with_runtime(runtime)
                    .login(interaction.clone())
                    .await
                    .unwrap_err()
            } else {
                OpenAICodexOAuth::with_runtime(runtime)
                    .login(interaction.clone())
                    .await
                    .unwrap_err()
            };
            assert_eq!(error.callback().thrown_value().value(), expected["value"]);
            assert_eq!(random.call.load(Ordering::SeqCst), fail_at + 1);
            assert!(fetch.requests().is_empty());
            assert!(interaction.events().is_empty());
            assert_eq!(interaction.prompts().len(), if flow == "anthropic" { 0 } else { 1 });
            assert_eq!(
                expected["effects"],
                if flow == "anthropic" {
                    js_json!([])
                } else {
                    js_json!(["prompt"])
                }
            );
        }
    }
}
