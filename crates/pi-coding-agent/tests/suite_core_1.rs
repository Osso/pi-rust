// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "abort.rs"]
mod abort;
#[path = "agent_core_differential_unicode_normalize.rs"]
mod agent_core_differential_unicode_normalize;
#[path = "agent_core_fuzzy_match_result.rs"]
mod agent_core_fuzzy_match_result;
#[path = "agent_identity_paths.rs"]
mod agent_identity_paths;
#[path = "agents_core_end_to_end.rs"]
mod agents_core_end_to_end;
#[path = "agents_core_runtime.rs"]
mod agents_core_runtime;
#[path = "agents_core_shared_viewer.rs"]
mod agents_core_shared_viewer;
#[path = "approval_reviewer.rs"]
mod approval_reviewer;
#[path = "architect.rs"]
mod architect;
#[path = "args.rs"]
mod args;
#[path = "auth_phase1.rs"]
mod auth_phase1;
#[path = "auth_phase1_errors.rs"]
mod auth_phase1_errors;
#[path = "auth_phase1_lossless.rs"]
mod auth_phase1_lossless;
#[path = "auth_phase1_public.rs"]
mod auth_phase1_public;
#[path = "auth_phase1_rotation.rs"]
mod auth_phase1_rotation;
#[path = "auth_source_discrepancy.rs"]
mod auth_source_discrepancy;
#[path = "branch_summarization.rs"]
mod branch_summarization;
#[path = "bun_entry.rs"]
mod bun_entry;
#[path = "chalk.rs"]
mod chalk;
#[path = "child_process.rs"]
mod child_process;
#[path = "child_process_kill_signal.rs"]
mod child_process_kill_signal;
#[path = "command.rs"]
mod command;
#[path = "components.rs"]
mod components;
#[path = "config_install.rs"]
mod config_install;
#[path = "config_value_references.rs"]
mod config_value_references;
#[path = "control_command.rs"]
mod control_command;
#[path = "coordination_message.rs"]
mod coordination_message;
#[path = "coordination_visibility.rs"]
mod coordination_visibility;
#[path = "core_constants.rs"]
mod core_constants;
#[path = "custom_operation_errors.rs"]
mod custom_operation_errors;
#[path = "debug_command.rs"]
mod debug_command;
#[path = "defaults.rs"]
mod defaults;
#[path = "deprecation.rs"]
mod deprecation;
#[path = "detached_job_cleanup.rs"]
mod detached_job_cleanup;
#[path = "detached_job_control.rs"]
mod detached_job_control;
#[path = "detached_job_retention.rs"]
mod detached_job_retention;
#[path = "detached_job_runner.rs"]
mod detached_job_runner;
#[path = "detached_pyrun.rs"]
mod detached_pyrun;
#[path = "detached_pyrun_bridge.rs"]
mod detached_pyrun_bridge;
#[path = "detached_pyrun_evaluation.rs"]
mod detached_pyrun_evaluation;
#[path = "differential_accumulator_path_error.rs"]
mod differential_accumulator_path_error;
#[path = "differential_accumulator_sequences.rs"]
mod differential_accumulator_sequences;
#[path = "differential_agent_lifecycle_trace.rs"]
mod differential_agent_lifecycle_trace;
#[path = "differential_agents_core_deterministic.rs"]
mod differential_agents_core_deterministic;
#[path = "differential_architect_prompt.rs"]
mod differential_architect_prompt;
#[path = "differential_architect_request_loop.rs"]
mod differential_architect_request_loop;
#[path = "differential_args.rs"]
mod differential_args;
#[path = "differential_assistant_message.rs"]
mod differential_assistant_message;
#[path = "differential_auth_key_value.rs"]
mod differential_auth_key_value;
#[path = "differential_bordered_loader.rs"]
mod differential_bordered_loader;
#[path = "differential_bwrap.rs"]
mod differential_bwrap;
#[path = "differential_catalog_clock.rs"]
mod differential_catalog_clock;
#[path = "differential_catalog_deadline.rs"]
mod differential_catalog_deadline;
#[path = "differential_catalog_retry_causes.rs"]
mod differential_catalog_retry_causes;
#[path = "differential_catalog_startup.rs"]
mod differential_catalog_startup;
#[path = "differential_chalk.rs"]
mod differential_chalk;
#[path = "differential_change_cwd_stat.rs"]
mod differential_change_cwd_stat;
#[path = "differential_changelog.rs"]
mod differential_changelog;
#[path = "differential_codex_hosted_abort.rs"]
mod differential_codex_hosted_abort;
#[path = "differential_codex_quota.rs"]
mod differential_codex_quota;
