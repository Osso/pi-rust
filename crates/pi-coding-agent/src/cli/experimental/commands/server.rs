//! Experimental `server` command (`cli/experimental/commands/server.ts`).

use crate::cli::experimental::auth::AuthInput;
use crate::cli::experimental::command::{Command, CommandParseResult, NamedCommandInvocation, ParsedCommandInput};
use crate::cli::experimental::command_options::{
    AUTH_TOKEN_FILE_OPTION, AUTH_TOKEN_OPTION, parse_auth, parse_legacy_options, transport_option,
    unsupported_legacy_options,
};
use crate::cli::experimental::transport_address::TransportAddress;

use crate::cli::experimental::cli::ExperimentalCliContext;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerCommand {
    pub auth: Option<AuthInput>,
    pub listen: Vec<TransportAddress>,
}

impl NamedCommandInvocation for ServerCommand {
    fn command(&self) -> &str {
        "server"
    }
}

pub fn server_command() -> Command<ServerCommand, ExperimentalCliContext> {
    let listen_option = transport_option("--listen");
    let mut command: Command<ServerCommand, ExperimentalCliContext> = Command::new("server");
    command
        .option(listen_option.clone())
        .option(AUTH_TOKEN_OPTION.clone())
        .option(AUTH_TOKEN_FILE_OPTION.clone())
        .build(move |input| build_server(input, &listen_option))
        .action(|command, context| (context.run_server)(command));
    command
}

fn build_server(
    input: &ParsedCommandInput<'_>,
    listen_option: &crate::cli::experimental::command::CommandOption<TransportAddress>,
) -> CommandParseResult<ServerCommand> {
    let auth = parse_auth(input);
    let listen = input.values(listen_option);
    let (_, option_errors) = parse_legacy_options(input);
    let mut errors = auth.errors;
    errors.extend(option_errors);
    errors.extend(unsupported_legacy_options("server", input));
    if !errors.is_empty() {
        return CommandParseResult::Errors(errors);
    }
    CommandParseResult::Ok(ServerCommand {
        auth: auth.auth,
        listen,
    })
}
