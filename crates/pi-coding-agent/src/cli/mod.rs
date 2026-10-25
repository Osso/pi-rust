pub use pi_coding_agent_core::cli::args;

pub mod config_selector;
pub mod control_command;
pub mod debug_command;
pub(crate) mod export_terminal;
pub mod file_processor;
pub mod initial_message;
pub mod list_extensions;
pub mod list_models;
pub mod list_tools;
pub mod login_command;
pub mod project_trust;
pub mod resident_console_command;
pub mod session_picker;
pub mod sessions_command;
pub mod startup_ui;
pub use control_command::{ControlCommandDependencies, ControlCommandOutcome, handle_control_command};
pub use debug_command::{DebugCommandDependencies, DebugCommandOutcome, attach_debug_repl, handle_debug_command};
pub use file_processor::{FileProcessorError, ProcessFileOptions, ProcessedFiles, process_file_arguments};
pub use list_models::{ListModelsIo, ListModelsRuntime, StdioListModelsIo, format_token_count};
pub use list_tools::{format_tool_inventory, list_tools};
pub use login_command::{
    BrowserCliOpener, LoginCommandDependencies, LoginCommandOutcome, LoginIo, RunLoginCommandOptions, StdioLoginIo,
    handle_login_command, run_login_command,
};
pub use project_trust::{CreateProjectTrustContextOptions, ProjectTrustStartupUi, create_project_trust_context};
pub use sessions_command::{SessionsCommandDependencies, SessionsCommandOutcome, handle_sessions_command};
