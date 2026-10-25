//! Interactive coding-agent mode, layered above core and bundled extensions.

pub mod modes {
    pub use pi_coding_agent_core::modes::extension_actions;
    pub mod interactive;
}
pub use core::{ParsedSkillBlock, parse_skill_block};
pub use pi_coding_agent_core::{cli, config, core, utils};
pub use pi_coding_agent_extensions::bundled_extensions;
pub use pi_coding_agent_ui::export_terminal;

#[cfg(test)]
#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
pub(crate) mod test_oracle;
