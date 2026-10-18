//! Bundled `agents-mailbox` extension (`extensions/agents-mailbox/src/index.ts`).

use crate::core::extensions::types::{ExtensionApi, HandlerError};

use super::agents_core::runtime::{MultiAgentExtensionOptions, register_agents_mailbox_tools};

pub fn agents_mailbox_extension(pi: &ExtensionApi, options: &MultiAgentExtensionOptions) -> Result<(), HandlerError> {
    register_agents_mailbox_tools(pi, options)
}
