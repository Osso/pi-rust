// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "agent_tool_execution_context.rs"]
mod agent_tool_execution_context;
#[path = "compaction.rs"]
mod compaction;
#[path = "compaction_details.rs"]
mod compaction_details;
#[path = "compaction_errors.rs"]
mod compaction_errors;
#[path = "compaction_index_domain.rs"]
mod compaction_index_domain;
#[path = "compaction_settings.rs"]
mod compaction_settings;
#[path = "compaction_utils.rs"]
mod compaction_utils;
#[path = "context_usage_estimate.rs"]
mod context_usage_estimate;
#[path = "differential_compaction.rs"]
mod differential_compaction;
#[path = "differential_tree_session.rs"]
mod differential_tree_session;
#[path = "repin_compaction_utils.rs"]
mod repin_compaction_utils;
#[path = "repin_tree_session.rs"]
mod repin_tree_session;
#[path = "session_inventory.rs"]
mod session_inventory;
#[path = "session_jsonl_faults.rs"]
mod session_jsonl_faults;
#[path = "session_number_decode.rs"]
mod session_number_decode;
