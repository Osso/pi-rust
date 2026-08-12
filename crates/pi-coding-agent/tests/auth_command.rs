use std::error::Error as _;

use pi_coding_agent::cli::{
    AuthCommand, AuthCommandError, AuthCommandKind, get_auth_command_name, get_auth_command_usage,
    is_auth_command_help, parse_auth_command,
};

const MAX_SAFE_INTEGER_BITS: u64 = 0x433f_ffff_ffff_ffff;
const FIRST_UNSAFE_INTEGER_BITS: u64 = 0x4340_0000_0000_0000;
const MAX_SAFE_INTEGER_SECONDS_BITS: u64 = 0x43df_3fff_ffff_ffff;
const ONE_E308_BITS: u64 = 0x7fe1_ccf3_85eb_c8a0;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn parse(values: &[&str]) -> Result<Option<AuthCommand>, AuthCommandError> {
    parse_auth_command(&args(values))
}

fn command(values: &[&str]) -> AuthCommand {
    parse(values)
        .expect("expected auth command parsing to succeed")
        .expect("expected an auth command")
}

fn error(values: &[&str]) -> AuthCommandError {
    parse(values).expect_err("expected auth command parsing to fail")
}

fn assert_command(
    command: &AuthCommand,
    kind: AuthCommandKind,
    expected_args: &[&str],
    json: bool,
    credentials: bool,
    no_refresh: bool,
    expected_expiry_bits: Option<u64>,
) {
    let expected_args = args(expected_args);
    assert_eq!(command.kind(), kind);
    assert_eq!(command.args(), expected_args.as_slice());
    assert_eq!(command.json(), json);
    assert_eq!(command.credentials(), credentials);
    assert_eq!(command.no_refresh(), no_refresh);
    assert_eq!(command.min_expiry_ms().map(f64::to_bits), expected_expiry_bits);
}

