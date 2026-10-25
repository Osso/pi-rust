//! Coding-agent tools layer; compatibility module paths remain unchanged.

pub mod core;
pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};
pub use pi_coding_agent_data::cli;
pub mod modes {
    pub mod interactive {
        pub mod theme;
        pub mod components {
            pub mod diff;
            pub mod keybinding_hints;
            pub mod visual_truncate;
        }
    }
}
pub mod supervisor {
    pub mod approval_reviewer;
    pub mod client;
    pub mod ensure_running;
    pub mod project_resolver;
    pub mod request_wake;
}
mod callback_errors;

#[cfg(test)]
#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
pub(crate) mod test_oracle;
