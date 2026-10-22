//! Ports `cli/auth-command.ts` helpers added after the parser slice:
//! `validateAuthCommandArgs`, `getAuthCredential`, and `printAuthCommandHelp`.

use pi_ai::auth::types::{AuthResult, ModelAuth};
use pi_ai::types::ProviderHeaders;
use pi_coding_agent::cli::args::parse_args;
use pi_coding_agent::cli::{AuthCommandKind, get_auth_credential, print_auth_command_help, validate_auth_command_args};
use pi_js_compat::js_json::JsString;

fn parse(values: &[&str]) -> pi_coding_agent::cli::args::Args {
    parse_args(&values.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
}

fn auth_with(api_key: Option<&str>, headers: Option<&[(&str, Option<&str>)]>) -> AuthResult {
    AuthResult {
        auth: ModelAuth {
            api_key: (api_key.map(str::to_owned)).map(JsString::from),
            headers: headers.map(|entries| {
                entries
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.map(str::to_owned)))
                    .collect::<ProviderHeaders>()
            }),
            base_url: None,
        },
        env: None,
        source: None,
    }
}

#[test]
fn validates_provider_and_model_with_trimming() {
    let target = validate_auth_command_args(&parse(&["--provider", "  openai  "]), AuthCommandKind::Check)
        .expect("valid target");
    assert_eq!(target.provider(), Some("openai"));
    assert_eq!(target.model(), None);

    let target = validate_auth_command_args(&parse(&["--model", "\topenai/gpt-4.1\n"]), AuthCommandKind::ApiKey)
        .expect("valid target");
    assert_eq!(target.model(), Some("openai/gpt-4.1"));

    let target = validate_auth_command_args(
        &parse(&["--provider", "   ", "--model", "gpt-4.1"]),
        AuthCommandKind::Check,
    )
    .expect("valid target");
    assert_eq!(target.provider(), None);
}

#[test]
fn rejects_unknown_flags_messages_files_and_api_keys() {
    let error = validate_auth_command_args(
        &parse(&["--provider", "openai", "--extra", "value"]),
        AuthCommandKind::Check,
    )
    .expect_err("unknown flag");
    assert_eq!(error.message(), "Unknown option --extra for \"auth check\".");

    let error = validate_auth_command_args(&parse(&["--provider", "openai", "--extra"]), AuthCommandKind::ApiKey)
        .expect_err("unknown flag");
    assert_eq!(error.message(), "Unknown option --extra for \"auth print-api-key\".");

    let error = validate_auth_command_args(
        &parse(&["--provider", "openai", "--api-key", "key"]),
        AuthCommandKind::Check,
    )
    .expect_err("api key");
    assert_eq!(error.message(), "Auth commands only accept --provider and --model");

    let error = validate_auth_command_args(&parse(&["--provider", "openai", "message"]), AuthCommandKind::Check)
        .expect_err("message");
    assert_eq!(error.message(), "Auth commands only accept --provider and --model");

    let error = validate_auth_command_args(&parse(&["--provider", "openai", "@file"]), AuthCommandKind::Check)
        .expect_err("file");
    assert_eq!(error.message(), "Auth commands only accept --provider and --model");
}

#[test]
fn requires_a_provider_or_model_per_command_kind() {
    let error = validate_auth_command_args(&parse(&[]), AuthCommandKind::Check).expect_err("missing target");
    assert_eq!(
        error.message(),
        "Auth checks require --provider <provider> or --model <model>"
    );

    for kind in [AuthCommandKind::ApiKey, AuthCommandKind::BearerToken] {
        let error = validate_auth_command_args(&parse(&[]), kind).expect_err("missing target");
        assert_eq!(
            error.message(),
            "Credential printing requires --provider <provider> or --model <model>"
        );
    }
}

#[test]
fn extracts_api_keys_before_headers() {
    let auth = auth_with(Some("sk-test"), Some(&[("authorization", Some("Bearer header-token"))]));
    assert_eq!(
        get_auth_credential(Some(&auth)).as_ref(),
        Some(&JsString::from("sk-test"))
    );

    let empty = auth_with(Some(""), Some(&[("authorization", Some("Bearer header-token"))]));
    assert_eq!(
        get_auth_credential(Some(&empty)).as_ref(),
        Some(&JsString::from("header-token"))
    );
    assert_eq!(get_auth_credential(None), None);
}

#[test]
fn extracts_bearer_tokens_from_case_insensitive_authorization_headers() {
    let auth = auth_with(None, Some(&[("Authorization", Some("Bearer header-token"))]));
    assert_eq!(
        get_auth_credential(Some(&auth)).as_ref(),
        Some(&JsString::from("header-token"))
    );

    let lowercase = auth_with(None, Some(&[("AUTHORIZATION", Some("bearer lower-token"))]));
    assert_eq!(
        get_auth_credential(Some(&lowercase)).as_ref(),
        Some(&JsString::from("lower-token"))
    );

    let spaces = auth_with(None, Some(&[("authorization", Some("Bearer \t \u{a0}token"))]));
    assert_eq!(
        get_auth_credential(Some(&spaces)).as_ref(),
        Some(&JsString::from("token"))
    );
}

#[test]
fn rejects_non_bearer_and_malformed_authorization_values() {
    for value in [
        "Basic abc",
        "Bearer",
        "Bearer ",
        "Bearer token\nmore",
        "Bearer token\r",
        "Bearer to\u{2028}ken",
        "Bearer to\u{2029}ken",
    ] {
        let auth = auth_with(None, Some(&[("authorization", Some(value))]));
        assert_eq!(get_auth_credential(Some(&auth)), None, "value: {value:?}");
    }

    let missing = auth_with(None, Some(&[("authorization", None)]));
    assert_eq!(get_auth_credential(Some(&missing)), None);
    let other = auth_with(None, Some(&[("x-api-key", Some("Bearer token"))]));
    assert_eq!(get_auth_credential(Some(&other)), None);
}

#[test]
fn prints_auth_command_help_with_console_log_newline() {
    assert_eq!(
        print_auth_command_help(),
        "Usage:\n  pi auth print-api-key [--provider <provider>] [--model <model>]\n  pi auth print-bearer-token [--provider <provider>] [--model <model>] [--min-expiry <duration>]\n  pi auth check [--provider <provider>] [--model <model>] [--json] [--credentials] [--no-refresh]\n\nAuth commands require at least one of --provider or --model. Checks refresh expired OAuth credentials by default; --no-refresh prevents this. --credentials emits the credential, or includes it in JSON output.\n"
    );
}
