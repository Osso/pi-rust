// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "agent_session.rs"]
mod agent_session;
#[path = "agent_session_mutation_target.rs"]
mod agent_session_mutation_target;
#[path = "agent_session_runtime_mailbox.rs"]
mod agent_session_runtime_mailbox;
#[path = "agent_session_services.rs"]
mod agent_session_services;
#[path = "agent_session_signal_wake.rs"]
mod agent_session_signal_wake;
#[path = "agents_core_session_factories.rs"]
mod agents_core_session_factories;
#[path = "approved_mailbox_deltas.rs"]
mod approved_mailbox_deltas;
#[path = "approved_transcript_delta.rs"]
mod approved_transcript_delta;
#[path = "auth_storage.rs"]
mod auth_storage;
#[path = "auth_storage_cross_process.rs"]
mod auth_storage_cross_process;
#[path = "codex_restriction_context.rs"]
mod codex_restriction_context;
#[path = "compaction.rs"]
mod compaction;
#[path = "compaction_generation.rs"]
mod compaction_generation;
#[path = "compaction_utils.rs"]
mod compaction_utils;
#[path = "context_tool.rs"]
mod context_tool;
#[path = "control_db_public_api.rs"]
mod control_db_public_api;
#[path = "cross_implementation_control_db.rs"]
mod cross_implementation_control_db;
#[path = "current_session_version.rs"]
mod current_session_version;
#[path = "differential_agent_session.rs"]
mod differential_agent_session;
#[path = "differential_compaction.rs"]
mod differential_compaction;
#[path = "differential_control_db.rs"]
mod differential_control_db;
#[path = "differential_current_session_version.rs"]
mod differential_current_session_version;
#[path = "differential_idle_compaction.rs"]
mod differential_idle_compaction;
#[path = "differential_session_info_dates.rs"]
mod differential_session_info_dates;
#[path = "differential_session_manager.rs"]
mod differential_session_manager;
#[path = "differential_session_search.rs"]
mod differential_session_search;
#[path = "differential_session_search_surface.rs"]
mod differential_session_search_surface;
#[path = "empty_session_cleanup.rs"]
mod empty_session_cleanup;
#[path = "external_session_importer.rs"]
mod external_session_importer;
#[path = "lossless_agent_session_paths.rs"]
mod lossless_agent_session_paths;
#[path = "lossless_runtime_mailbox.rs"]
mod lossless_runtime_mailbox;
#[path = "lossless_session.rs"]
mod lossless_session;
#[path = "lossless_session_persistence.rs"]
mod lossless_session_persistence;
#[path = "lossless_session_projections.rs"]
mod lossless_session_projections;
#[path = "lossless_session_records.rs"]
mod lossless_session_records;
#[path = "readonly_session_manager.rs"]
mod readonly_session_manager;
#[path = "repin_control_mailbox.rs"]
mod repin_control_mailbox;
#[path = "repin_control_sessions.rs"]
mod repin_control_sessions;
#[path = "repin_resident_session.rs"]
mod repin_resident_session;
#[path = "repin_runtime_mailbox_transcript.rs"]
mod repin_runtime_mailbox_transcript;
#[path = "repin_storage_history.rs"]
mod repin_storage_history;
#[path = "repin_storage_retention.rs"]
mod repin_storage_retention;
#[path = "repin_storage_terminal_outbox.rs"]
mod repin_storage_terminal_outbox;
#[path = "repin_storage_tool_output.rs"]
mod repin_storage_tool_output;
#[path = "repin_storage_transcript.rs"]
mod repin_storage_transcript;
#[path = "resume_session_tool.rs"]
mod resume_session_tool;
#[path = "rpc_mode_session_name.rs"]
mod rpc_mode_session_name;
#[path = "search_current_session_history_tool.rs"]
mod search_current_session_history_tool;
#[path = "session_autoname.rs"]
mod session_autoname;
#[path = "session_continuation.rs"]
mod session_continuation;
#[path = "session_control_db.rs"]
mod session_control_db;
#[path = "session_coordination_tools.rs"]
mod session_coordination_tools;
#[path = "session_cwd.rs"]
mod session_cwd;
#[path = "session_directory.rs"]
mod session_directory;
#[path = "session_health.rs"]
mod session_health;
#[path = "session_manager.rs"]
mod session_manager;
#[path = "session_picker.rs"]
mod session_picker;
#[path = "session_tool_output.rs"]
mod session_tool_output;
#[path = "session_tool_output_constants.rs"]
mod session_tool_output_constants;
#[path = "sessions_command.rs"]
mod sessions_command;
