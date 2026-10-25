//! Coding-agent storage layer; compatibility module paths remain unchanged.

pub mod core;
pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};
pub use pi_coding_agent_data::cli;
mod callback_errors;

#[cfg(test)]
#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
pub(crate) mod test_oracle;
