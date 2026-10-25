// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "message_timestamp_consumers.rs"]
mod message_timestamp_consumers;
#[path = "messages.rs"]
mod messages;
#[path = "migrations.rs"]
mod migrations;
#[path = "mime.rs"]
mod mime;
#[path = "multi_agent_store.rs"]
mod multi_agent_store;
#[path = "native_error_uncaught.rs"]
mod native_error_uncaught;
#[path = "node_fs_errors.rs"]
mod node_fs_errors;
#[path = "node_realpath.rs"]
mod node_realpath;
#[path = "open_browser.rs"]
mod open_browser;
#[path = "openai_remote_compact.rs"]
mod openai_remote_compact;
#[path = "output_accumulator.rs"]
mod output_accumulator;
#[path = "own_key_tables.rs"]
mod own_key_tables;
#[path = "package_command_execution.rs"]
mod package_command_execution;
#[path = "package_index.rs"]
mod package_index;
#[path = "package_manager.rs"]
mod package_manager;
#[path = "package_manager_progress.rs"]
mod package_manager_progress;
#[path = "path_utils.rs"]
mod path_utils;
#[path = "paths.rs"]
mod paths;
#[path = "permission_rule_store.rs"]
mod permission_rule_store;
#[path = "permissions.rs"]
mod permissions;
#[path = "permissions_prompt.rs"]
mod permissions_prompt;
#[path = "photon.rs"]
mod photon;
#[path = "pi_manifest.rs"]
mod pi_manifest;
#[path = "pi_user_agent.rs"]
mod pi_user_agent;
#[path = "project_resolver.rs"]
mod project_resolver;
#[path = "project_trust.rs"]
mod project_trust;
#[path = "prompt_templates.rs"]
mod prompt_templates;
#[path = "question_relay.rs"]
mod question_relay;
#[path = "raw_app_keybindings.rs"]
mod raw_app_keybindings;
#[path = "raw_error_consumer_components.rs"]
mod raw_error_consumer_components;
#[path = "raw_leaf_components.rs"]
mod raw_leaf_components;
#[path = "raw_resource_diagnostics.rs"]
mod raw_resource_diagnostics;
#[path = "raw_status_indicator.rs"]
mod raw_status_indicator;
#[path = "raw_summary_components.rs"]
mod raw_summary_components;
#[path = "repin_control_agents.rs"]
mod repin_control_agents;
#[path = "repin_control_channel.rs"]
mod repin_control_channel;
#[path = "repin_control_exports.rs"]
mod repin_control_exports;
#[path = "repin_control_lifecycle.rs"]
mod repin_control_lifecycle;
#[path = "repin_control_migrations.rs"]
mod repin_control_migrations;
#[path = "repin_control_raw_json.rs"]
mod repin_control_raw_json;
#[path = "repin_control_reciprocal.rs"]
mod repin_control_reciprocal;
#[path = "repin_control_requests.rs"]
mod repin_control_requests;
#[path = "repin_control_schema.rs"]
mod repin_control_schema;
#[path = "repin_control_schema_constraints.rs"]
mod repin_control_schema_constraints;
#[path = "repin_control_validation.rs"]
mod repin_control_validation;
#[path = "repin_control_whitespace.rs"]
mod repin_control_whitespace;
#[path = "repin_multi_agent_store.rs"]
mod repin_multi_agent_store;
#[path = "repin_store_persistence.rs"]
mod repin_store_persistence;
#[path = "request_wake.rs"]
mod request_wake;
#[path = "resident_console_command.rs"]
mod resident_console_command;
#[path = "resident_console_drift.rs"]
mod resident_console_drift;
#[path = "resident_console_transport.rs"]
mod resident_console_transport;
#[path = "resolve_config_value.rs"]
mod resolve_config_value;
#[path = "resource_loader.rs"]
mod resource_loader;
#[path = "retry_delay.rs"]
mod retry_delay;
#[path = "runtime_credentials.rs"]
mod runtime_credentials;
#[path = "runtime_process.rs"]
mod runtime_process;
#[path = "sdk.rs"]
mod sdk;
#[path = "self_restart_state.rs"]
mod self_restart_state;
#[path = "settings_manager.rs"]
mod settings_manager;
