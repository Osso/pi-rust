// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "abort_reasons.rs"]
mod abort_reasons;
#[path = "agent.rs"]
mod agent;
#[path = "agent_abort.rs"]
mod agent_abort;
#[path = "agent_continue_queue.rs"]
mod agent_continue_queue;
#[path = "agent_harness.rs"]
mod agent_harness;
#[path = "agent_identity.rs"]
mod agent_identity;
#[path = "agent_iterator_return.rs"]
mod agent_iterator_return;
#[path = "agent_loop.rs"]
mod agent_loop;
#[path = "agent_repin.rs"]
mod agent_repin;
#[path = "agent_types.rs"]
mod agent_types;
#[path = "agent_verifier_regressions.rs"]
mod agent_verifier_regressions;
#[path = "branch_summarization.rs"]
mod branch_summarization;
#[path = "branch_summary_message.rs"]
mod branch_summary_message;
#[path = "branch_summary_paths.rs"]
mod branch_summary_paths;
#[path = "cut_point_result.rs"]
mod cut_point_result;
#[path = "differential_agent_harness.rs"]
mod differential_agent_harness;
#[path = "differential_agent_loop.rs"]
mod differential_agent_loop;
#[path = "differential_harness_core.rs"]
mod differential_harness_core;
#[path = "differential_shell_capture.rs"]
mod differential_shell_capture;
#[path = "differential_tree_repository.rs"]
mod differential_tree_repository;
#[path = "differential_uuid.rs"]
mod differential_uuid;
#[path = "harness_env.rs"]
mod harness_env;
#[path = "harness_errors.rs"]
mod harness_errors;
#[path = "harness_failure_boundaries.rs"]
mod harness_failure_boundaries;
#[path = "harness_messages_conversion.rs"]
mod harness_messages_conversion;
#[path = "harness_stream_options.rs"]
mod harness_stream_options;
#[path = "lossless_agent.rs"]
mod lossless_agent;
#[path = "lossless_core_utils.rs"]
mod lossless_core_utils;
#[path = "lossless_prompt_templates.rs"]
mod lossless_prompt_templates;
#[path = "lossless_provider_errors.rs"]
mod lossless_provider_errors;
#[path = "messages_convert.rs"]
mod messages_convert;
#[path = "prompt_template_diagnostic.rs"]
mod prompt_template_diagnostic;
#[path = "prompt_template_loading.rs"]
mod prompt_template_loading;
#[path = "prompt_templates.rs"]
mod prompt_templates;
#[path = "public_entrypoints.rs"]
mod public_entrypoints;
#[path = "repin_harness_messages.rs"]
mod repin_harness_messages;
#[path = "repin_node_lines.rs"]
mod repin_node_lines;
#[path = "resources.rs"]
mod resources;
#[path = "sanitize_binary_output.rs"]
mod sanitize_binary_output;
#[path = "shell_output_capture.rs"]
mod shell_output_capture;
#[path = "skill_formatting.rs"]
mod skill_formatting;
#[path = "skill_loading.rs"]
mod skill_loading;
#[path = "summarization_system_prompt.rs"]
mod summarization_system_prompt;
#[path = "summary_markers.rs"]
mod summary_markers;
#[path = "thinking_level.rs"]
mod thinking_level;
#[path = "uuid.rs"]
mod uuid;
