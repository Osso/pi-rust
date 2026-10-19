//! Tool-definition accessor shared by extension tool tests.

use pi_coding_agent::core::extensions::types::ToolDefinition;

use super::support::Harness;

impl Harness {
    pub fn tool(&self, name: &str) -> ToolDefinition {
        self.extension
            .tools
            .get(name)
            .unwrap_or_else(|| panic!("tool {name} registered"))
            .definition
            .clone()
    }
}
