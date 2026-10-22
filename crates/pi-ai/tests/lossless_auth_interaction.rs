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

#[test]
fn http_header_value_has_exact_node_bytestring_errors() {
    for (units, index, value) in [
        (vec![0x61, 0xd800], 1, 55296),
        (vec![0xdc00], 0, 56320),
        (vec![0x20ac], 0, 8364),
    ] {
        let error = pi_ai::request::header_value(&JsString::from_utf16(units)).unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert_eq!(
            error.message,
            format!(
                "Cannot convert argument to a ByteString because the character at index {index} has a value of {value} which is greater than 255."
            )
        );
    }
    assert_eq!(pi_ai::request::header_value(&JsString::from("é")).unwrap(), "é");
}

#[test]
fn http_url_replaces_lone_units_only_when_parsing_url() {
    let raw = JsString::from("https://example.test/") + JsString::from_utf16(vec![0xd800, 0x2f, 0xdc00]);
    assert_eq!(
        pi_ai::request::request_url(&raw).unwrap(),
        "https://example.test/%EF%BF%BD/%EF%BF%BD"
    );
    assert_eq!(raw.units()[21], 0xd800);
}
