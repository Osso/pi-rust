//! Coding-agent data layer; compatibility module paths remain unchanged.

pub mod core;
pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};
pub mod cli {
    pub mod args;
}
