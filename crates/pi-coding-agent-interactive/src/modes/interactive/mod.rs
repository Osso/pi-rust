//! Ported from `packages/coding-agent/src/modes/interactive/`.

pub use pi_coding_agent_ui::modes::interactive::{components, login_options, model_search};
pub mod coordination_visibility;
pub mod editor_dialog_restore;
pub mod interactive_mode;
pub mod interactive_root_compositor;
pub use pi_coding_agent_core::modes::interactive::theme;
pub mod working_editor;

pub use interactive_mode::{
    ExpandableText, InteractiveMode, InteractiveModeOptions, InteractiveTuiHandle, InteractiveTuiOptions, Renderer,
    create_interactive_tui, format_resume_command, is_dead_terminal_error, run_interactive_mode,
    terminal_error_message,
};
