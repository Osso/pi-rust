pub mod args;
mod auth_command;
pub mod experimental;

pub mod auth_check;
pub mod control_command;
pub mod credential_print;
pub mod debug_command;
pub mod initial_message;
pub mod sessions_command;
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
pub use sessions_command::{SessionsCommandDependencies, SessionsCommandOutcome, handle_sessions_command};