fn assert_duration_error(name: &str, values: &[&str]) {
    let actual = error(values);
    assert_eq!(
        actual.message(),
        "--min-expiry must use a duration such as 30m or 1h",
        "invalid duration case: {name}"
    );
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

#[test]
fn parse_auth_command_returns_none_or_recognizes_all_kinds_with_defaults() {
    assert_eq!(parse(&[]).unwrap(), None);
    assert_eq!(parse(&["status"]).unwrap(), None);
    assert_eq!(parse(&["status", "auth", "--help", "--min-expiry"]).unwrap(), None);

    assert_command(
        &command(&["auth", "check"]),
        AuthCommandKind::Check,
        &[],
        false,
        false,
        false,
        None,
    );
    assert_command(
        &command(&["auth", "print-api-key"]),
        AuthCommandKind::ApiKey,
        &[],
        false,
        false,
        false,
        None,
    );
    assert_command(
        &command(&["auth", "print-bearer-token"]),
        AuthCommandKind::BearerToken,
        &[],
        false,
        false,
        false,
        None,
    );
}

#[test]
fn unknown_auth_subcommands_have_exact_dynamic_messages() {
    let cases = [
        &["auth"][..],
        &["auth", ""][..],
        &["auth", "help"][..],
        &["auth", "hé\0lp"][..],
    ];

    for case in cases {
        let subcommand = case.get(1).copied().unwrap_or("");
        let expected = format!(
            "Unknown auth command \"{subcommand}\". Use \"pi auth print-api-key\", \"pi auth print-bearer-token\", or \"pi auth check\"."
        );
        let actual = error(case);
        assert_eq!(actual.message(), expected);
    }
}

#[test]
fn check_flags_are_removed_idempotently_and_other_arguments_keep_order() {
    let parsed = command(&[
        "auth",
        "check",
        "before",
        "--json",
        "--credentials",
        "--no-refresh",
        "--json",
        "é",
        "\0",
        "duplicate",
        "duplicate",
        "--help",
        "after",
        "--credentials",
        "--no-refresh",
    ]);

    assert_command(
        &parsed,
        AuthCommandKind::Check,
        &["before", "é", "\0", "duplicate", "duplicate", "--help", "after"],
        true,
        true,
        true,
        None,
    );
}

#[test]
fn credential_commands_reject_check_only_flags() {
    let cases = [
        (&["auth", "print-api-key", "--json"][..], "--json"),
        (&["auth", "print-api-key", "--credentials"][..], "--credentials"),
        (&["auth", "print-api-key", "--no-refresh"][..], "--no-refresh"),
        (&["auth", "print-bearer-token", "--json"][..], "--json"),
        (&["auth", "print-bearer-token", "--credentials"][..], "--credentials"),
        (&["auth", "print-bearer-token", "--no-refresh"][..], "--no-refresh"),
    ];

    for (case, flag) in cases {
        let actual = error(case);
        assert_eq!(actual.message(), format!("{flag} is only supported by auth check"));
    }

    let actual = error(&["auth", "print-bearer-token", "--min-expiry", "1h", "--json"]);
    assert_eq!(actual.message(), "--json is only supported by auth check");
}

#[test]
fn min_expiry_is_only_supported_by_bearer_tokens() {
    for kind in ["check", "print-api-key"] {
        for tail in [&[][..], &["invalid"][..], &["1m"][..]] {
            let mut values = vec!["auth", kind, "--min-expiry"];
            values.extend_from_slice(tail);
            let actual = error(&values);
            assert_eq!(actual.message(), "--min-expiry is only supported by print-bearer-token");
        }
    }
}

#[test]
fn bearer_min_expiry_rejects_invalid_duration_text() {
    let cases = [
        ("empty", ""),
        ("Arabic digits", "١m"),
        ("fullwidth digits", "１m"),
        ("decimal", "1.5m"),
        ("plus sign", "+1m"),
        ("minus sign", "-1m"),
        ("exponent", "1e3m"),
        ("leading whitespace", " 1m"),
        ("trailing whitespace", "1m "),
        ("internal whitespace", "1 m"),
        ("missing unit", "1"),
        ("unsupported unit", "1d"),
        ("duplicate unit", "1msm"),
        ("NUL suffix", "1m\0"),
    ];

    for (name, value) in cases {
        assert_duration_error(name, &["auth", "print-bearer-token", "--min-expiry", value]);
    }

    assert_duration_error("missing value", &["auth", "print-bearer-token", "--min-expiry"]);
}

#[test]
fn accepted_min_expiry_units_preserve_source_case_folding_bug() {
    let cases = [
        ("1ms", 1.0_f64),
        ("1s", 1_000.0),
        ("1m", 60_000.0),
        ("1mS", 3_600_000.0),
        ("1mſ", 3_600_000.0),
        ("1Ms", 3_600_000.0),
        ("1MS", 3_600_000.0),
        ("1Mſ", 3_600_000.0),
        ("1S", 3_600_000.0),
        ("1ſ", 3_600_000.0),
        ("1M", 3_600_000.0),
        ("1h", 3_600_000.0),
        ("1H", 3_600_000.0),
        ("0ms", 0.0),
        ("000ms", 0.0),
    ];

    for (duration, expected) in cases {
        let parsed = command(&["auth", "print-bearer-token", "--min-expiry", duration]);
        assert_command(
            &parsed,
            AuthCommandKind::BearerToken,
            &[],
            false,
            false,
            false,
            Some(expected.to_bits()),
        );
    }
}

#[test]
fn repeated_valid_expiry_values_last_wins_and_later_invalid_values_fail() {
    let parsed = command(&[
        "auth",
        "print-bearer-token",
        "before",
        "--min-expiry",
        "1s",
        "middle",
        "--min-expiry",
        "2m",
        "after",
    ]);
    assert_command(
        &parsed,
        AuthCommandKind::BearerToken,
        &["before", "middle", "after"],
        false,
        false,
        false,
        Some(120_000.0_f64.to_bits()),
    );

    assert_duration_error(
        "later invalid value",
        &[
            "auth",
            "print-bearer-token",
            "--min-expiry",
            "1s",
            "--min-expiry",
            "invalid",
        ],
    );
}

#[test]
fn accepted_expiry_values_match_javascript_number_bits() {
    let repeated_nines_308 = "9".repeat(308);
    let repeated_nines_309 = "9".repeat(309);
    let cases = [
        ("9007199254740991ms", MAX_SAFE_INTEGER_BITS),
        ("9007199254740993ms", FIRST_UNSAFE_INTEGER_BITS),
        ("9007199254740991s", MAX_SAFE_INTEGER_SECONDS_BITS),
        (&format!("{repeated_nines_308}ms"), ONE_E308_BITS),
        (&format!("{repeated_nines_309}ms"), f64::INFINITY.to_bits()),
        (&format!("{repeated_nines_308}h"), f64::INFINITY.to_bits()),
    ];

    for (duration, expected_bits) in cases {
        let parsed = command(&["auth", "print-bearer-token", "--min-expiry", duration]);
        assert_command(
            &parsed,
            AuthCommandKind::BearerToken,
            &[],
            false,
            false,
            false,
            Some(expected_bits),
        );
        let expiry = parsed.min_expiry_ms().expect("accepted duration has expiry");
        assert!(!expiry.is_nan());
        assert_ne!(expiry.to_bits(), (-0.0_f64).to_bits());
    }
}

#[test]
fn auth_command_errors_expose_source_compatible_identity_and_message() {
    let actual = error(&["auth", "unknown"]);
    let expected = "Unknown auth command \"unknown\". Use \"pi auth print-api-key\", \"pi auth print-bearer-token\", or \"pi auth check\".";

    assert_eq!(actual.name(), "Error");
    assert_eq!(actual.message(), expected);
    assert_eq!(actual.to_string(), expected);
    assert!(actual.source().is_none());

    let custom = AuthCommandError::new("custom");
    assert_eq!(custom.name(), "Error");
    assert_eq!(custom.message(), "custom");
}
