//! Shared experimental CLI options (`cli/experimental/command-options.ts`).

use std::sync::LazyLock;

use super::auth::{ParsedAuthInput, RawAuthOptions, parse_auth_input};
use super::command::{CommandOption, ParsedCommandInput, string_option, value_option};
use super::transport_address::{TransportAddress, parse_transport_address};
use crate::cli::args::Args;
use crate::cli::args::parse_args;

pub static AUTH_TOKEN_OPTION: LazyLock<CommandOption<String>> = LazyLock::new(|| string_option("--auth-token"));
pub static AUTH_TOKEN_FILE_OPTION: LazyLock<CommandOption<String>> =
    LazyLock::new(|| string_option("--auth-token-file"));

/// `--listen`/`--connect` address option.
pub fn transport_option(name: &'static str) -> CommandOption<TransportAddress> {
    value_option(name, move |value| parse_transport_address(value, name))
}

pub fn parse_auth(input: &ParsedCommandInput<'_>) -> ParsedAuthInput {
    parse_auth_input(&RawAuthOptions {
        auth_token: input.value(&AUTH_TOKEN_OPTION),
        auth_token_file: input.value(&AUTH_TOKEN_FILE_OPTION),
    })
}

/// Parse the legacy CLI options left in `remainingArgs`.
pub fn parse_legacy_options(input: &ParsedCommandInput<'_>) -> (Args, Vec<String>) {
    let options = parse_args(input.remaining_args);
    let errors = options
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind == crate::cli::args::DiagnosticType::Error)
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    (options, errors)
}

pub fn unsupported_legacy_options(command: &str, input: &ParsedCommandInput<'_>) -> Vec<String> {
    if input.remaining_args.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "The experimental {command} command does not support existing CLI options yet"
    )]
}
