use pi_coding_agent::cli::{AuthCommandKind, get_auth_command_name, get_auth_command_usage, is_auth_command_help};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn auth_command_names_match_public_commands() {
    assert_eq!(get_auth_command_name(AuthCommandKind::Check), "auth check");
    assert_eq!(get_auth_command_name(AuthCommandKind::ApiKey), "auth print-api-key");
    assert_eq!(
        get_auth_command_name(AuthCommandKind::BearerToken),
        "auth print-bearer-token"
    );
}

#[test]
fn auth_command_usages_match_public_syntax() {
    assert_eq!(
        get_auth_command_usage(AuthCommandKind::Check),
        "pi auth check --provider <provider> [--json] [--credentials] [--no-refresh]"
    );
    assert_eq!(
        get_auth_command_usage(AuthCommandKind::ApiKey),
        "pi auth print-api-key --provider <provider> [--model <model>]"
    );
    assert_eq!(
        get_auth_command_usage(AuthCommandKind::BearerToken),
        "pi auth print-bearer-token --provider <provider> [--model <model>] [--min-expiry <duration>]"
    );
}

#[test]
fn auth_help_requires_exact_first_argument() {
    assert!(!is_auth_command_help(&args(&[])));
    assert!(!is_auth_command_help(&args(&["--help"])));
    assert!(!is_auth_command_help(&args(&["status", "--help"])));
    assert!(!is_auth_command_help(&args(&["Auth", "--help"])));
}

#[test]
fn auth_alone_and_auth_help_are_help_requests() {
    assert!(is_auth_command_help(&args(&["auth"])));
    assert!(is_auth_command_help(&args(&["auth", "help"])));
    assert!(is_auth_command_help(&args(&["auth", "help", "extra"])));
}

#[test]
fn exact_help_flags_match_in_any_later_position() {
    let cases = [
        &["auth", "--help"][..],
        &["auth", "-h"][..],
        &["auth", "check", "--help"][..],
        &["auth", "check", "-h"][..],
        &["auth", "check", "--help", "-h"][..],
        &["auth", "check", "-h", "--help"][..],
        &["auth", "unknown", "--help"][..],
        &["auth", "unknown", "duplicate", "-h"][..],
        &["auth", "check", "--help", "--help"][..],
    ];

    for case in cases {
        assert!(is_auth_command_help(&args(case)), "expected help: {case:?}");
    }
}

#[test]
fn lookalike_help_arguments_do_not_match() {
    let cases = [
        &["Help"][..],
        &["auth", "Help"][..],
        &["auth", "--HELP"][..],
        &["auth", "-H"][..],
        &["auth", "helpful"][..],
        &["auth", "unknown", "extra"][..],
        &["auth", "--helpful"][..],
        &["auth", "héLp"][..],
        &["auth", "--help\0"][..],
        &["auth", "\0help"][..],
        &["auth", "ｈelp"][..],
    ];

    for case in cases {
        assert!(!is_auth_command_help(&args(case)), "unexpected help: {case:?}");
    }
}
