// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "settings_manager_drain.rs"]
mod settings_manager_drain;
#[path = "shell.rs"]
mod shell;
#[path = "skill_block.rs"]
mod skill_block;
#[path = "skills.rs"]
mod skills;
#[path = "slash_commands.rs"]
mod slash_commands;
#[path = "sleep.rs"]
mod sleep;
#[path = "source_info.rs"]
mod source_info;
#[path = "standalone_worker_factories.rs"]
mod standalone_worker_factories;
#[path = "standalone_worker_main_process.rs"]
mod standalone_worker_main_process;
#[path = "standalone_worker_runtime.rs"]
mod standalone_worker_runtime;
#[path = "standalone_worker_sdk.rs"]
mod standalone_worker_sdk;
#[path = "status_indicator_api.rs"]
mod status_indicator_api;
#[path = "summary_stream_acquisition.rs"]
mod summary_stream_acquisition;
#[path = "supervisor.rs"]
mod supervisor;
#[path = "supervisor_ensure_running_scenarios.rs"]
mod supervisor_ensure_running_scenarios;
#[path = "supervisor_startup_errors.rs"]
mod supervisor_startup_errors;
#[path = "system_prompt.rs"]
mod system_prompt;
#[path = "telemetry_provider_attribution.rs"]
mod telemetry_provider_attribution;
#[path = "terminal_current_directory.rs"]
mod terminal_current_directory;
#[path = "terminal_outbox_delivery.rs"]
mod terminal_outbox_delivery;
#[path = "test_isolation.rs"]
mod test_isolation;
#[path = "thinking_level_validator.rs"]
mod thinking_level_validator;
#[path = "transport_address.rs"]
mod transport_address;
#[path = "usage_totals.rs"]
mod usage_totals;
#[path = "windows_self_update.rs"]
mod windows_self_update;
