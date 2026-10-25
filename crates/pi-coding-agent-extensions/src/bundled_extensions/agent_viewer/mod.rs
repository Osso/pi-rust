//! Bundled `agent-viewer` extension (`extensions/agent-viewer/src/index.ts`).

use crate::core::extensions::types::{ExtensionApi, HandlerError};

use super::agents_core::runtime::{MultiAgentExtensionOptions, register_agent_viewer_tools};

pub fn agent_viewer_extension(pi: &ExtensionApi, options: &MultiAgentExtensionOptions) -> Result<(), HandlerError> {
    register_agent_viewer_tools(pi, options)
}
