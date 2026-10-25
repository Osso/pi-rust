// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "frontmatter.rs"]
mod frontmatter;
#[path = "fs_watch.rs"]
mod fs_watch;
#[path = "git.rs"]
mod git;
#[path = "git_project.rs"]
mod git_project;
#[path = "git_worktree.rs"]
mod git_worktree;
#[path = "host_relay.rs"]
mod host_relay;
#[path = "host_relay_protocol.rs"]
mod host_relay_protocol;
#[path = "host_relay_resident.rs"]
mod host_relay_resident;
#[path = "host_relay_store.rs"]
mod host_relay_store;
#[path = "html.rs"]
mod html;
#[path = "http_dispatcher.rs"]
mod http_dispatcher;
#[path = "image_content_roundtrip.rs"]
mod image_content_roundtrip;
#[path = "image_convert.rs"]
mod image_convert;
#[path = "image_process.rs"]
mod image_process;
#[path = "image_resize.rs"]
mod image_resize;
#[path = "initial_message.rs"]
mod initial_message;
#[path = "iso_time.rs"]
mod iso_time;
#[path = "js_json.rs"]
mod js_json;
#[path = "js_value_artifacts.rs"]
mod js_value_artifacts;
#[path = "json_event.rs"]
mod json_event;
#[path = "json_utils.rs"]
mod json_utils;
#[path = "keybindings.rs"]
mod keybindings;
#[path = "lifecycle_coordinator.rs"]
mod lifecycle_coordinator;
#[path = "login_command.rs"]
mod login_command;
#[path = "lossless_agent_node.rs"]
mod lossless_agent_node;
#[path = "lossless_architect.rs"]
mod lossless_architect;
#[path = "lossless_architect_input.rs"]
mod lossless_architect_input;
#[path = "lossless_argument_completions.rs"]
mod lossless_argument_completions;
#[path = "lossless_code_index.rs"]
mod lossless_code_index;
#[path = "lossless_control_claim_order.rs"]
mod lossless_control_claim_order;
#[path = "lossless_control_json_authority.rs"]
mod lossless_control_json_authority;
#[path = "lossless_control_response_parsers.rs"]
mod lossless_control_response_parsers;
#[path = "lossless_coordination_format.rs"]
mod lossless_coordination_format;
#[path = "lossless_detached_paths.rs"]
mod lossless_detached_paths;
#[path = "lossless_git_source.rs"]
mod lossless_git_source;
#[path = "lossless_incoming_content.rs"]
mod lossless_incoming_content;
#[path = "lossless_incoming_error.rs"]
mod lossless_incoming_error;
#[path = "lossless_local_path_validation.rs"]
mod lossless_local_path_validation;
#[path = "lossless_mcp_permission.rs"]
mod lossless_mcp_permission;
#[path = "lossless_migrations.rs"]
mod lossless_migrations;
#[path = "lossless_package_contracts.rs"]
mod lossless_package_contracts;
#[path = "lossless_package_manager.rs"]
mod lossless_package_manager;
#[path = "lossless_paths.rs"]
mod lossless_paths;
#[path = "lossless_project_trust.rs"]
mod lossless_project_trust;
#[path = "lossless_provider_errors.rs"]
mod lossless_provider_errors;
#[path = "lossless_pyrun.rs"]
mod lossless_pyrun;
#[path = "lossless_resident_console.rs"]
mod lossless_resident_console;
#[path = "lossless_resource_loader.rs"]
mod lossless_resource_loader;
#[path = "lossless_resources.rs"]
mod lossless_resources;
#[path = "lossless_retention.rs"]
mod lossless_retention;
#[path = "lossless_rule_store.rs"]
mod lossless_rule_store;
#[path = "lossless_runtime_identity.rs"]
mod lossless_runtime_identity;
#[path = "lossless_settings.rs"]
mod lossless_settings;
#[path = "lossless_supervisor_main.rs"]
mod lossless_supervisor_main;
#[path = "lossless_supervisor_project.rs"]
mod lossless_supervisor_project;
#[path = "lossless_supervisor_wake.rs"]
mod lossless_supervisor_wake;
#[path = "lossless_version_check.rs"]
mod lossless_version_check;
#[path = "main.rs"]
mod main;
#[path = "main_process.rs"]
mod main_process;
#[path = "management_http.rs"]
mod management_http;
