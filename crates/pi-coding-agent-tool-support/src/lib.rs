//! Shared tool algorithms, extension failures and theme components.

pub mod core;
pub use pi_coding_agent_base::{config, internal_runner, main_runner, utils};
pub mod modes {
    pub mod interactive {
        pub mod theme;
        pub mod components {
            pub mod diff;
            pub mod footer_format;
            pub mod keybinding_hints;
            pub mod visual_truncate;
        }
    }
}
mod callback_errors;
