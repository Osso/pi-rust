//! Coding-agent core and the shared runtime/UI leaves it consumes.

mod callback_errors;
pub mod cli {
    pub mod args;
}
pub mod core;
pub mod modes {
    pub mod extension_actions;
    pub mod interactive {
        pub mod theme;
        pub mod components {
            pub mod diff;
            pub mod footer;
            pub mod footer_format;
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

pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};

#[cfg(test)]
#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
pub(crate) mod test_oracle;
