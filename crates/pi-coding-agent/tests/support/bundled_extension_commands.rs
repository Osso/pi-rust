//! Command invocation helpers shared by extension command tests.

use pi_coding_agent::core::extensions::types::{ExtensionCommandContext, HandlerError, RegisteredCommand};

use super::support::Harness;

impl Harness {
    pub fn command(&self, name: &str) -> RegisteredCommand {
        self.extension
            .read()
            .commands
            .get(name)
            .unwrap_or_else(|| panic!("command {name} registered"))
            .clone()
    }

    pub fn command_context(&self) -> ExtensionCommandContext {
        self.runner.create_command_context()
    }

    pub async fn run_command(&self, name: &str, args: &str) -> Result<(), HandlerError> {
        let handler = self.command(name).handler;
        handler(args.to_owned(), self.command_context()).await
    }
}
