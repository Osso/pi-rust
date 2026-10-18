pub mod args;
mod auth_command;
pub mod experimental;

pub mod auth_check;
pub mod control_command;
pub mod credential_print;
pub mod debug_command;
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
pub use auth_check::{
    AuthCheckReason, AuthCheckResult, AuthCheckStatus, CheckProviderAuthOptions, check_provider_auth,
    create_auth_check_model_runtime, get_provider_credential,
};
pub use auth_command::{
    AuthCommand, AuthCommandError, AuthCommandKind, AuthCommandTarget, get_auth_command_name, get_auth_command_usage,
    get_auth_credential, is_auth_command_help, parse_auth_command, print_auth_command_help, validate_auth_command_args,
};
pub use control_command::{ControlCommandDependencies, ControlCommandOutcome, handle_control_command};
pub use credential_print::{CredentialPrintError, CredentialPrintKind, resolve_credential_for_print};
pub use debug_command::{DebugCommandDependencies, DebugCommandOutcome, attach_debug_repl, handle_debug_command};
pub use file_processor::{FileProcessorError, ProcessFileOptions, ProcessedFiles, process_file_arguments};
pub use list_models::{ListModelsOutput, ListModelsRuntime, format_token_count};
pub use list_tools::{format_tool_inventory, list_tools};
pub use login_command::{
    BrowserCliOpener, LoginCommandDependencies, LoginCommandOutcome, LoginIo, RunLoginCommandOptions, StdioLoginIo,
    handle_login_command, run_login_command,
};
pub use project_trust::{CreateProjectTrustContextOptions, ProjectTrustStartupUi, create_project_trust_context};
pub use sessions_command::{SessionsCommandDependencies, SessionsCommandOutcome, handle_sessions_command};
