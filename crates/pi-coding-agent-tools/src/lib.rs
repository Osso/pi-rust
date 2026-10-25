//! Coding-agent tools layer; compatibility module paths remain unchanged.

pub mod core;
pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};
pub mod cli;
pub use pi_coding_agent_tool_support::modes;
pub mod supervisor {
    pub mod approval_reviewer;
    pub mod client;
    pub mod ensure_running;
    pub mod project_resolver;
    pub mod request_wake;
}

#[cfg(test)]
#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
pub(crate) mod test_oracle;
