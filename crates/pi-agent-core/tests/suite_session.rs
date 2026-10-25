// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "../../pi-ai/tests/support/oracle.rs"]
mod shared_oracle;

#[path = "agent_tool_execution_context.rs"]
pub mod agent_tool_execution_context;
#[path = "compaction.rs"]
pub mod compaction;
#[path = "compaction_details.rs"]
pub mod compaction_details;
#[path = "compaction_errors.rs"]
pub mod compaction_errors;
#[path = "compaction_index_domain.rs"]
pub mod compaction_index_domain;
#[path = "compaction_settings.rs"]
pub mod compaction_settings;
#[path = "compaction_utils.rs"]
pub mod compaction_utils;
#[path = "context_usage_estimate.rs"]
pub mod context_usage_estimate;
#[path = "differential_compaction.rs"]
pub mod differential_compaction;
#[path = "differential_tree_session.rs"]
pub mod differential_tree_session;
#[path = "repin_compaction_utils.rs"]
pub mod repin_compaction_utils;
#[path = "repin_tree_session.rs"]
pub mod repin_tree_session;
#[path = "session_inventory.rs"]
pub mod session_inventory;
#[path = "session_jsonl_faults.rs"]
pub mod session_jsonl_faults;
#[path = "session_number_decode.rs"]
pub mod session_number_decode;
