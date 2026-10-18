use pi_coding_agent::cli::experimental::auth::{AuthInput, ParsedAuthInput, RawAuthOptions, parse_auth_input};
use serde_json::json;

fn options(token: Option<&str>, file: Option<&str>) -> RawAuthOptions {
    RawAuthOptions {
        auth_token: token.map(str::to_owned),
        auth_token_file: file.map(str::to_owned),
    }
}

#[test]
fn rejects_token_and_token_file_together() {
    assert_eq!(
        parse_auth_input(&options(Some("secret"), Some("/tmp/token"))),
        ParsedAuthInput {
            auth: None,
            errors: vec!["--auth-token and --auth-token-file are mutually exclusive".into()],
        }
    );
}

#[test]
fn parses_token_and_file_sources() {
    let token = parse_auth_input(&options(Some("secret"), None));
    assert_eq!(token.auth, Some(AuthInput::Token { token: "secret".into() }));
    assert!(token.errors.is_empty());
    assert_eq!(
        serde_json::to_value(token.auth).unwrap(),
        json!({"type": "token", "token": "secret"})
    );
    let file = parse_auth_input(&options(None, Some("/tmp/token")));
    assert_eq!(
        serde_json::to_value(file.auth).unwrap(),
        json!({"type": "file", "path": "/tmp/token"})
    );
}

#[test]
fn keeps_empty_strings_as_provided_values() {
    assert_eq!(
        parse_auth_input(&options(Some(""), None)).auth,
        Some(AuthInput::Token { token: String::new() })
    );
}

#[test]
fn permits_omitted_authentication() {
    assert_eq!(parse_auth_input(&options(None, None)), ParsedAuthInput::default());
}
