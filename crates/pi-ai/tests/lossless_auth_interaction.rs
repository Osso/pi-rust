use pi_ai::auth::types::Credential;
use pi_js_compat::js_json::{self, JsString};

#[test]
fn credential_round_trip_preserves_unmatched_key_and_oauth_fields() {
    for wire in [
        r#"{"type":"api_key","key":"secret\ud800"}"#,
        r#"{"type":"oauth","refresh":"\ud800","access":"\udc00","expires":0,"\ud800":"\udfff"}"#,
    ] {
        let credential: Credential = serde_json::from_str(wire).expect("native credential");
        let output = serde_json::to_string(&credential).unwrap();
        assert_eq!(
            js_json::parse_lossless(&output).unwrap(),
            js_json::parse_lossless(wire).unwrap()
        );
    }
}

#[derive(Default)]
struct NativeInteraction;
impl pi_ai::auth::types::AuthInteraction for NativeInteraction {
    fn signal(&self) -> tokio_util::sync::CancellationToken {
        tokio_util::sync::CancellationToken::new()
    }
    fn prompt(
        &self,
        _prompt: pi_ai::auth::types::AuthPrompt,
    ) -> pi_ai::request::BoxFuture<'_, Result<JsString, pi_ai::request::CallbackError>> {
        Box::pin(async { Ok(JsString::from_utf16(vec![0xd800, 0x20, 0xdc00])) })
    }
    fn notify(&self, _event: pi_ai::auth::types::AuthEvent) {}
}

#[tokio::test]
async fn api_key_login_keeps_native_prompt_answer() {
    use pi_ai::auth::{helpers::env_api_key_auth, types::ApiKeyAuth};
    let credential = env_api_key_auth("fixture", &[])
        .login(std::sync::Arc::new(NativeInteraction))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        credential.key.unwrap(),
        JsString::from_utf16(vec![0xd800, 0x20, 0xdc00])
    );
}
