use pi_coding_agent::cli::{AuthCommand, AuthCommandKind, parse_auth_command};
use serde_json::{Value, json};

use super::fixtures::Fixture;

pub(crate) fn all() -> Vec<Fixture> {
    [
        non_auth_and_defaults(),
        subcommand_errors(),
        passthrough_arguments(),
        min_expiry_kind_errors(),
        duration_format_errors(),
        check_flag_errors(),
        lowercase_duration_units(),
        source_bug_units(),
        numeric_boundaries(),
        late_error_and_long_s(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn non_auth_and_defaults() -> Vec<Fixture> {
    vec![
        fixture(
            12014,
            "non-auth arguments",
            args(&["status", "auth", "--help", "--min-expiry"]),
        ),
        fixture(12015, "check defaults", args(&["auth", "check"])),
        fixture(12016, "api-key defaults", args(&["auth", "print-api-key"])),
        fixture(12017, "bearer defaults", args(&["auth", "print-bearer-token"])),
    ]
}

fn subcommand_errors() -> Vec<Fixture> {
    vec![
        fixture(12018, "missing subcommand", args(&["auth"])),
        fixture(12019, "unknown Unicode and NUL subcommand", args(&["auth", "hé\0lp"])),
    ]
}

fn passthrough_arguments() -> Vec<Fixture> {
    vec![
        check_passthrough_fixture(),
        api_key_passthrough_fixture(),
        bearer_repeated_expiry_fixture(),
    ]
}

fn check_passthrough_fixture() -> Fixture {
    fixture(
        12020,
        "check flags and passthrough arguments",
        args(&[
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
        ]),
    )
}

fn api_key_passthrough_fixture() -> Fixture {
    fixture(
        12021,
        "api-key passthrough arguments",
        args(&[
            "auth",
            "print-api-key",
            "--provider",
            "example",
            "--unknown",
            "value",
            "--help",
            "--unknown",
            "value",
        ]),
    )
}

fn bearer_repeated_expiry_fixture() -> Fixture {
    fixture(
        12022,
        "bearer repeated expiry last wins",
        args(&[
            "auth",
            "print-bearer-token",
            "--provider",
            "example",
            "--min-expiry",
            "1s",
            "--model",
            "model",
            "--min-expiry",
            "2m",
        ]),
    )
}

fn min_expiry_kind_errors() -> Vec<Fixture> {
    vec![
        fixture(
            12023,
            "check min-expiry missing value",
            args(&["auth", "check", "--min-expiry"]),
        ),
        fixture(
            12024,
            "api-key min-expiry invalid",
            args(&["auth", "print-api-key", "--min-expiry", "invalid"]),
        ),
        fixture(
            12025,
            "bearer min-expiry missing value",
            args(&["auth", "print-bearer-token", "--min-expiry"]),
        ),
    ]
}

fn duration_format_errors() -> Vec<Fixture> {
    vec![
        duration(12026, "Unicode digit duration", "١m"),
        duration(12027, "decimal duration", "1.5m"),
        duration(12028, "whitespace duration", "1 m"),
        duration(12029, "NUL-suffixed duration", "1m\0"),
    ]
}

fn check_flag_errors() -> Vec<Fixture> {
    vec![
        fixture(12030, "api-key JSON flag", args(&["auth", "print-api-key", "--json"])),
        fixture(
            12031,
            "bearer credentials flag",
            args(&["auth", "print-bearer-token", "--credentials"]),
        ),
        fixture(
            12032,
            "api-key no-refresh flag",
            args(&["auth", "print-api-key", "--no-refresh"]),
        ),
        fixture(
            12033,
            "bearer valid expiry then JSON flag",
            args(&["auth", "print-bearer-token", "--min-expiry", "1h", "--json"]),
        ),
    ]
}

fn lowercase_duration_units() -> Vec<Fixture> {
    vec![
        duration(12034, "millisecond duration", "1ms"),
        duration(12035, "second duration", "1s"),
        duration(12036, "minute duration", "1m"),
        duration(12037, "hour duration", "1h"),
    ]
}

fn source_bug_units() -> Vec<Fixture> {
    vec![
        duration(12038, "uppercase seconds source bug", "1S"),
        duration(12039, "mixed long-s source bug", "1Mſ"),
        duration(12040, "uppercase milliseconds source bug", "1MS"),
    ]
}

fn numeric_boundaries() -> Vec<Fixture> {
    vec![
        duration(12041, "leading-zero duration", "000ms"),
        duration(12042, "maximum safe integer milliseconds", "9007199254740991ms"),
        duration(12043, "unsafe integer milliseconds", "9007199254740993ms"),
        duration(12044, "maximum safe integer seconds", "9007199254740991s"),
        fixture(12045, "308 digits milliseconds", repeated_digit_duration(308, "ms")),
        fixture(12046, "309 digits milliseconds", repeated_digit_duration(309, "ms")),
        fixture(12047, "308 digits hours", repeated_digit_duration(308, "h")),
    ]
}

fn late_error_and_long_s() -> Vec<Fixture> {
    vec![
        fixture(
            12048,
            "valid then invalid repeated expiry",
            args(&[
                "auth",
                "print-bearer-token",
                "--min-expiry",
                "1s",
                "--min-expiry",
                "invalid",
            ]),
        ),
        duration(12049, "long-s duration", "1ſ"),
    ]
}

fn duration(id: u64, name: &'static str, value: &str) -> Fixture {
    fixture(id, name, args(&["auth", "print-bearer-token", "--min-expiry", value]))
}

fn repeated_digit_duration(count: usize, unit: &str) -> Vec<String> {
    let value = format!("{}{unit}", "9".repeat(count));
    args(&["auth", "print-bearer-token", "--min-expiry", &value])
}

fn fixture(id: u64, name: &'static str, args: Vec<String>) -> Fixture {
    let expected = expected_response(id, &args);
    let request = json!({
        "id": id,
        "operation": "parse_auth_command",
        "args": args,
    });
    Fixture {
        name,
        request,
        expected,
    }
}

fn expected_response(id: u64, args: &[String]) -> Value {
    match parse_auth_command(args) {
        Ok(None) => json!({"id": id, "ok": true, "output": null}),
        Ok(Some(command)) => command_response(id, &command),
        Err(error) => json!({
            "id": id,
            "ok": false,
            "error": {"name": error.name(), "message": error.message()},
        }),
    }
}

fn command_response(id: u64, command: &AuthCommand) -> Value {
    let mut output = json!({
        "kind": command_kind_name(command.kind()),
        "args": command.args(),
        "json": command.json(),
        "credentials": command.credentials(),
        "noRefresh": command.no_refresh(),
    });
    if let Some(expiry) = command.min_expiry_ms() {
        output["minExpiryMsBits"] = json!(format!("{:016x}", expiry.to_bits()));
    }

    json!({"id": id, "ok": true, "output": output})
}

fn command_kind_name(kind: AuthCommandKind) -> &'static str {
    match kind {
        AuthCommandKind::Check => "check",
        AuthCommandKind::ApiKey => "api_key",
        AuthCommandKind::BearerToken => "bearer_token",
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
