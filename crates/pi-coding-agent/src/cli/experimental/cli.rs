//! Experimental CLI composition (`cli/experimental/cli.ts`).

use super::command::{Command, CommandError, NamedCommandInvocation};
use super::command_options::{AUTH_TOKEN_FILE_OPTION, AUTH_TOKEN_OPTION, transport_option};
use super::commands::client::{ClientCommand, client_command};
use super::commands::pi::{PiCommand, build_pi};
use super::commands::server::{ServerCommand, server_command};

/// Invocation union produced by [`experimental_cli`].
#[derive(Debug, Clone, PartialEq)]
pub enum ExperimentalCommand {
    Pi(Box<PiCommand>),
    Server(ServerCommand),
    Client(ClientCommand),
}

impl NamedCommandInvocation for ExperimentalCommand {
    fn command(&self) -> &str {
        match self {
            Self::Pi(command) => command.command(),
            Self::Server(command) => command.command(),
            Self::Client(command) => command.command(),
        }
    }
}

/// Callbacks run by the experimental command actions.
pub struct ExperimentalCliContext {
    pub run_pi: Box<dyn FnMut(&PiCommand)>,
    pub run_server: Box<dyn FnMut(&ServerCommand)>,
    pub run_client: Box<dyn FnMut(&ClientCommand)>,
}

/// `experimentalCli`: `pi` root with `server` and `client` subcommands.
pub fn experimental_cli() -> Result<Command<ExperimentalCommand, ExperimentalCliContext>, CommandError> {
    let listen_option = transport_option("--listen");
    let mut cli: Command<ExperimentalCommand, ExperimentalCliContext> = Command::new("pi");
    cli.option(listen_option.clone())?
        .option(AUTH_TOKEN_OPTION.clone())?
        .option(AUTH_TOKEN_FILE_OPTION.clone())?
        .build(move |input| match build_pi(input, &listen_option) {
            super::command::CommandParseResult::Ok(command) => {
                super::command::CommandParseResult::Ok(ExperimentalCommand::Pi(Box::new(command)))
            }
            super::command::CommandParseResult::Errors(errors) => super::command::CommandParseResult::Errors(errors),
        })
        .action(|command, context| {
            if let ExperimentalCommand::Pi(command) = command {
                (context.run_pi)(command);
            }
        })
        .command(server_command()?, ExperimentalCommand::Server)?
        .command(client_command()?, ExperimentalCommand::Client)?;
    Ok(cli)
}
