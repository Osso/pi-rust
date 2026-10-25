// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "bundled_commands.rs"]
mod bundled_commands;
#[path = "bundled_extensions_gates.rs"]
mod bundled_extensions_gates;
#[path = "bundled_extensions_tools.rs"]
mod bundled_extensions_tools;
#[path = "claude_bash_hook_extension.rs"]
mod claude_bash_hook_extension;
#[path = "codex_usage_extension.rs"]
mod codex_usage_extension;
#[path = "differential_bundled_extensions.rs"]
mod differential_bundled_extensions;
#[path = "differential_goal_extension.rs"]
mod differential_goal_extension;
#[path = "differential_list_extensions.rs"]
mod differential_list_extensions;
#[path = "example_built_in_tool_renderer.rs"]
mod example_built_in_tool_renderer;
#[path = "example_claude_rules.rs"]
mod example_claude_rules;
#[path = "example_commands.rs"]
mod example_commands;
#[path = "example_custom_anthropic.rs"]
mod example_custom_anthropic;
#[path = "example_doom_engine_failures.rs"]
mod example_doom_engine_failures;
#[path = "example_doom_overlay.rs"]
mod example_doom_overlay;
#[path = "example_doom_overlay_helpers.rs"]
mod example_doom_overlay_helpers;
#[path = "example_dynamic_resources.rs"]
mod example_dynamic_resources;
#[path = "example_handoff.rs"]
mod example_handoff;
#[path = "example_minimal_mode.rs"]
mod example_minimal_mode;
#[path = "example_plan_mode.rs"]
mod example_plan_mode;
#[path = "example_plan_mode_utils.rs"]
mod example_plan_mode_utils;
#[path = "example_preset.rs"]
mod example_preset;
#[path = "example_project_trust.rs"]
mod example_project_trust;
#[path = "example_prompt_customizer.rs"]
mod example_prompt_customizer;
#[path = "example_provider_payload.rs"]
mod example_provider_payload;
#[path = "example_question.rs"]
mod example_question;
#[path = "example_questionnaire.rs"]
mod example_questionnaire;
#[path = "example_rpc_demo.rs"]
mod example_rpc_demo;
#[path = "example_rpc_extension_ui.rs"]
mod example_rpc_extension_ui;
#[path = "example_sdk_api_keys_and_oauth.rs"]
mod example_sdk_api_keys_and_oauth;
#[path = "example_sdk_context_files.rs"]
mod example_sdk_context_files;
#[path = "example_sdk_custom_model.rs"]
mod example_sdk_custom_model;
#[path = "example_sdk_extensions.rs"]
mod example_sdk_extensions;
#[path = "example_sdk_prompt_templates.rs"]
mod example_sdk_prompt_templates;
#[path = "example_sdk_sessions.rs"]
mod example_sdk_sessions;
#[path = "example_sdk_settings.rs"]
mod example_sdk_settings;
#[path = "example_sdk_skills.rs"]
mod example_sdk_skills;
#[path = "example_sdk_tools.rs"]
mod example_sdk_tools;
#[path = "example_snake.rs"]
mod example_snake;
#[path = "example_space_invaders.rs"]
mod example_space_invaders;
#[path = "example_subagent_agents.rs"]
mod example_subagent_agents;
#[path = "example_summarize.rs"]
mod example_summarize;
#[path = "example_todo.rs"]
mod example_todo;
#[path = "example_trigger_compact.rs"]
mod example_trigger_compact;
#[path = "extensions.rs"]
mod extensions;
#[path = "goal_extension.rs"]
mod goal_extension;
#[path = "lossless_extension_loader.rs"]
mod lossless_extension_loader;
#[path = "pyrun_extension.rs"]
mod pyrun_extension;
#[path = "raw_extension_dialogs.rs"]
mod raw_extension_dialogs;
#[path = "raw_extension_ui_types.rs"]
mod raw_extension_ui_types;
