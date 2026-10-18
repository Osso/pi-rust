//! Experimental `client` command (`cli/experimental/commands/client.ts`).

use crate::cli::experimental::auth::AuthInput;
use crate::cli::experimental::command::{Command, CommandParseResult, NamedCommandInvocation, ParsedCommandInput};
use crate::cli::experimental::command_options::{
    AUTH_TOKEN_FILE_OPTION, AUTH_TOKEN_OPTION, parse_auth, parse_legacy_options, transport_option,
    unsupported_legacy_options,
};
use crate::cli::experimental::transport_address::TransportAddress;

use crate::cli::experimental::cli::ExperimentalCliContext;

#[derive(Debug, Clone, PartialEq)]
pub struct ClientCommand {
    pub auth: Option<AuthInput>,
    pub connect: Option<TransportAddress>,
}

impl NamedCommandInvocation for ClientCommand {
    fn command(&self) -> &str {
        "client"
    }
}

pub fn client_command() -> Command<ClientCommand, ExperimentalCliContext> {
    let connect_option = transport_option("--connect");
    let mut command: Command<ClientCommand, ExperimentalCliContext> = Command::new("client");
    command
        .option(connect_option.clone())
        .option(AUTH_TOKEN_OPTION.clone())
        .option(AUTH_TOKEN_FILE_OPTION.clone())
        .build(move |input| build_client(input, &connect_option))
        .action(|command, context| (context.run_client)(command));
    command
}

fn build_client(
    input: &ParsedCommandInput<'_>,
    connect_option: &crate::cli::experimental::command::CommandOption<TransportAddress>,
) -> CommandParseResult<ClientCommand> {
    let auth = parse_auth(input);
    let connect = input.value(connect_option);
    let (_, option_errors) = parse_legacy_options(input);
    let mut errors = auth.errors;
    errors.extend(option_errors);
    errors.extend(unsupported_legacy_options("client", input));
    if !errors.is_empty() {
        return CommandParseResult::Errors(errors);
    }
    CommandParseResult::Ok(ClientCommand {
        auth: auth.auth,
        connect,
    })
}
