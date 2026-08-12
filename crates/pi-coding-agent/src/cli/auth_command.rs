use std::error::Error;
use std::fmt::{self, Display, Formatter};

const AUTH_CHECK_USAGE: &str = "pi auth check --provider <provider> [--json] [--credentials] [--no-refresh]";
const AUTH_API_KEY_USAGE: &str = "pi auth print-api-key --provider <provider> [--model <model>]";
const AUTH_BEARER_TOKEN_USAGE: &str =
    "pi auth print-bearer-token --provider <provider> [--model <model>] [--min-expiry <duration>]";
const UNKNOWN_AUTH_COMMAND_SUFFIX: &str =
    ". Use \"pi auth print-api-key\", \"pi auth print-bearer-token\", or \"pi auth check\".";
const MIN_EXPIRY_KIND_ERROR: &str = "--min-expiry is only supported by print-bearer-token";
const MIN_EXPIRY_FORMAT_ERROR: &str = "--min-expiry must use a duration such as 30m or 1h";
const MILLISECONDS_PER_SECOND: f64 = 1_000.0;
const MILLISECONDS_PER_MINUTE: f64 = 60_000.0;
const MILLISECONDS_PER_HOUR: f64 = 3_600_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCommandKind {
    Check,
    ApiKey,
    BearerToken,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthCommand {
    kind: AuthCommandKind,
    args: Vec<String>,
    json: bool,
    credentials: bool,
    no_refresh: bool,
    min_expiry_ms: Option<f64>,
}

impl AuthCommand {
    fn new(kind: AuthCommandKind) -> Self {
        Self {
            kind,
            args: Vec::new(),
            json: false,
            credentials: false,
            no_refresh: false,
            min_expiry_ms: None,
        }
    }

    pub const fn kind(&self) -> AuthCommandKind {
        self.kind
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub const fn json(&self) -> bool {
        self.json
    }

    pub const fn credentials(&self) -> bool {
        self.credentials
    }

    pub const fn no_refresh(&self) -> bool {
        self.no_refresh
    }

    pub const fn min_expiry_ms(&self) -> Option<f64> {
        self.min_expiry_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCommandError {
    message: String,
}

impl AuthCommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub const fn name(&self) -> &'static str {
        "Error"
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for AuthCommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AuthCommandError {}

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

pub fn parse_auth_command(args: &[String]) -> Result<Option<AuthCommand>, AuthCommandError> {
    if args.first().map(String::as_str) != Some("auth") {
        return Ok(None);
    }

    let kind = parse_auth_command_kind(args.get(1).map(String::as_str))?;
    parse_auth_arguments(kind, &args[2..]).map(Some)
}

fn parse_auth_command_kind(subcommand: Option<&str>) -> Result<AuthCommandKind, AuthCommandError> {
    match subcommand {
        Some("check") => Ok(AuthCommandKind::Check),
        Some("print-api-key") => Ok(AuthCommandKind::ApiKey),
        Some("print-bearer-token") => Ok(AuthCommandKind::BearerToken),
        Some(subcommand) => Err(unknown_auth_command_error(subcommand)),
        None => Err(unknown_auth_command_error("")),
    }
}

fn unknown_auth_command_error(subcommand: &str) -> AuthCommandError {
    AuthCommandError::new(format!(
        "Unknown auth command \"{subcommand}\"{UNKNOWN_AUTH_COMMAND_SUFFIX}"
    ))
}

fn parse_auth_arguments(kind: AuthCommandKind, arguments: &[String]) -> Result<AuthCommand, AuthCommandError> {
    let mut command = AuthCommand::new(kind);
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        match argument.as_str() {
            "--min-expiry" => {
                command.min_expiry_ms = Some(parse_min_expiry(kind, arguments.get(index + 1))?);
                index += 2;
            }
            "--json" => {
                ensure_check_flag(kind, argument)?;
                command.json = true;
                index += 1;
            }
            "--credentials" => {
                ensure_check_flag(kind, argument)?;
                command.credentials = true;
                index += 1;
            }
            "--no-refresh" => {
                ensure_check_flag(kind, argument)?;
                command.no_refresh = true;
                index += 1;
            }
            _ => {
                command.args.push(argument.clone());
                index += 1;
            }
        }
    }

    Ok(command)
}

fn ensure_check_flag(kind: AuthCommandKind, flag: &str) -> Result<(), AuthCommandError> {
    if kind == AuthCommandKind::Check {
        return Ok(());
    }

    Err(AuthCommandError::new(format!("{flag} is only supported by auth check")))
}

fn parse_min_expiry(kind: AuthCommandKind, value: Option<&String>) -> Result<f64, AuthCommandError> {
    if kind != AuthCommandKind::BearerToken {
        return Err(AuthCommandError::new(MIN_EXPIRY_KIND_ERROR));
    }

    let value = value.ok_or_else(invalid_min_expiry_error)?;
    parse_duration_ms(value).ok_or_else(invalid_min_expiry_error)
}

fn invalid_min_expiry_error() -> AuthCommandError {
    AuthCommandError::new(MIN_EXPIRY_FORMAT_ERROR)
}

fn parse_duration_ms(value: &str) -> Option<f64> {
    let digit_bytes = value.bytes().take_while(u8::is_ascii_digit).count();
    if digit_bytes == 0 {
        return None;
    }

    let (amount, unit) = value.split_at(digit_bytes);
    if !is_source_duration_unit(unit) {
        return None;
    }

    let amount = amount.parse::<f64>().ok()?;
    Some(amount * duration_multiplier(unit))
}

fn is_source_duration_unit(unit: &str) -> bool {
    matches!(
        unit,
        "ms" | "mS" | "mſ" | "Ms" | "MS" | "Mſ" | "s" | "S" | "ſ" | "m" | "M" | "h" | "H"
    )
}

fn duration_multiplier(unit: &str) -> f64 {
    match unit {
        "ms" => 1.0,
        "s" => MILLISECONDS_PER_SECOND,
        "m" => MILLISECONDS_PER_MINUTE,
        _ => MILLISECONDS_PER_HOUR,
    }
}
