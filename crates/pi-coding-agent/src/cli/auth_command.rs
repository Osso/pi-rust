const AUTH_CHECK_USAGE: &str = "pi auth check --provider <provider> [--json] [--credentials] [--no-refresh]";
const AUTH_API_KEY_USAGE: &str = "pi auth print-api-key --provider <provider> [--model <model>]";
const AUTH_BEARER_TOKEN_USAGE: &str =
    "pi auth print-bearer-token --provider <provider> [--model <model>] [--min-expiry <duration>]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCommandKind {
    Check,
    ApiKey,
    BearerToken,
}

pub fn get_auth_command_name(kind: AuthCommandKind) -> &'static str {
    match kind {
        AuthCommandKind::Check => "auth check",
        AuthCommandKind::ApiKey => "auth print-api-key",
        AuthCommandKind::BearerToken => "auth print-bearer-token",
    }
}

pub fn get_auth_command_usage(kind: AuthCommandKind) -> &'static str {
    match kind {
        AuthCommandKind::Check => AUTH_CHECK_USAGE,
        AuthCommandKind::ApiKey => AUTH_API_KEY_USAGE,
        AuthCommandKind::BearerToken => AUTH_BEARER_TOKEN_USAGE,
    }
}

pub fn is_auth_command_help(args: &[String]) -> bool {
    if args.first().map(String::as_str) != Some("auth") {
        return false;
    }

    let second_is_missing_or_help = args.get(1).is_none_or(|argument| argument == "help");
    let has_help_flag = args.iter().any(|argument| matches!(argument.as_str(), "--help" | "-h"));

    second_is_missing_or_help || has_help_flag
}
