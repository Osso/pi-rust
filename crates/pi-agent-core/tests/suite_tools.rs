// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "bash_execution_message.rs"]
pub mod bash_execution_message;
#[path = "before_tool_call_result.rs"]
pub mod before_tool_call_result;
#[path = "differential_truncate.rs"]
pub mod differential_truncate;
#[path = "file_info.rs"]
pub mod file_info;
#[path = "file_operations.rs"]
pub mod file_operations;
#[path = "tool_execution_mode.rs"]
pub mod tool_execution_mode;
#[path = "truncate.rs"]
pub mod truncate;
