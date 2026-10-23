pub mod approval_reviewer;
pub mod client;
pub mod ensure_running;
pub mod main;
pub mod project_resolver;
pub mod request_wake;
pub mod response_tool;
pub mod service;

pub use approval_reviewer::{SupervisorApprovalCall, SupervisorHumanReviewer, review_tool_call_with_supervisor};
pub use client::{RequestSupervisorDecisionInput, request_supervisor_decision};
pub use main::{
    CreateSupervisorSessionOptions, RunSupervisorRequestLoopInput, SUPERVISOR_COMPACTION_PERCENT,
    SUPERVISOR_EXCLUDED_TOOL_NAMES, SUPERVISOR_SESSION_ID, SUPERVISOR_TOOL_NAMES, SupervisorConsolePrompt,
    SupervisorConsolePromptQueue, SupervisorSession, block_supervisor_file_access, cleanup_supervisor_provider_context,
    create_supervisor_console_snapshot, create_supervisor_resource_loader, create_supervisor_session,
    create_supervisor_settings_manager, open_supervisor_session, process_supervisor_request,
    read_current_supervisor_response, run_supervisor_request_loop, run_supervisor_service,
    validate_supervisor_extension_load,
};
pub use response_tool::{
    SUPERVISOR_RESPONSE_TOOL_NAME, create_supervisor_response_tool, supervisor_response_parameters,
    supervisor_response_text,
};
pub use service::{
    RunSupervisorRequestInput, RunSupervisorRequestOutcome, SupervisorEvaluator, build_supervisor_prompt,
    parse_supervisor_response, run_supervisor_request,
};
