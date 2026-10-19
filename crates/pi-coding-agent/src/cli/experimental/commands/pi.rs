//! Experimental `pi` command (`cli/experimental/commands/pi.ts`).

use crate::cli::args::Args;
use crate::cli::experimental::auth::AuthInput;
use crate::cli::experimental::command::{
    Command, CommandError, CommandParseResult, NamedCommandInvocation, ParsedCommandInput,
};
use crate::cli::experimental::command_options::{
    AUTH_TOKEN_FILE_OPTION, AUTH_TOKEN_OPTION, parse_auth, parse_legacy_options, transport_option,
};
use crate::cli::experimental::transport_address::TransportAddress;

use crate::cli::experimental::cli::ExperimentalCliContext;

#[derive(Debug, Clone, PartialEq)]
pub struct PiCommand {
    pub auth: Option<AuthInput>,
    pub options: Args,
    pub listen: Vec<TransportAddress>,
}

impl NamedCommandInvocation for PiCommand {
    fn command(&self) -> &str {
        "pi"
    }
}

pub fn pi_command() -> Result<Command<PiCommand, ExperimentalCliContext>, CommandError> {
    let listen_option = transport_option("--listen");
    let mut command: Command<PiCommand, ExperimentalCliContext> = Command::new("pi");
    command
        .option(listen_option.clone())?
        .option(AUTH_TOKEN_OPTION.clone())?
        .option(AUTH_TOKEN_FILE_OPTION.clone())?
        .build(move |input| build_pi(input, &listen_option))
        .action(|command, context| (context.run_pi)(command));
    Ok(command)
}

pub fn build_pi(
    input: &ParsedCommandInput<'_>,
    listen_option: &crate::cli::experimental::command::CommandOption<TransportAddress>,
) -> CommandParseResult<PiCommand> {
    let auth = parse_auth(input);
    let listen = input.values(listen_option);
    let (options, option_errors) = parse_legacy_options(input);
    let mut errors = auth.errors;
    errors.extend(option_errors);
    if options.unknown_flags.contains_key("connect") {
        errors.push("--connect is only valid for client mode".to_owned());
    }
    if !errors.is_empty() {
        return CommandParseResult::Errors(errors);
    }
    CommandParseResult::Ok(PiCommand {
        auth: auth.auth,
        options,
        listen,
    })
}
