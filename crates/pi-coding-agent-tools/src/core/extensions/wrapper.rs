//! Tool wrappers for extension-registered tools (`core/extensions/wrapper.ts`).
//!
//! Tool-call and tool-result interception lives in the agent session; these
//! wrappers only adapt execution so extension tools receive the runner context.

use std::sync::Arc;

use pi_agent_core::agent::types::AgentTool;

use super::runner::ExtensionRunner;
use super::types::{ExtensionContext, HandlerError, RegisteredTool, ToolDefinition};
use crate::core::tools::tool_definition_wrapper::{ToolContextFactory, wrap_definition};

/// Wrap a registered extension tool into an `AgentTool`.
pub fn wrap_registered_tool(registered_tool: RegisteredTool, runner: &ExtensionRunner) -> Arc<dyn AgentTool> {
    wrap_definition_tool(registered_tool.definition, runner)
}

/// Wrap a bare tool definition into an `AgentTool` bound to the runner context.
pub fn wrap_definition_tool(definition: ToolDefinition, runner: &ExtensionRunner) -> Arc<dyn AgentTool> {
    let context_runner = runner.clone();
    let context_factory: ToolContextFactory = Arc::new(move || context_runner.create_context());
    wrap_definition(definition, Some(context_factory))
}

/// Wrap all registered extension tools into `AgentTool`s.
pub fn wrap_registered_tools(
    registered_tools: Vec<RegisteredTool>,
    runner: &ExtensionRunner,
) -> Vec<Arc<dyn AgentTool>> {
    registered_tools
        .into_iter()
        .map(|tool| wrap_registered_tool(tool, runner))
        .collect()
}

/// Adapt a `ContextTool<ExtensionContext>` into an extension `ToolDefinition`.
///
/// First-party coordination tools implement this Rust method adapter; the definition
/// shape is what `createAllToolDefinitions` exposes to renderers and extensions.
pub fn context_tool_definition<T>(tool: Arc<T>) -> ToolDefinition
where
    T: super::types::ContextTool<Option<ExtensionContext>> + 'static,
{
    let execute_tool = Arc::clone(&tool);
    let arguments_tool = Arc::clone(&tool);
    ToolDefinition {
        name: tool.name().to_owned(),
        label: tool.label(),
        description: tool.description(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: tool.parameters().clone(),
        render_shell: None,
        prepare_arguments: Some(Arc::new(move |args: &pi_js_compat::js_json::JsValue| {
            Ok(arguments_tool.prepare_arguments(args.clone()))
        })),
        prepare_execution: None,
        execution_mode: tool.execution_mode(),
        execute: Arc::new(move |tool_call_id, params, signal, on_update, context| {
            let tool = Arc::clone(&execute_tool);
            Box::pin(async move {
                tool.execute(tool_call_id, params, signal, on_update, context, None)
                    .await
                    .map_err(|error| HandlerError::new(error.message))
            })
        }),
        render_call: None,
        render_result: None,
    }
}
