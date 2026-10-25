//! Opt-in native port of `bash-spawn-hook.ts`; register explicitly.
use pi_coding_agent::core::extensions::{ExtensionApi, HandlerError};
use pi_coding_agent::core::tools::bash::{BashToolOptions, create_bash_tool};
use pi_coding_agent::core::tools::tool_definition_wrapper::create_tool_definition_from_agent_tool;
use pi_js_compat::js_format;
use std::sync::Arc;

pub fn bash_spawn_hook_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let cwd = pi_js_compat::process_cwd::cwd().map_err(HandlerError::from)?;
    let tool = create_bash_tool(
        cwd,
        BashToolOptions {
            spawn_hook: Some(Arc::new(|mut context| {
                context.command = js_format!("source ~/.profile\n{}", context.command);
                context.env.insert("PI_SPAWN_HOOK".into(), Some("1".into()));
                Ok(context)
            })),
            ..Default::default()
        },
    );
    let definition = create_tool_definition_from_agent_tool(tool);
    pi.register_tool(definition)
}
