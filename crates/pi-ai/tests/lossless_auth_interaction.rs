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
    fn signal(&self) -> pi_js_compat::abort::AbortSignal {
        pi_js_compat::abort::AbortSignal::default()
    }
    fn prompt(
        &self,
        _prompt: pi_ai::auth::types::AuthPrompt,
    ) -> pi_ai::request::BoxFuture<'_, Result<JsString, pi_ai::request::CallbackError>> {
        Box::pin(async { Ok(JsString::from_utf16(vec![0xd800, 0x20, 0xdc00])) })
    }
    fn notify(&self, _event: pi_ai::auth::types::AuthEvent) -> Result<(), pi_ai::request::CallbackError> {
        Ok(())
    }
}

#[tokio::test]
async fn api_key_login_keeps_native_prompt_answer() {
    use pi_ai::auth::{helpers::env_api_key_auth, types::ApiKeyAuth};
    let credential = env_api_key_auth("fixture", &[]).login().unwrap()(std::sync::Arc::new(NativeInteraction))
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
fn request_boundary_errors_retain_typeerror_constructor() {
    use pi_js_compat::{abort::AbortReason, node_error::NativeErrorClass};
    let errors = [
        pi_ai::request::header_value(&JsString::from("€")).unwrap_err(),
        pi_ai::request::request_url(&JsString::from("not a URL")).unwrap_err(),
    ];
    for error in errors {
        let AbortReason::Error(error) = error.thrown_value() else {
            panic!("expected native Error");
        };
        assert_eq!(error.name, "TypeError");
        assert_eq!(error.constructor_class(), NativeErrorClass::TypeError);
    }
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

#[tokio::test]
async fn auth_context_encodes_raw_path_only_at_filesystem_lookup() {
    let directory = std::env::temp_dir().join(format!("native-auth-context-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let raw = JsString::from(directory.to_str().unwrap()) + "/" + JsString::from_utf16(vec![0xd800]);
    std::fs::write(pi_js_compat::js_json::encode_node_utf8(&raw), b"fixture").unwrap();
    assert!(
        pi_ai::auth::context::default_provider_auth_context()
            .file_exists(&raw)
            .await
            .unwrap()
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn manual_code_parser_retains_units_until_url_or_form_boundaries() {
    use pi_ai::auth::oauth::support::{form_encode, parse_authorization_input, to_form_usv_string};
    let raw = JsString::from_utf16(vec![0xd800]);
    let parsed = parse_authorization_input(&(raw.clone() + "#state"));
    assert_eq!(parsed.code, Some(raw.clone()));
    assert_eq!(parsed.state, Some(JsString::from("state")));
    assert_eq!(form_encode(&[("code", &to_form_usv_string(&raw))]), "code=%EF%BF%BD");
    let parsed_url = parse_authorization_input(&(JsString::from("https://example.test/?code=") + &raw));
    assert_eq!(parsed_url.code, Some(JsString::from("�")));
}

#[tokio::test]
async fn cloudflare_login_keeps_native_key_and_account_id() {
    use pi_ai::auth::types::ApiKeyAuth;
    let credential = pi_ai::providers::cloudflare_auth::CloudflareWorkersAiAuth
        .login()
        .unwrap()(std::sync::Arc::new(NativeInteraction))
    .await
    .unwrap();
    let raw = JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]);
    assert_eq!(credential.key, Some(raw.clone()));
    assert_eq!(credential.env.unwrap()["CLOUDFLARE_ACCOUNT_ID"], raw);
}

#[tokio::test]
async fn oauth_to_auth_does_not_narrow_token_before_request() {
    use pi_ai::auth::types::{OAuthAuth, OAuthCredential};
    let raw = JsString::from_utf16(vec![0xd800]);
    let auth = pi_ai::auth::oauth::anthropic::AnthropicOAuth::new()
        .to_auth(OAuthCredential::new("refresh", raw.clone(), 0.0))
        .await
        .unwrap();
    assert_eq!(auth.api_key, Some(raw));
}

#[test]
fn frozen_auth_event_json_retains_native_utf16() {
    let cases = [
        r#"{"type":"auth_url","url":"https://example.test/\udfff","instructions":"\ud800"}"#,
        r#"{"type":"device_code","userCode":"\ud800","verificationUri":"https://example.test/\udfff","intervalSeconds":0,"expiresInSeconds":13.5}"#,
        r#"{"type":"progress","message":"\ud800 \udc00"}"#,
    ];
    for wire in cases {
        let event: pi_ai::auth::types::AuthEvent = serde_json::from_str(wire).unwrap();
        assert_eq!(
            js_json::parse_lossless(serde_json::to_string(&event).unwrap()).unwrap(),
            js_json::parse_lossless(wire).unwrap()
        );
    }
}

#[test]
fn models_error_exposes_native_message_without_display_projection() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let error = pi_ai::auth::ModelsError::new(pi_ai::auth::ModelsErrorCode::Auth, raw.clone());
    assert_eq!(error.message, raw);
    assert_eq!(error.to_string(), "�");
}
